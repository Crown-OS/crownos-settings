//! Turning studio results and play requests into frames on the canvas.

use std::time::Instant;

use crownos_config::schema::Lock;
use crownui::prelude::Cx;
use lockscreen::flow::Frame;

use super::playback::Playback;
use super::sprites::SpriteSet;
use super::studio::StudioEvent;
use super::{Request, State, UNLOCKED_HOLD};
use crate::pages::lock_screen::moment::Moment;
use crate::util::texture::Texture;

impl State {
    pub(super) fn accept(self, cx: &mut Cx, event: StudioEvent) {
        match event {
            StudioEvent::Sprites {
                scale,
                frame,
                sprites,
            } => {
                let now = Instant::now();
                let mut uploaded = SpriteSet::upload(cx, scale, frame, sprites);
                let mut replaced = SpriteSet::default();
                self.sprites
                    .update(cx, |current| std::mem::swap(current, &mut replaced));
                let was_morphing = replaced.is_morphing(now);
                let morphs = cx.untrack(|runtime| {
                    self.store
                        .section(runtime, |lock: &Lock| lock.clock.morph_digits)
                });
                if morphs {
                    uploaded.hand_over(cx, &mut replaced, now);
                }
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
            StudioEvent::PictureUnreadable(unreadable) => {
                if self
                    .unreadable
                    .with_untracked(cx, |current| *current != unreadable)
                {
                    self.unreadable.set(cx, unreadable);
                }
            }
        }
    }

    pub(super) fn handle(self, cx: &mut Cx, request: Request) {
        let now = Instant::now();
        let lock = cx.untrack(|runtime| self.store.section(runtime, Lock::clone));
        self.playback.update(cx, |playback| match request {
            Request::Play(moment) => playback.play(&lock, moment, now),
            Request::Show(mode) => playback.show(&lock, mode, now),
        });
        let mode = self.playback.with_untracked(cx, Playback::mode);
        if self.mode.with_untracked(cx, |current| *current != mode) {
            self.mode.set(cx, mode);
        }
        if !self.animating.with_untracked(cx, |animating| *animating) {
            self.animating.set(cx, true);
            self.tick(cx);
        }
    }

    /// One frame of whatever is playing, and the next while it lasts.
    fn tick(self, cx: &mut Cx) {
        let now = Instant::now();
        let (frame, animating, unlocked, generation) =
            self.playback.with_untracked(cx, |playback| {
                (
                    playback.frame(now),
                    playback.is_animating(now),
                    playback.is_unlocked(now),
                    playback.generation(),
                )
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
        if unlocked {
            cx.spawn(tokio::time::sleep(UNLOCKED_HOLD), move |cx, ()| {
                let current = self.playback.with_untracked(cx, Playback::generation);
                if current == generation {
                    self.handle(cx, Request::Play(Moment::Appear));
                }
            });
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
        if self.frame.with_untracked(cx, |current| *current != frame) {
            self.frame.set(cx, frame);
        }
    }
}
