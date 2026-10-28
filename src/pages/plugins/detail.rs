//! One theme or plugin: what it is, what it may do, and what it said.

use crownui::kit::{ButtonOptions, ButtonVariant, StatusOptions, Tone, icons, status, toggle};
use crownui::prelude::{
    AnyView, Cx, Listeners, Runtime, Semantic, Signal, Styled, View, bind, for_each, keyed, spacer,
    vstack,
};

use super::capabilities;
use super::parts::{acting, row_button, status_line, troubled};
use crate::layout::{
    card_stack, paragraph, setting_row, setting_row_content, setting_row_desc, settings_card,
    settings_card_titled, settings_divider, spread, value_text, when,
};
use crate::net::plugins::{EntityInfo, Kind, LogLevel, LogLine, PluginsState};
use crate::util::bridge::{bridge, derived};
use crate::util::clock::{ago, unix_millis};

/// The newest lines the card shows; the rest stay in the daemon's ring.
const SHOWN_LOG_LINES: usize = 50;

/// Reads the open entity, or `R`'s default once it has gone.
fn entity<R: Default>(
    runtime: &mut Runtime,
    state: Signal<PluginsState>,
    id: &str,
    read: impl FnOnce(&EntityInfo) -> R,
) -> R {
    state
        .with(runtime, |state| state.entity(id).map(read))
        .unwrap_or_default()
}

pub fn cards(cx: &mut Cx, state: Signal<PluginsState>, id: String, name: String) -> AnyView {
    let (kind, system) = cx.untrack(|runtime| {
        entity(runtime, state, &id, |entity| {
            (Some(entity.kind), entity.system)
        })
    });
    card_stack((
        acting(
            row_button("All Plugins & Themes", ButtonVariant::Ghost)
                .leading_icon(icons::CHEVRON_LEFT),
            state,
            PluginsState::close,
        )
        .self_start(),
        summary_card(cx, state, id.clone(), &name, kind.unwrap_or(Kind::Plugin)),
        actions_card(cx, state, id.clone(), system),
        capabilities::cards(state, id),
        logs_card(cx, state),
    ))
    .boxed()
}

fn summary_card(
    cx: &mut Cx,
    state: Signal<PluginsState>,
    id: String,
    name: &str,
    kind: Kind,
) -> impl View + use<> {
    let fact = |read: fn(&EntityInfo) -> String| {
        let id = id.clone();
        value_text(bind(move |runtime| entity(runtime, state, &id, read)))
    };
    let (described, written) = (id.clone(), id.clone());
    let enabled = bridge(
        cx,
        {
            let id = id.clone();
            move |runtime| entity(runtime, state, &id, |entity| entity.active)
        },
        move |runtime, enabled| {
            state.update(runtime, |state| state.set_enabled(written.clone(), enabled));
        },
    );
    let (switch_title, switch_description) = match kind {
        Kind::Theme => ("Use this theme", "Replaces the stock look"),
        Kind::Plugin => ("Enabled", "Runs in the background and on the bar"),
    };
    let troubled_id = id.clone();
    settings_card_titled(
        name,
        (
            keyed(
                move |runtime| {
                    entity(runtime, state, &described, |entity| {
                        entity.description.clone()
                    })
                },
                |_, description| {
                    if description.is_empty() {
                        vstack(()).display_none().boxed()
                    } else {
                        vstack((
                            setting_row_content(paragraph(description)),
                            settings_divider(),
                        ))
                        .boxed()
                    }
                },
            ),
            setting_row("Author", fact(|entity| entity.author.clone())),
            settings_divider(),
            setting_row("Version", fact(|entity| entity.version.clone())),
            settings_divider(),
            setting_row(
                "Status",
                keyed(
                    move |runtime| {
                        entity(runtime, state, &troubled_id, |entity| {
                            (troubled(entity), status_line(entity))
                        })
                    },
                    |_, (troubled, line)| {
                        let tone = if troubled {
                            Tone::Danger
                        } else {
                            Tone::Neutral
                        };
                        status(line).tone(tone)
                    },
                ),
            ),
            settings_divider(),
            setting_row_desc(
                switch_title,
                switch_description,
                toggle(enabled).label(switch_title),
            ),
        ),
    )
}

