//! Images on the GPU that free themselves with the component that made them.

use crownui::ext::ImageId;
use crownui::prelude::{Cx, Runtime, Size};
use lockscreen::tiny_skia::Pixmap;

use super::pixmap::straight_rgba;

/// One uploaded image. [`free`](Self::free) releases it at once; otherwise it
/// goes when the component that uploaded it unmounts.
pub struct Texture {
    pub image: ImageId,
    /// In physical pixels.
    pub size: Size,
    scope_end: Box<dyn FnOnce(&mut Runtime)>,
}

impl Texture {
    /// Hands straight-alpha `rgba` to the renderer.
    pub fn upload(cx: &mut Cx, width: u32, height: u32, rgba: Vec<u8>) -> Self {
        let owner = cx.current_scope();
        let scope = cx.create_child_scope(owner);
        let outer = cx.set_current_scope(scope);
        let image = cx.register_image(width, height, rgba);
        cx.release_image_on_cleanup(image);
        cx.set_current_scope(outer);
        Self {
            image,
            size: Size::new(width as f32, height as f32),
            scope_end: Box::new(move |runtime| runtime.dispose_scope(scope)),
        }
    }

    /// Uploads a premultiplied tiny-skia pixmap.
    pub fn from_pixmap(cx: &mut Cx, pixmap: &Pixmap) -> Self {
        Self::upload(cx, pixmap.width(), pixmap.height(), straight_rgba(pixmap))
    }

    pub fn free(self, runtime: &mut Runtime) {
        (self.scope_end)(runtime);
    }
}

#[cfg(test)]
mod tests {
    use crowntest::{Role, TestApp, role};
    use crownui::kit::button;
    use crownui::prelude::{Listeners, bind, component, show, vstack};

    use super::*;

    fn uploaded(cx: &mut Cx) -> Texture {
        Texture::upload(cx, 1, 1, vec![u8::MAX; 4])
    }

    #[test]
    fn a_texture_goes_when_freed_or_with_its_component() {
        let mut app = TestApp::new(component(|cx| {
            let mounted = cx.signal(true);
            let first = uploaded(cx);
            let held = cx.signal(Some(first));
            vstack((
                show(
                    bind(move |runtime| mounted.get(runtime)),
                    || {
                        component(|cx| {
                            let _owned_by_this_component = uploaded(cx);
                            vstack(())
                        })
                    },
                    || vstack(()),
                ),
                button("Free").on_click(move |cx, _| {
                    let mut taken = None;
                    held.update(cx, |held| taken = held.take());
                    if let Some(texture) = taken {
                        texture.free(cx);
                    }
                }),
                button("Unmount").on_click(move |cx, _| mounted.set(cx, false)),
            ))
        }));
        app.settle();
        let count = |app: &mut TestApp| app.inline().renderer().image_count();
        assert_eq!(count(&mut app), 2);

        app.click(role(Role::Button).name("Free")).settle();
        assert_eq!(count(&mut app), 1, "freeing releases at once");

        app.click(role(Role::Button).name("Unmount")).settle();
        assert_eq!(count(&mut app), 0, "unmounting releases what is left");
    }
}
