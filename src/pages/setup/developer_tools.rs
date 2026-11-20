//! The Developer Tools page: the coding agents CrownOS installs, the browser
//! automation they share, and the setup tasks behind both.

use crownos_config::system::{DeveloperTool, SetupTask, TaskState, TaskStatus};
use crownui::kit::{icons, toggle};
use crownui::prelude::{AnyView, Cx, Semantic, Signal, View, bind, for_each, vstack};
use tokio::sync::mpsc::UnboundedSender;

use super::view::SetupView;
use crate::layout::{
    Listed, hairline_unless_first, listed, live_glyph, nothing, paragraph, setting_row_content,
    setting_row_desc, setting_row_icon_desc, settings_card_titled, settings_divider, when,
};
use crate::net::setup::{self, SetupStatus, playwright_line, task_line, tool_line};
use crate::pages::{PageDescriptor, page};
use crate::state::Store;
use crate::util::bridge::{bridge, derived};
use crate::util::worker::mailbox;

pub const PAGE: PageDescriptor = PageDescriptor {
    title: "Developer Tools",
    slug: "developer-tools",
    icon: icons::SQUARE_TERMINAL,
    keywords: &[
        "claude",
        "codex",
        "cursor",
        "agent",
        "mcp",
        "playwright",
        "developer",
    ],
    build,
};

type ToolRequests = UnboundedSender<(DeveloperTool, bool)>;
type TaskRow = Signal<Listed<(SetupTask, TaskState)>>;

fn build(cx: &mut Cx, _store: Store) -> AnyView {
    let setup = SetupView::open(cx);
    let requests = mailbox(cx, move |cx, (tool, enabled): (DeveloperTool, bool)| {
        setup.apply(cx, setup::set_tool(tool, enabled));
    });
    let mut agent_rows = Vec::new();
    for tool in DeveloperTool::ALL {
        if !agent_rows.is_empty() {
            agent_rows.push(settings_divider().boxed());
        }
        agent_rows.push(tool_row(cx, setup, requests.clone(), tool).boxed());
    }

    page(
        &PAGE,
        (
            settings_card_titled("Coding agents", agent_rows),
            settings_card_titled(
                "Browser automation",
                (setting_row_desc(
                    "Playwright",
                    bind(move |runtime| setup.line(runtime, playwright_line)),
                    nothing(),
                ),),
            ),
            tasks_card(cx, setup),
            setup.notice(),
            setup.failure(),
        ),
    )
}

fn tool_row(
    cx: &mut Cx,
    setup: SetupView,
    requests: ToolRequests,
    tool: DeveloperTool,
) -> impl View + use<> {
    let enabled = bridge(
        cx,
        move |runtime| setup.config(runtime).developer_tools.is_enabled(tool),
        move |_, enabled| {
            let _ = requests.send((tool, enabled));
        },
    );
    setting_row_desc(
        tool.name(),
        bind(move |runtime| setup.line(runtime, |status| tool_line(status, tool))),
        toggle(enabled).disabled(bind(move |runtime| setup.is_locked(runtime))),
    )
}

fn tasks_card(cx: &mut Cx, setup: SetupView) -> impl View + use<> {
    let tasks = derived(cx, move |runtime| {
        setup
            .read(runtime, |status| listed(status.tasks.iter().cloned()))
            .unwrap_or_default()
    });
    settings_card_titled(
        "Setup tasks",
        (
            when(
                move |runtime| tasks.with(runtime, Vec::is_empty),
                move |_| {
                    setting_row_content(paragraph(bind(move |runtime| {
                        setup.line(runtime, empty_tasks_line)
                    })))
                },
            ),
            for_each(
                tasks,
                |row: &Listed<(SetupTask, TaskState)>| row.item.0,
                task_row,
            ),
        ),
    )
}

fn task_row(cx: &mut Cx, row: TaskRow) -> impl View + use<> {
    let task = row.with_untracked(cx, |row| row.item.0);
    vstack((
        hairline_unless_first(row),
        setting_row_icon_desc(
            live_glyph(move |runtime| row.with(runtime, |row| task_glyph(&row.item.1))),
            task.name(),
            bind(move |runtime| row.with(runtime, |row| task_line(&row.item.1))),
            nothing(),
        ),
    ))
}

fn empty_tasks_line(status: &SetupStatus) -> String {
    let line = match status.states_error {
        Some(_) => "Could not read the task states",
        None => "No setup tasks yet",
    };
    line.to_owned()
}

fn task_glyph(state: &TaskState) -> &'static str {
    match state.status {
        TaskStatus::Pending => icons::CLOCK,
        TaskStatus::Running => icons::LOADER_CIRCLE,
        TaskStatus::Done => icons::CIRCLE_CHECK,
        TaskStatus::Failed { .. } => icons::CIRCLE_X,
    }
}
