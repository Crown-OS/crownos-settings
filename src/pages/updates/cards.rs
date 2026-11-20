//! The cards of the System Updates page, one for each part of the update.

use crownui::kit::{
    ButtonOptions, ButtonVariant, CalloutOptions, StatusOptions, Tone, button, callout, card,
    progress, spinner, status,
};
use crownui::prelude::{
    AnyView, Listeners, Runtime, Signal, Styled, View, bind, keyed, spacer, vstack,
};

use super::entrance::entering;
use super::notes::notes_view;
use super::verbs::{apply_label, refusal_reason, retry_command, verb_button};
use crate::layout::{
    INLINE_GAP, body_text, headline, inline, nothing, paragraph, setting_row_content,
    setting_row_desc, settings_card, settings_divider, spread, when,
};
use crate::net::updates::{
    ApplyStep, DownloadStatus, Failure, PendingRestart, Phase, RestartKind, UpdatesCommand,
    UpdatesState, Verb,
};
use crate::util::markdown;
use crate::util::os_release::installed_version_id;

const ROW_SPINNER: f32 = 14.0;
const SHOWN_LOG_LINES: usize = 3;

/// What the notes card shows; it is built again only when this changes.
#[derive(Debug, Clone, PartialEq)]
pub struct ReleaseNotes {
    version: String,
    notes: Option<String>,
}

impl ReleaseNotes {
    /// The next release, until it is installed.
    pub fn of(state: &UpdatesState) -> Option<Self> {
        let snapshot = state.snapshot.as_ref()?;
        if snapshot.pending_restart.is_some() {
            return None;
        }
        let next = snapshot.next.as_ref()?;
        Some(Self {
            version: next.version.to_string(),
            notes: next.notes.clone(),
        })
    }
}

#[derive(Debug, Clone, PartialEq)]
enum UpdateStatus {
    Unknown,
    NotChecked,
    Checking,
    UpToDate,
    Available(String),
    RestartPending(String),
}

impl UpdateStatus {
    fn of(state: &UpdatesState) -> Self {
        let Some(snapshot) = state.snapshot.as_ref() else {
            return Self::Unknown;
        };
        if snapshot.phase == Phase::Checking {
            return Self::Checking;
        }
        if let Some(pending) = &snapshot.pending_restart {
            return Self::RestartPending(pending.version.to_string());
        }
        match (&snapshot.next, snapshot.last_check) {
            (Some(next), _) => Self::Available(next.version.to_string()),
            (None, None) => Self::NotChecked,
            (None, Some(_)) => Self::UpToDate,
        }
    }
}

pub fn version_card(updates: Signal<UpdatesState>) -> impl View {
    settings_card((
        setting_row_desc(
            bind(move |runtime| updates.with(runtime, installed_title)),
            bind(move |runtime| updates.with(runtime, channel_line)),
            verb_button(
                "Check for updates",
                ButtonVariant::Secondary,
                updates,
                UpdatesCommand::Check,
            ),
        ),
        settings_divider(),
        setting_row_content(spread((
            keyed(
                move |runtime| updates.with(runtime, UpdateStatus::of),
                |_, status| status_view(status),
            ),
            spacer(),
            refusal_reason(updates, Verb::Check),
        ))),
    ))
}

fn installed_title(state: &UpdatesState) -> String {
    let version = state
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.installed.as_ref())
        .map(ToString::to_string)
        .or_else(|| installed_version_id().map(str::to_owned));
    match version {
        Some(version) => format!("CrownOS {version}"),
        None => "CrownOS".to_owned(),
    }
}

fn channel_line(state: &UpdatesState) -> String {
    match state
        .snapshot
        .as_ref()
        .and_then(|snapshot| snapshot.channel)
    {
        Some(channel) => format!("Release channel {channel}"),
        None => "The installed release".to_owned(),
    }
}

fn status_view(update_status: UpdateStatus) -> AnyView {
    match update_status {
        UpdateStatus::Unknown => nothing().boxed(),
        UpdateStatus::NotChecked => status("Not checked yet").boxed(),
        UpdateStatus::Checking => {
            inline((spinner().size(ROW_SPINNER), status("Checking…"))).boxed()
        }
        UpdateStatus::UpToDate => status("Up to date").tone(Tone::Success).boxed(),
        UpdateStatus::Available(version) => status(format!("{version} available")).boxed(),
        UpdateStatus::RestartPending(version) => status(format!("{version} is installed"))
            .tone(Tone::Warning)
            .boxed(),
    }
}

pub fn offline_card() -> impl View {
    settings_card((setting_row_content(
        status("The update service is not running").tone(Tone::Danger),
    ),))
}

pub fn error_card(updates: Signal<UpdatesState>, message: String) -> impl View {
    settings_card((setting_row_content(spread((
        status(message).tone(Tone::Danger),
        spacer(),
        button("Dismiss")
            .variant(ButtonVariant::Ghost)
            .on_click(move |cx, _| updates.update(cx, UpdatesState::dismiss_error)),
    ))),))
}

