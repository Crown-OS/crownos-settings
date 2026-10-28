//! What the studio is asked to paint, kept in step with the settings, the
//! monitors and the canvas.

use crownos_config::schema::lock::{Background, BlurKey, Dim};
use crownos_config::schema::{ClockFrame, Lock, Theme, appearance};
use crownui::prelude::{Cx, Runtime};
use lockscreen::lockscene::LockScene;
use tokio::sync::mpsc::UnboundedSender;

use super::studio::{BackdropRequest, LayoutRequest, StudioCommand};
use super::{FALLBACK_OUTPUT, State};

impl State {
    /// Keeps the studio's requests in step with the settings and the canvas.
    pub(super) fn follow(self, cx: &mut Cx, commands: UnboundedSender<StudioCommand>) {
        let layout = cx.memo(move |runtime| self.layout_request(runtime));
        let backdrop = cx.memo(move |runtime| self.backdrop_request(runtime));
        let sender = commands.clone();
        cx.effect(move |runtime| {
            if let Some(request) = layout.get(runtime) {
                let _ = sender.send(StudioCommand::Layout(Box::new(request)));
            }
        });
        cx.effect(move |runtime| {
            if let Some(request) = backdrop.get(runtime) {
                let _ = commands.send(StudioCommand::Backdrop(request));
            }
        });
    }

    fn layout_request(self, runtime: &mut Runtime) -> Option<LayoutRequest> {
        let size = self.size.get(runtime);
        if size.width < 1.0 {
            return None;
        }
        let output = self.output(runtime);
        let scale = size.width / output[0] * self.device_scale.get(runtime);
        let config = self.store.section(runtime, laid_out);
        let theme = self.store.section(runtime, Theme::clone);
        Some(LayoutRequest {
            scene: LockScene {
                config,
                theme,
                ..LockScene::default()
            },
            output,
            scale,
        })
    }

    fn backdrop_request(self, runtime: &mut Runtime) -> Option<BackdropRequest> {
        let size = self.size.get(runtime);
        let device_scale = self.device_scale.get(runtime);
        let pixels = [size.width, size.height].map(|side| (side * device_scale).round() as u32);
        if pixels.contains(&0) {
            return None;
        }
        Some(BackdropRequest {
            desktop: self.store.get(runtime, appearance::Wallpaper),
            background: self.store.get(runtime, Background),
            pixels,
            output: self.output(runtime).map(|side| side as u32),
            blur: self.store.get(runtime, BlurKey),
            depth: self
                .store
                .section(runtime, |lock: &Lock| lock.clock.depth.clone()),
            dim: self.store.get(runtime, Dim),
        })
    }

    /// The logical size of the first monitor in use.
    pub(super) fn output(self, runtime: &mut Runtime) -> [f32; 2] {
        self.store
            .outputs
            .with(runtime, |outputs| {
                outputs
                    .monitors
                    .iter()
                    .filter(|monitor| monitor.enabled)
                    .map(|monitor| monitor.logical_size())
                    .find(|(width, height)| *width > 0 && *height > 0)
            })
            .map_or(FALLBACK_OUTPUT, |(width, height)| {
                [width as f32, height as f32]
            })
    }
}

/// Only what the layout reads, so editing anything else — the background,
/// the transitions, the clock's warp — does not lay the screen out again.
fn laid_out(lock: &Lock) -> Lock {
    let mut clock = lock.clock.clone();
    clock.frame.warp = ClockFrame::default().warp;
    clock.depth = None;
    Lock {
        clock_style: lock.clock_style,
        clock,
        time_format: lock.time_format.clone(),
        date_format: lock.date_format.clone(),
        font: lock.font.clone(),
        layout: lock.layout,
        auth_field: lock.auth_field,
        slots: lock.slots.clone(),
        show_user: lock.show_user,
        ..Lock::default()
    }
}
