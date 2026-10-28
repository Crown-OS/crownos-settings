use crownui::prelude::{Cx, Signal};

use crate::net::wallpaper::WallpaperState;
use crate::pages;
use crate::state::Store;

/// Which page the content pane shows, and the bookkeeping a switch owes.
#[derive(Debug, Clone, Copy)]
pub struct Navigation {
    page: Signal<usize>,
    wallpaper: Signal<WallpaperState>,
}

impl Navigation {
    /// Open the page at `ordinal` — paying for the wallpaper picker straight
    /// away should that be the one.
    pub fn new(cx: &mut Cx, store: Store, ordinal: usize) -> Self {
        let navigation = Self {
            page: cx.signal(ordinal),
            wallpaper: store.wallpaper,
        };
        if pages::shows_wallpapers(ordinal) {
            navigation.wallpaper.update(cx, WallpaperState::enter);
        }
        navigation
    }

    /// The ordinal of the page on screen.
    pub fn current(self) -> Signal<usize> {
        self.page
    }

    /// Switch the content pane to the page at `ordinal`.
    ///
    /// Thumbnails cost memory, so the picker is entered and left on the edges
    /// only. Leaving releases every texture at once; the renderer frees each
    /// as soon as the page sliding out stops drawing it.
    pub fn show(self, cx: &mut Cx, ordinal: usize) {
        let previous = self.page.with_untracked(cx, |page| *page);
        if previous == ordinal {
            return;
        }
        self.page.set(cx, ordinal);
        match (
            pages::shows_wallpapers(previous),
            pages::shows_wallpapers(ordinal),
        ) {
            (false, true) => self.wallpaper.update(cx, WallpaperState::enter),
            (true, false) => {
                let mut released = Vec::new();
                self.wallpaper
                    .update(cx, |wallpaper| released = wallpaper.leave());
                for image in released {
                    cx.release_image(image);
                }
            }
            _ => {}
        }
    }
}