/// The failure, and the command that tries again in the state it left.
pub fn failure_of(state: &UpdatesState) -> Option<(Failure, UpdatesCommand, &'static str)> {
    let failure = state.failure()?.clone();
    let (retry, label) = retry_command(state);
    Some((failure, retry, label))
}

pub fn failure_card(
    updates: Signal<UpdatesState>,
    (failure, retry, retry_label): (Failure, UpdatesCommand, &'static str),
) -> impl View {
    let rollback_hint = match failure.rollback {
        Some(rollback) => setting_row_content(paragraph(format!(
            "To go back, hold Space while the computer starts and choose Rollback to CrownOS {}.",
            rollback.from
        )))
        .boxed(),
        None => nothing().boxed(),
    };
    settings_card((
        setting_row_content(
            callout(Tone::Danger, "The update did not finish").body(failure.message),
        ),
        rollback_hint,
        setting_row_content(spread((
            spacer(),
            button("Dismiss")
                .variant(ButtonVariant::Ghost)
                .on_click(move |cx, _| updates.update(cx, UpdatesState::dismiss_failure)),
            verb_button(retry_label, ButtonVariant::Primary, updates, retry),
        ))),
    ))
}

pub fn restart_card(updates: Signal<UpdatesState>, pending: PendingRestart) -> impl View {
    settings_card((
        setting_row_content(
            callout(
                Tone::Success,
                format!("CrownOS {} is installed", pending.version),
            )
            .body(restart_consequence(pending.kind)),
        ),
        setting_row_content(spread((
            spacer(),
            verb_button(
                pending.kind.button_label(),
                ButtonVariant::Primary,
                updates,
                UpdatesCommand::Restart(pending.kind),
            ),
        ))),
    ))
}

fn restart_consequence(kind: RestartKind) -> &'static str {
    match kind {
        RestartKind::Session => "Open apps will close and the desktop restarts.",
        RestartKind::SoftReboot => "The system restarts without a full reboot.",
        RestartKind::Reboot => "The computer reboots.",
    }
}

pub fn progress_card(updates: Signal<UpdatesState>, phase: Phase) -> impl View {
    let fraction = move |runtime: &mut Runtime| {
        updates.with(runtime, |state| {
            state.progress.and_then(|progress| progress.fraction)
        })
    };
    settings_card((setting_row_content(
        vstack((
            body_text(phase_label(phase)),
            keyed(
                move |runtime| fraction(runtime).is_some(),
                move |_, determinate| {
                    if determinate {
                        progress(bind(move |runtime| fraction(runtime).unwrap_or(0.0)))
                            .w_full()
                            .boxed()
                    } else {
                        spinner().size(ROW_SPINNER).boxed()
                    }
                },
            ),
            paragraph(bind(move |runtime| updates.with(runtime, last_log_lines))),
        ))
        .gap(INLINE_GAP)
        .self_stretch(),
    ),))
}

fn phase_label(phase: Phase) -> &'static str {
    match phase {
        Phase::Idle => "",
        Phase::Checking => "Checking…",
        Phase::Downloading => "Downloading…",
        Phase::Applying(ApplyStep::Snapshot) => "Taking a snapshot…",
        Phase::Applying(ApplyStep::KeepKernel) => "Keeping the current kernel…",
        Phase::Applying(ApplyStep::Install) => "Installing…",
        Phase::Applying(ApplyStep::SwitchChannel) => "Switching the release channel…",
        Phase::Applying(ApplyStep::PostUpdate) => "Running the release script…",
        Phase::Applying(ApplyStep::RecordApplied) => "Recording the update…",
    }
}

fn last_log_lines(state: &UpdatesState) -> String {
    let skipped = state.log.len().saturating_sub(SHOWN_LOG_LINES);
    let lines: Vec<&str> = state.log.iter().skip(skipped).map(String::as_str).collect();
    lines.join("\n")
}

pub fn notes_card(updates: Signal<UpdatesState>, release: ReleaseNotes) -> impl View {
    let notes = match release.notes.as_deref() {
        Some(notes) => notes_view(markdown::blocks(notes)).boxed(),
        None => paragraph("This release has no notes.").boxed(),
    };
    let download_pending = move |runtime: &mut Runtime| {
        updates.with(runtime, |state| {
            state
                .snapshot
                .as_ref()
                .and_then(|snapshot| snapshot.next.as_ref())
                .is_some_and(|next| next.download == DownloadStatus::Pending)
        })
    };
    entering(
        card((
            setting_row_content(
                vstack((
                    headline(format!("What's new in {}", release.version)),
                    notes,
                ))
                .gap(INLINE_GAP)
                .self_stretch(),
            ),
            settings_divider(),
            setting_row_content(spread((
                refusal_reason(updates, Verb::Apply),
                spacer(),
                when(download_pending, move |_| {
                    verb_button(
                        "Download",
                        ButtonVariant::Secondary,
                        updates,
                        UpdatesCommand::Download,
                    )
                }),
                verb_button(
                    bind(move |runtime| updates.with(runtime, apply_label)),
                    ButtonVariant::Primary,
                    updates,
                    UpdatesCommand::Apply,
                ),
            ))),
        ))
        .self_stretch(),
    )
}
