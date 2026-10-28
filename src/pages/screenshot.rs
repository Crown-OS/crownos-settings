//! The Screenshot & Recording page: where captures are saved and how recordings are encoded.

use std::path::Path;

use crownos_config::Key;
use crownos_config::schema::screenshot::{
    Codec, Container, CountdownSeconds, Format, FrameRate, Quality, RecordingDirectory,
    ScreenshotDirectory,
};
use crownos_config::schema::{
    ImageFormat, RecordingQuality, VideoCodec, VideoContainer, screenshot,
};
use crownui::kit::{TextInputOptions, icons};
use crownui::prelude::{AnyView, Cx, Semantic, View};

use crate::controls::{Options, choice, field_text, switch};
use crate::layout::{setting_row, setting_row_desc, settings_card_titled, settings_divider};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use crate::util::home::expand_home;

const PATH_WIDTH: f32 = 280.0;

static IMAGE_FORMATS: Options<ImageFormat> =
    Options::new(&[("PNG", ImageFormat::Png), ("JPEG", ImageFormat::Jpeg)]);

static CODECS: Options<VideoCodec> = Options::new(&[
    ("H.264", VideoCodec::H264),
    ("HEVC", VideoCodec::Hevc),
    ("AV1", VideoCodec::Av1),
]);

static CONTAINERS: Options<VideoContainer> = Options::new(&[
    ("MP4", VideoContainer::Mp4),
    ("Matroska", VideoContainer::Mkv),
]);

static FRAME_RATES: Options<u32> = Options::new(&[
    ("24 fps", 24),
    ("30 fps", 30),
    ("60 fps", 60),
    ("120 fps", 120),
]);

static QUALITIES: Options<RecordingQuality> = Options::new(&[
    ("Low", RecordingQuality::Low),
    ("Medium", RecordingQuality::Medium),
    ("High", RecordingQuality::High),
    ("Near lossless", RecordingQuality::Lossless),
]);

static COUNTDOWNS: Options<u32> = Options::new(&[
    ("None", 0),
    ("3 seconds", 3),
    ("5 seconds", 5),
    ("10 seconds", 10),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Screenshot & Recording",
    slug: "screenshot",
    icon: icons::CAMERA,
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    page(
        &PAGE,
        (
            settings_card_titled(
                "Screenshots",
                (
                    setting_row_desc(
                        "Save to",
                        "Leave empty for Pictures/Screenshots",
                        folder_field(cx, store, ScreenshotDirectory, "~/Pictures/Screenshots"),
                    ),
                    settings_divider(),
                    setting_row("Format", choice(cx, store, Format, &IMAGE_FORMATS)),
                ),
            ),
            settings_card_titled(
                "Recordings",
                (
                    setting_row_desc(
                        "Save to",
                        "Leave empty for Videos/Recordings",
                        folder_field(cx, store, RecordingDirectory, "~/Videos/Recordings"),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Codec",
                        "Encoded on the GPU when it supports the codec",
                        choice(cx, store, Codec, &CODECS),
                    ),
                    settings_divider(),
                    setting_row("Container", choice(cx, store, Container, &CONTAINERS)),
                    settings_divider(),
                    setting_row("Frame rate", choice(cx, store, FrameRate, &FRAME_RATES)),
                    settings_divider(),
                    setting_row("Quality", choice(cx, store, Quality, &QUALITIES)),
                    settings_divider(),
                    setting_row(
                        "Countdown",
                        choice(cx, store, CountdownSeconds, &COUNTDOWNS),
                    ),
                    settings_divider(),
                    setting_row("Show pointer", switch(cx, store, screenshot::ShowCursor)),
                ),
            ),
            settings_card_titled(
                "Audio",
                (
                    setting_row(
                        "Record microphone",
                        switch(cx, store, screenshot::CaptureMicrophone),
                    ),
                    settings_divider(),
                    setting_row(
                        "Record system audio",
                        switch(cx, store, screenshot::CaptureSystemAudio),
                    ),
                ),
            ),
        ),
    )
}

/// A folder path: empty means the default, `~` is expanded, relative paths are not saved.
fn folder_field<K>(
    cx: &mut Cx,
    store: Store,
    key: K,
    placeholder: &'static str,
) -> impl View + use<K>
where
    K: Key<Section = crownos_config::schema::Screenshot, Value = Option<String>> + Copy + 'static,
{
    field_text(
        cx,
        move |runtime| store.get(runtime, key).unwrap_or_default(),
        move |runtime, typed| {
            let typed = typed.trim();
            if typed.is_empty() {
                store.set(runtime, key, None);
                return;
            }
            let path = expand_home(typed);
            if Path::new(path.as_ref()).is_absolute() {
                store.set(runtime, key, Some(path.into_owned()));
            }
        },
    )
    .placeholder(placeholder)
    .width(PATH_WIDTH)
    .label("Folder")
}
