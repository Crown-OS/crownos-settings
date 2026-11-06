//! Turning studio results and the mode switch into frames on the canvas.

use std::time::Instant;

use crownos_config::schema::Lock;
use crownui::prelude::Cx;
use lockscreen::flow::Frame;
use lockscreen::tiny_skia::Pixmap;

use super::State;
use super::playback::{Mode, Playback};
use super::sprites::SpriteSet;
use super::studio::StudioEvent;
use crate::util::signal::set_if_changed;
use crate::util::texture::Texture;

impl State {
    pub(super) fn accept(self, cx: &mut Cx, event: StudioEvent) {
        match event {
            StudioEvent::Sprites {
                layout,
                scale,
                frame,
                sprites,
            } => {
                let now = Instant::now();
                let mut replaced = SpriteSet::default();
                self.sprites
                    .update(cx, |current| std::mem::swap(current, &mut replaced));
                let was_morphing = replaced.is_morphing(now);
                let mut uploaded =
                    SpriteSet::upload(cx, (layout, scale, frame), sprites, &mut replaced);
                uploaded.hand_over(cx, &mut replaced, now);
                let morphing = uploaded.is_morphing(now);
                self.sprites.set(cx, uploaded);
                replaced.free(cx);
                if morphing && !was_morphing {
                    self.morph_tick(cx);
                }
                if !self.animating.with_untracked(cx, |animating| *animating) {
                    let frame = self.frame.with_untracked(cx, |frame| *frame);
                    self.show(cx, frame);
                }
            }
            StudioEvent::Frosted { layout, faces } => self.refrost(cx, layout, faces),
            StudioEvent::Picture(slot, picture) => {
                let texture = picture.map(|picture| {
                    Texture::upload(cx, picture.width, picture.height, picture.rgba)
                });
                let mut replaced = None;
                self.backdrop
                    .update(cx, |backdrop| replaced = backdrop.replace(slot, texture));
                if let Some(texture) = replaced {
                    texture.free(cx);
                }
            }
            StudioEvent::Thumbnails(shelf, pictures) => self.shelve(cx, shelf, pictures),
            StudioEvent::PictureUnreadable(unreadable) => {
                set_if_changed(cx, self.unreadable, unreadable);
            }
            StudioEvent::Depth(depth) => set_if_changed(cx, self.depth, depth),
        }
    }

    /// Swaps in glass faces frosted again for where the clock is shown, if
    /// they are of the layout on the canvas.
    fn refrost(self, cx: &mut Cx, layout: u64, faces: Vec<(u64, Pixmap)>) {
        if !self
            .sprites
            .with_untracked(cx, |sprites| sprites.is_layout(layout))
        {
            return;
        }
        let made = faces
            .into_iter()
            .map(|(id, pixmap)| {
                let texture = Texture::from_pixmap(cx, &pixmap);
                (id, pixmap, texture)
            })
            .collect();
        let mut unused = Vec::new();
        self.sprites
            .update(cx, |sprites| unused = sprites.refrost(made));
        for texture in unused {
            texture.free(cx);
        }
        let frame = self.frame.with_untracked(cx, |frame| *frame);
        self.show(cx, frame);
    }

    /// Plays the lock's own animation toward `mode`.
    pub(super) fn show_mode(self, cx: &mut Cx, mode: Mode) {
        let now = Instant::now();
        let lock = cx.untrack(|runtime| self.store.section(runtime, Lock::clone));
        self.playback
            .update(cx, |playback| playback.show(&lock, mode, now));
        let mode = self.playback.with_untracked(cx, Playback::mode);
        set_if_changed(cx, self.mode, mode);
        if !self.animating.with_untracked(cx, |animating| *animating) {
            self.animating.set(cx, true);
            self.tick(cx);
        }
    }

    /// One frame of whatever is playing, and the next while it lasts.
    fn tick(self, cx: &mut Cx) {
        let now = Instant::now();
        let (frame, animating) = self.playback.with_untracked(cx, |playback| {
            (playback.frame(now), playback.is_animating(now))
        });
        self.show(cx, frame);
        if animating {
            cx.request_animation_frame(move |cx, _| self.tick(cx));
            return;
        }
        self.animating.set(cx, false);
        let mut unused = Vec::new();
        self.sprites
            .update(cx, |sprites| unused = sprites.unused(&frame));
        for texture in unused {
            texture.free(cx);
        }
    }

    /// One frame of the digits changing, and the next while they do.
    fn morph_tick(self, cx: &mut Cx) {
        let now = Instant::now();
        self.morph_clock.set(cx, now);
        if self
            .sprites
            .with_untracked(cx, |sprites| sprites.is_morphing(now))
        {
            cx.request_animation_frame(move |cx, _| self.morph_tick(cx));
            return;
        }
        let mut unused = Vec::new();
        self.sprites
            .update(cx, |sprites| unused = sprites.settle(now));
        for texture in unused {
            texture.free(cx);
        }
    }

    /// Puts `frame` on screen, blurring any sprite it needs blurred first.
    pub(super) fn show(self, cx: &mut Cx, frame: Frame) {
        let missing = self
            .sprites
            .with_untracked(cx, |sprites| sprites.missing(&frame));
        if !missing.is_empty() {
            let made: Vec<_> = missing
                .into_iter()
                .map(|(index, radius, pixmap)| (index, radius, Texture::from_pixmap(cx, &pixmap)))
                .collect();
            self.sprites.update(cx, |sprites| sprites.adopt(made));
        }
        set_if_changed(cx, self.frame, frame);
    }
}
