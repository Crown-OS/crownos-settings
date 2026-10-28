//! Installing from a folder, a `.crownpkg` file or a git URL.

use crownui::kit::{
    ButtonVariant, CalloutOptions, TextInputOptions, Tone, callout, icons, spinner, text_input,
};
use crownui::prelude::{
    Cx, Listeners, Runtime, Semantic, Signal, Styled, View, bind, spacer, vstack,
};

use super::parts::row_button;
use crate::layout::{
    optional, setting_row, setting_row_content, setting_row_desc, settings_card_titled,
    settings_divider, spread, when,
};
use crate::net::plugins::{PluginsState, Source};
use crate::util::home::expand_home;

const SOURCE_WIDTH: f32 = 300.0;
const BRANCH_WIDTH: f32 = 180.0;
const ROW_SPINNER: f32 = 14.0;

pub fn card(cx: &mut Cx, state: Signal<PluginsState>) -> impl View + use<> {
    let typed = cx.signal(String::new());
    let branch = cx.signal(String::new());

    let source_field = text_input(typed)
        .placeholder("~/plugins/clock, clock.crownpkg or https://…")
        .width(SOURCE_WIDTH)
        .label("Install from")
        .on_submit(move |cx, _| install(cx, state, typed, branch));
    let branch_row = when(
        move |runtime| typed.with(runtime, |typed| is_git_url(typed)),
        move |_| {
            vstack((
                settings_divider(),
                setting_row(
                    "Branch",
                    text_input(branch)
                        .placeholder("Default branch")
                        .width(BRANCH_WIDTH)
                        .label("Branch"),
                ),
            ))
        },
    );
    let busy = move |runtime: &mut Runtime| state.with(runtime, |state| state.installing);
    let install_button = row_button("Install", ButtonVariant::Primary)
        .disabled(bind(move |runtime| {
            busy(runtime) || typed.with(runtime, |typed| typed.trim().is_empty())
        }))
        .on_click(move |cx, _| install(cx, state, typed, branch));

    settings_card_titled(
        "Install",
        (
            setting_row_desc(
                "Source",
                "A package folder, a .crownpkg file or an https git URL",
                source_field,
            ),
            branch_row,
            settings_divider(),
            setting_row_content(spread((
                when(busy, |_| spinner().size(ROW_SPINNER)),
                spacer(),
                install_button,
            ))),
            optional(
                move |runtime| state.with(runtime, |state| state.install_error.clone()),
                |_, message| {
                    setting_row_content(
                        callout(Tone::Danger, "Could not install")
                            .body(message)
                            .icon(icons::CIRCLE_ALERT),
                    )
                },
            ),
        ),
    )
}

fn install(
    runtime: &mut Runtime,
    state: Signal<PluginsState>,
    typed: Signal<String>,
    branch: Signal<String>,
) {
    let typed = typed.with(runtime, Clone::clone);
    let branch = branch.with(runtime, Clone::clone);
    let Some(source) = source_of(&typed, &branch) else {
        return;
    };
    if state.with(runtime, |state| state.installing) {
        return;
    }
    state.update(runtime, |state| state.install(source));
}

fn is_git_url(typed: &str) -> bool {
    typed.trim_start().starts_with("https://")
}

/// What the daemon is asked to install, or `None` for an empty field.
fn source_of(typed: &str, branch: &str) -> Option<Source> {
    let typed = typed.trim();
    if typed.is_empty() {
        return None;
    }
    if is_git_url(typed) {
        let branch = branch.trim();
        return Some(Source::Git {
            url: typed.to_owned(),
            rev: (!branch.is_empty()).then(|| branch.to_owned()),
        });
    }
    Some(Source::Path(expand_home(typed).into_owned()))
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_typed_source_becomes_what_the_daemon_installs() {
        assert_eq!(source_of("  ", "main"), None);
        assert_eq!(
            source_of("/opt/clock.crownpkg", "main"),
            Some(Source::Path("/opt/clock.crownpkg".to_owned())),
            "a branch means nothing for a path"
        );
        assert_eq!(
            source_of(" https://github.com/crown-os/clock ", ""),
            Some(Source::Git {
                url: "https://github.com/crown-os/clock".to_owned(),
                rev: None,
            })
        );
        assert_eq!(
            source_of("https://github.com/crown-os/clock", " dev "),
            Some(Source::Git {
                url: "https://github.com/crown-os/clock".to_owned(),
                rev: Some("dev".to_owned()),
            })
        );
    }
}