fn actions_card(
    cx: &mut Cx,
    state: Signal<PluginsState>,
    id: String,
    system: bool,
) -> impl View + use<> {
    let confirming = cx.signal(false);
    let reloaded = id.clone();
    let reload = setting_row_desc(
        "Reload",
        "Start it again from its files on disk",
        acting(
            row_button("Reload", ButtonVariant::Secondary).leading_icon(icons::ROTATE_CW),
            state,
            move |state| state.reload(reloaded.clone()),
        ),
    );
    let uninstall = (!system).then(|| {
        vstack((
            settings_divider(),
            keyed(
                move |runtime| confirming.get(runtime),
                move |_, asking| {
                    let id = id.clone();
                    if asking {
                        setting_row_content(spread((
                            status("Uninstall and delete its files?").tone(Tone::Danger),
                            spacer(),
                            row_button("Cancel", ButtonVariant::Ghost)
                                .on_click(move |cx, _| confirming.set(cx, false)),
                            acting(
                                row_button("Uninstall", ButtonVariant::Destructive),
                                state,
                                move |state| state.uninstall(id.clone()),
                            ),
                        )))
                        .boxed()
                    } else {
                        setting_row_desc(
                            "Uninstall",
                            "Removes it and the settings it stored",
                            row_button("Uninstall…", ButtonVariant::Destructive)
                                .on_click(move |cx, _| confirming.set(cx, true)),
                        )
                        .boxed()
                    }
                },
            ),
        ))
    });
    settings_card((reload, uninstall))
}

/// The newest log lines first, each with its position from the newest.
fn newest_first(logs: &[LogLine]) -> Vec<(usize, LogLine)> {
    logs.iter()
        .rev()
        .take(SHOWN_LOG_LINES)
        .cloned()
        .enumerate()
        .collect()
}

fn logs_card(cx: &mut Cx, state: Signal<PluginsState>) -> impl View + use<> {
    let lines = derived(cx, move |runtime| {
        state.with(runtime, |state| newest_first(&state.logs))
    });
    settings_card_titled(
        "Recent Activity",
        (
            when(
                move |runtime| lines.with(runtime, Vec::is_empty),
                |_| setting_row_content(status("Nothing logged yet").tone(Tone::Neutral)),
            ),
            for_each(
                lines,
                |(position, _): &(usize, LogLine)| *position,
                |_, line| log_row(line),
            ),
        ),
    )
}

fn log_row(line: Signal<(usize, LogLine)>) -> impl View {
    let read = move |runtime: &mut Runtime, pick: fn(&LogLine) -> String| {
        line.with(runtime, |(_, line)| pick(line))
    };
    vstack((
        when(
            move |runtime| line.with(runtime, |(position, _)| *position > 0),
            |_| settings_divider(),
        ),
        setting_row_content(spread((
            keyed(
                move |runtime| line.with(runtime, |(_, line)| line.level),
                |_, level| status(level_label(level)).tone(level_tone(level)),
            ),
            vstack(paragraph(bind(move |runtime| {
                read(runtime, |line| line.message.clone())
            })))
            .shrink(1.0),
            spacer(),
            value_text(bind(move |runtime| {
                read(runtime, |line| ago(line.at, unix_millis()))
            })),
        ))),
    ))
}

fn level_label(level: LogLevel) -> &'static str {
    match level {
        LogLevel::Debug => "Debug",
        LogLevel::Info => "Info",
        LogLevel::Warn => "Warning",
        LogLevel::Error => "Error",
    }
}

fn level_tone(level: LogLevel) -> Tone {
    match level {
        LogLevel::Debug | LogLevel::Info => Tone::Neutral,
        LogLevel::Warn => Tone::Warning,
        LogLevel::Error => Tone::Danger,
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn the_log_shows_newest_first_and_only_so_many() {
        let logs: Vec<LogLine> = (0..80)
            .map(|at| LogLine {
                id: "cpu".to_owned(),
                level: LogLevel::Info,
                at,
                message: String::new(),
            })
            .collect();
        let shown = newest_first(&logs);
        assert_eq!(shown.len(), SHOWN_LOG_LINES);
        assert_eq!(shown[0], (0, logs[79].clone()));
        assert_eq!(shown[1].0, 1);
    }
}
