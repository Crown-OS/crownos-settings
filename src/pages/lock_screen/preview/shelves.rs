//! The thumbnails of other looks and faces, on the GPU, and the canvases
//! that show them: a look over the lock's own background, a face on its own.

use crownui::ext::DrawList;
use crownui::prelude::{
    Color, CornerRadii, Cx, Point, Rect, Role, Runtime, Semantic, Size, Styled, View, bind, canvas,
};

use super::compose::{CORNER_RADIUS, background};
use super::studio::{Picture, Shelf};
use super::{Preview, State};
use crate::util::texture::Texture;

/// Behind a face, whatever the background: ColorOS's grey clock tile.
const FACE_GROUND: Color = Color::new(0.294, 0.294, 0.286, 1.0);

#[derive(Default)]
pub struct Shelves {
    pictures: [Vec<Option<Texture>>; Shelf::ALL.len()],
}

impl Shelves {
    fn get(&self, shelf: Shelf, index: usize) -> Option<&Texture> {
        self.pictures[shelf as usize].get(index)?.as_ref()
    }
}

impl State {
    pub(super) fn shelve(self, cx: &mut Cx, shelf: Shelf, pictures: Vec<Option<Picture>>) {
        let mut uploaded: Vec<_> = pictures
            .into_iter()
            .map(|picture| {
                picture
                    .map(|picture| Texture::upload(cx, picture.width, picture.height, picture.rgba))
            })
            .collect();
        self.shelves.update(cx, |shelves| {
            std::mem::swap(&mut shelves.pictures[shelf as usize], &mut uploaded);
        });
        for texture in uploaded.into_iter().flatten() {
            texture.free(cx);
        }
    }

    fn paint_look(self, runtime: &mut Runtime, index: usize, size: Size, list: &mut DrawList) {
        let look = self.look(runtime);
        let bounds = Rect::new(Point::ZERO, size);
        list.save();
        list.clip(bounds, CornerRadii::uniform(CORNER_RADIUS));
        runtime.with2(self.backdrop, self.shelves, |backdrop, shelves| {
            background(list, bounds, &look, backdrop, 1.0);
            if let Some(thumbnail) = shelves.get(Shelf::Looks, index) {
                list.image(thumbnail.image, bounds);
            }
        });
        list.restore();
    }

    fn paint_face(self, runtime: &mut Runtime, index: usize, size: Size, list: &mut DrawList) {
        let bounds = Rect::new(Point::ZERO, size);
        list.fill(bounds, CornerRadii::uniform(CORNER_RADIUS), FACE_GROUND);
        self.shelves.with(runtime, |shelves| {
            if let Some(thumbnail) = shelves.get(Shelf::Faces, index) {
                list.image(thumbnail.image, contained(thumbnail.size, size));
            }
        });
    }
}

/// `picture` as large as fits inside `size`, centred.
fn contained(picture: Size, size: Size) -> Rect {
    let scale = (size.width / picture.width).min(size.height / picture.height);
    let (width, height) = (picture.width * scale, picture.height * scale);
    Rect::from_xywh(
        (size.width - width) / 2.0,
        (size.height - height) / 2.0,
        width,
        height,
    )
}

impl Preview {
    /// The lock screen as it would look in `LOOKS[index]`.
    pub fn look_picture(&self, index: usize) -> impl View + use<> {
        let state = self.state;
        canvas(move |runtime, size, list| state.paint_look(runtime, index, size, list))
            .w_full()
            .aspect(bind(move |runtime| {
                let [width, height] = state.output(runtime);
                width / height
            }))
            .rounded(CORNER_RADIUS)
            .role(Role::Image)
    }

    /// The clock in `FACES[index]`, `height` logical pixels tall.
    pub fn face_picture(&self, index: usize, height: f32) -> impl View + use<> {
        let state = self.state;
        canvas(move |runtime, size, list| state.paint_face(runtime, index, size, list))
            .w_full()
            .h(height)
            .rounded(CORNER_RADIUS)
            .role(Role::Image)
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_picture_fits_inside_centred() {
        let fitted = contained(Size::new(200.0, 400.0), Size::new(100.0, 100.0));
        assert_eq!(fitted, Rect::from_xywh(25.0, 0.0, 50.0, 100.0));
    }
}
