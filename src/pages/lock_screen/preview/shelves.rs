//! The Clock & colour sheet's pictures on the GPU, and the canvases that
//! show them: a layout over the whole screen, a font as the hours alone.

use crownui::ext::DrawList;
use crownui::prelude::{
    Cx, Point, Rect, Role, Runtime, Semantic, Size, Styled, View, bind, canvas,
};

use super::studio::{Picture, Shelf};
use super::{Preview, State};
use crate::util::texture::Texture;

/// Width over height of a layout card before its picture arrives.
const CARD_ASPECT: f32 = 0.75;

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

    fn paint_shelf(
        self,
        runtime: &mut Runtime,
        shelf: Shelf,
        index: usize,
        size: Size,
        list: &mut DrawList,
    ) {
        self.shelves.with(runtime, |shelves| {
            if let Some(picture) = shelves.get(shelf, index) {
                list.image(picture.image, contained(picture.size, size));
            }
        });
    }
}

/// `picture` as large as fits inside `size`, centred.
fn contained(picture: Size, size: Size) -> Rect {
    let scale = (size.width / picture.width).min(size.height / picture.height);
    let (width, height) = (picture.width * scale, picture.height * scale);
    Rect::new(
        Point::new((size.width - width) / 2.0, (size.height - height) / 2.0),
        Size::new(width, height),
    )
}

impl Preview {
    /// The clock in the sheet's layout `index`, on a card its picture's shape.
    pub fn layout_picture(&self, index: usize) -> impl View + use<> {
        let state = self.state;
        canvas(move |runtime, size, list| {
            state.paint_shelf(runtime, Shelf::Layouts, index, size, list);
        })
        .w_full()
        .aspect(bind(move |runtime| {
            state.shelves.with(runtime, |shelves| {
                shelves
                    .get(Shelf::Layouts, index)
                    .map_or(CARD_ASPECT, |picture| {
                        picture.size.width / picture.size.height
                    })
            })
        }))
        .role(Role::Image)
    }

    /// The hours in the sheet's font `index`, `height` logical pixels tall.
    pub fn font_picture(&self, index: usize, height: f32) -> impl View + use<> {
        let state = self.state;
        canvas(move |runtime, size, list| {
            state.paint_shelf(runtime, Shelf::Fonts, index, size, list);
        })
        .w_full()
        .h(height)
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
