//! The Screen Context page: whether the context engine observes, how long it
//! remembers, and what it never looks at.

use crownos_config::schema::VisualContext;
use crownos_config::schema::visual_context::{
    Enabled, ExcludedApps, PauseHotkey, PrivateTitles, RetentionDays,
};
use crownui::kit::{CalloutOptions, TextInputOptions, Tone, callout, icons};
use crownui::prelude::{AnyView, Cx, View};

use crate::controls::{Options, choice, field_text, string_list, switch};
use crate::layout::{
    nothing, setting_row, setting_row_desc, settings_card_titled, settings_divider,
};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use crate::util::strict_section::problem_with;

const SHORTCUT_WIDTH: f32 = 200.0;

static RETENTION: Options<u32> = Options::new(&[
    ("1 week", 7),
    ("2 weeks", 14),
    ("30 days", 30),
    ("90 days", 90),
    ("1 year", 365),
]);

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Screen Context",
    slug: "screen-context",
    icon: icons::SCAN_EYE,
    keywords: &[
        "context engine",
        "privacy",
        "observe",
        "memory",
        "history",
        "excluded apps",
        "private browsing",
        "incognito",
        "pause",
    ],
    build,
};

fn build(cx: &mut Cx, store: Store) -> AnyView {
    page(
        &PAGE,
        (
            broken_file_notice(),
            settings_card_titled(
                "Observation",
                (
                    setting_row_desc(
                        "Context engine",
                        "Remembers which windows you use. It never keeps screen pixels.",
                        switch(cx, store, Enabled),
                    ),
                    settings_divider(),
                    setting_row(
                        "Keep history for",
                        choice(cx, store, RetentionDays, &RETENTION),
                    ),
                    settings_divider(),
                    setting_row_desc(
                        "Pause shortcut",
                        "Pauses and resumes observation",
                        field_text(
                            cx,
                            move |runtime| store.get(runtime, PauseHotkey),
                            move |runtime, typed| {
                                let typed = typed.trim();
                                if !typed.is_empty() {
                                    store.set(runtime, PauseHotkey, typed.to_owned());
                                }
                            },
                        )
                        .width(SHORTCUT_WIDTH),
                    ),
                ),
            ),
            settings_card_titled(
                "Apps that are never observed",
                string_list(
                    cx,
                    store,
                    ExcludedApps,
                    "Add an app id, such as org.keepassxc.*",
                ),
            ),
            settings_card_titled(
                "Private window titles",
                string_list(
                    cx,
                    store,
                    PrivateTitles,
                    "Add a pattern, such as (?i)incognito",
                ),
            ),
        ),
    )
}

/// A warning while `visual_context.ron` does not parse; nothing is saved then.
fn broken_file_notice() -> AnyView {
    match problem_with::<VisualContext>(VisualContext::SECTION) {
        Some(problem) => callout(Tone::Warning, "visual_context.ron does not parse")
            .body(format!(
                "Fix the file by hand; changes here are not saved until then. {problem}"
            ))
            .icon(icons::TRIANGLE_ALERT)
            .boxed(),
        None => nothing().boxed(),
    }
}
