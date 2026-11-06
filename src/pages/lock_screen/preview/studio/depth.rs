//! The depth effect's mask for the lock's background: looked up in the cache,
//! or cut by `crown-depthcut` in a process of its own, one picture at a time.
//! A picture the helper could not cut is not tried again.

use std::path::{Path, PathBuf};
use std::process::Child;

use lockscreen::image::{background_zoom, hides_clock};

use super::{DepthState, Studio, StudioEvent};

impl Studio {
    /// The mask cut for `lock`, when depth is `wanted`, starting the cut when
    /// there is none yet; notes how far the cut has got.
    pub(super) fn depth_mask(&mut self, lock: Option<&str>, wanted: bool) -> Option<PathBuf> {
        let (state, mask) = match lock.map(Path::new) {
            _ if !wanted => (DepthState::Off, None),
            None => (DepthState::NoPicture, None),
            Some(image) => match depthcut::cached_mask(image) {
                Some(mask) => (DepthState::Ready, Some(mask)),
                None => (self.cut(image), None),
            },
        };
        self.depth = state;
        mask
    }

    /// How far the depth effect has got, for the canvas: a subject that would
    /// hide too much of the clock where it now sits is held back.
    pub(super) fn told_depth(&self) -> StudioEvent {
        let shown = (&self.subject, self.clock_area, &self.backdrop_request);
        let state = match (self.depth, shown) {
            (DepthState::Ready, (Some(subject), Some(clock), Some(request)))
                if hides_clock(
                    subject,
                    clock,
                    request.output.map(|side| side as f32),
                    background_zoom(&request.background),
                ) =>
            {
                DepthState::HeldBack
            }
            (state, _) => state,
        };
        StudioEvent::Depth(state)
    }

    fn cut(&mut self, image: &Path) -> DepthState {
        if self.cut_failed.as_deref() == Some(image) {
            return DepthState::Unavailable;
        }
        if self.cutting.is_some() {
            return DepthState::Cutting;
        }
        match depthcut::spawn_cut(image) {
            Ok(child) => {
                self.cutting = Some(image.to_owned());
                self.started_cut = Some(child);
                DepthState::Cutting
            }
            Err(_) => {
                self.cut_failed = Some(image.to_owned());
                DepthState::Unavailable
            }
        }
    }

    /// The cut under way has ended: a picture it left without a mask is
    /// marked as one that cannot be cut.
    pub(super) fn cut_ended(&mut self) {
        if let Some(image) = self.cutting.take()
            && depthcut::cached_mask(&image).is_none()
        {
            self.cut_failed = Some(image);
        }
    }
}

/// Waits for the helper to exit; for the blocking pool.
pub(super) fn wait_for(mut cut: Child) {
    let _ = cut.wait();
}

#[cfg(test)]
mod tests {
    use std::time::Duration;

    use crownos_config::schema::LockBackground;
    use lockscreen::layout::Rect;
    use lockscreen::tiny_skia::Pixmap;
    use tokio::sync::mpsc::unbounded_channel;

    use super::super::{BackdropRequest, Slot, StudioCommand, run};
    use super::*;
    use crate::util::worker::MessageProxy;

    /// The longest a cut may take before the test gives up on it.
    const CUT_PATIENCE: Duration = Duration::from_secs(120);

    fn depth_of(studio: &Studio) -> DepthState {
        match studio.told_depth() {
            StudioEvent::Depth(state) => state,
            _ => DepthState::Off,
        }
    }

    #[test]
    fn a_flat_color_has_no_subject_and_depth_off_cuts_nothing() {
        let mut studio = Studio::default();
        let request = BackdropRequest {
            desktop: String::new(),
            background: LockBackground::Token("accent.end".to_owned()),
            pixels: [64, 36],
            output: [1920, 1080],
            depth: true,
            dim: 0.0,
        };
        let mut events = Vec::new();
        studio.backdrop(&request, &mut events);
        assert_eq!(depth_of(&studio), DepthState::NoPicture);

        events.clear();
        let off = BackdropRequest {
            depth: false,
            background: LockBackground::Image("/nonexistent/lock.png".to_owned()),
            ..request
        };
        studio.backdrop(&off, &mut events);
        assert_eq!(depth_of(&studio), DepthState::Off);
        assert!(studio.started_cut.is_none());
    }

    #[test]
    fn a_subject_that_would_hide_the_clock_is_held_back() {
        let mut subject = Pixmap::new(40, 20).expect("size");
        subject.data_mut()[..40 * 10 * 4].fill(u8::MAX);
        let mut studio = Studio {
            depth: DepthState::Ready,
            subject: Some(subject),
            backdrop_request: Some(BackdropRequest {
                desktop: String::new(),
                background: LockBackground::Image("lock.png".to_owned()),
                pixels: [40, 20],
                output: [40, 20],
                depth: true,
                dim: 0.0,
            }),
            ..Studio::default()
        };
        let clock = |y| Rect {
            x: 5.0,
            y,
            width: 10.0,
            height: 6.0,
        };
        studio.clock_area = Some(clock(2.0));
        assert_eq!(
            depth_of(&studio),
            DepthState::HeldBack,
            "under the top half"
        );
        studio.clock_area = Some(clock(12.0));
        assert_eq!(depth_of(&studio), DepthState::Ready, "clear of it");
    }

    /// With `crown-depthcut` on `PATH` and `$CROWNSETTINGS_DEPTH_PICTURE`
    /// naming a picture with no mask cached yet, the studio cuts it in the
    /// helper and then sends its subject.
    #[tokio::test]
    #[ignore = "runs the depth helper and its model"]
    async fn a_missing_mask_is_cut_by_the_helper() {
        let Some(picture) = std::env::var_os("CROWNSETTINGS_DEPTH_PICTURE") else {
            return;
        };
        let (proxy, mut events) = unbounded_channel();
        let (commands, receiver) = unbounded_channel();
        let studio = tokio::spawn(run(MessageProxy::from(proxy), receiver));
        let request = BackdropRequest {
            desktop: String::new(),
            background: LockBackground::Image(picture.to_string_lossy().into_owned()),
            pixels: [320, 180],
            output: [1920, 1080],
            depth: true,
            dim: 0.0,
        };
        commands
            .send(StudioCommand::Backdrop(request))
            .expect("the studio is listening");
        let (mut states, mut subject) = (Vec::new(), false);
        while let Ok(Some(event)) = tokio::time::timeout(CUT_PATIENCE, events.recv()).await {
            match event {
                StudioEvent::Depth(state) => {
                    states.push(state);
                    if matches!(state, DepthState::Ready | DepthState::Unavailable) {
                        break;
                    }
                }
                StudioEvent::Picture(Slot::Subject, picture) => subject = picture.is_some(),
                _ => {}
            }
        }
        assert_eq!(states.first(), Some(&DepthState::Cutting));
        assert_eq!(states.last(), Some(&DepthState::Ready));
        assert!(subject, "the subject arrives once the cut is in");
        drop(commands);
        studio.await.expect("the loop ends cleanly");
    }
}
