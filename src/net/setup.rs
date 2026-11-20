//! The post-install choices that `crownos-setup` applies: the web browser and
//! the developer tools. The choices and the task states are read without
//! privileges. Changes go through `pkexec`.
//!
//! The runner package can be missing. Then the choices and states read as
//! the defaults and nothing can change.

use std::borrow::Cow;
use std::path::Path;

use anyhow::Result;
use crownos_config::system::{
    Browser, DeveloperTool, PLAYWRIGHT_MCP_SERVER, SetupConfig, SetupConfigError, SetupStateError,
    SetupTask, TaskState, TaskStatus,
};

use crate::util::pkexec::{AdminAction, run_as_admin};

/// Run through `pkexec`; its polkit action is `org.crownos.setup.manage`.
const HELPER: &str = "/usr/bin/crownos-setup";
const SETTING_UP: &str = "Setting up in the background";
const WAITING_FOR_NETWORK: &str = "Waits for the computer to be online";
const NOT_INSTALLED: &str = "Not installed";

#[derive(Debug, Clone, Default, PartialEq, Eq)]
pub struct SetupStatus {
    pub config: SetupConfig,
    pub config_error: Option<String>,
    pub tasks: Vec<(SetupTask, TaskState)>,
    pub states_error: Option<String>,
    pub browser_installed: bool,
    pub tools_installed: Vec<(DeveloperTool, bool)>,
    pub playwright_installed: bool,
    pub runner_available: bool,
}

impl SetupStatus {
    pub async fn read() -> Self {
        tokio::task::spawn_blocking(Self::read_blocking)
            .await
            .unwrap_or_default()
    }

    fn read_blocking() -> Self {
        let (config, config_error) = match SetupConfig::load() {
            Ok(config) => (config.unwrap_or_default(), None),
            Err(error) => (SetupConfig::default(), Some(config_error_detail(&error))),
        };
        let (tasks, states_error) = match TaskState::load_all_installed() {
            Ok(tasks) => (tasks, None),
            Err(error) => (Vec::new(), Some(state_error_detail(&error))),
        };
        let exists = |path: &str| Path::new(path).exists();
        Self {
            browser_installed: exists(config.browser.executable()),
            tools_installed: DeveloperTool::ALL
                .into_iter()
                .map(|tool| (tool, exists(tool.executable())))
                .collect(),
            playwright_installed: exists(PLAYWRIGHT_MCP_SERVER),
            runner_available: exists(HELPER),
            config,
            config_error,
            tasks,
            states_error,
        }
    }

    /// True while a task is not done (the runner retries a failed task), or
    /// is still to start: the runner is there, something chosen is missing,
    /// and its task has no state yet.
    pub fn is_busy(&self) -> bool {
        let unstarted = |task: SetupTask, missing: bool| {
            self.runner_available && missing && self.task(task).is_none()
        };
        let tools_missing = self
            .config
            .developer_tools
            .enabled()
            .any(|tool| !self.is_tool_installed(tool));
        self.tasks
            .iter()
            .any(|(_, state)| state.status != TaskStatus::Done)
            || unstarted(SetupTask::Browser, !self.browser_installed)
            || unstarted(SetupTask::DeveloperTools, tools_missing)
            || unstarted(
                SetupTask::Playwright,
                self.config.wants_playwright() && !self.playwright_installed,
            )
    }

    fn task(&self, task: SetupTask) -> Option<&TaskState> {
        self.tasks
            .iter()
            .find_map(|(candidate, state)| (*candidate == task).then_some(state))
    }

    fn task_status(&self, task: SetupTask) -> Option<&TaskStatus> {
        self.task(task).map(|state| &state.status)
    }

    fn is_tool_installed(&self, tool: DeveloperTool) -> bool {
        self.tools_installed
            .iter()
            .any(|(candidate, installed)| *candidate == tool && *installed)
    }
}

pub fn browser_line(status: &SetupStatus) -> String {
    let line = match (
        status.task_status(SetupTask::Browser),
        status.browser_installed,
    ) {
        (Some(TaskStatus::Failed { message }), _) => return failed(message),
        (Some(TaskStatus::Running), _) => SETTING_UP,
        (Some(TaskStatus::Pending), _) => WAITING_FOR_NETWORK,
        (None, false) if status.runner_available => WAITING_FOR_NETWORK,
        (Some(TaskStatus::Done), true) => "Installed and set as default",
        (None, true) => "Installed",
        (Some(TaskStatus::Done) | None, false) => NOT_INSTALLED,
    };
    line.to_owned()
}

/// The `developer-tools` task covers every tool, so its progress and failure
/// only describe a tool that is chosen and not there yet.
pub fn tool_line(status: &SetupStatus, tool: DeveloperTool) -> String {
    let chosen = status.config.developer_tools.is_enabled(tool);
    let installed = status.is_tool_installed(tool);
    let line = match (
        chosen,
        installed,
        status.task_status(SetupTask::DeveloperTools),
    ) {
        (true, false, Some(TaskStatus::Failed { message })) => return failed(message),
        (true, false, Some(TaskStatus::Running)) => "Installing in the background",
        (true, false, Some(TaskStatus::Pending)) => WAITING_FOR_NETWORK,
        (true, false, None) if status.runner_available => WAITING_FOR_NETWORK,
        (false, true, Some(TaskStatus::Pending | TaskStatus::Running))
            if status.runner_available =>
        {
            "Removing in the background"
        }
        (_, true, _) => "Installed",
        (_, false, _) => NOT_INSTALLED,
    };
    line.to_owned()
}

pub fn playwright_line(status: &SetupStatus) -> String {
    let line = match (
        status.task_status(SetupTask::Playwright),
        status.playwright_installed,
    ) {
        (Some(TaskStatus::Failed { message }), _) => return failed(message),
        (Some(TaskStatus::Running), _) => SETTING_UP,
        (Some(TaskStatus::Pending), _) => WAITING_FOR_NETWORK,
        (None, false) if status.runner_available => WAITING_FOR_NETWORK,
        (Some(TaskStatus::Done) | None, true) => {
            return format!("Playwright drives {}", status.config.browser.name());
        }
        (Some(TaskStatus::Done) | None, false) => NOT_INSTALLED,
    };
    line.to_owned()
}

pub fn task_line(state: &TaskState) -> String {
    match &state.status {
        TaskStatus::Pending => "Waiting".to_owned(),
        TaskStatus::Running => "Running".to_owned(),
        TaskStatus::Done => "Done".to_owned(),
        TaskStatus::Failed { message } => failed(message),
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct SetupNotice {
    pub title: &'static str,
    pub detail: Cow<'static, str>,
}

/// What stops the page from showing the full picture, most important first.
pub fn setup_notice(status: &SetupStatus) -> Option<SetupNotice> {
    let (title, detail) = if let Some(error) = &status.config_error {
        (
            "Could not read the setup choices",
            Cow::Owned(error.clone()),
        )
    } else if let Some(error) = &status.states_error {
        ("Could not read the setup state", Cow::Owned(error.clone()))
    } else if !status.runner_available {
        (
            "The crownos-setup package is not installed",
            Cow::Borrowed("Choices apply after it is."),
        )
    } else {
        return None;
    };
    Some(SetupNotice { title, detail })
}

/// True when the runner has not moved since `before`: the same tasks, each in
/// the same state and generation. A request rewrites the choices at once, so
/// only the tasks show that the runner took it up.
pub fn tasks_unchanged(before: &SetupStatus, after: &SetupStatus) -> bool {
    before.tasks == after.tasks
}

fn failed(message: &str) -> String {
    format!("Failed: {message}")
}

fn config_error_detail(error: &SetupConfigError) -> String {
    match error {
        SetupConfigError::Read { path, source } => format!("{}: {source}", path.display()),
        SetupConfigError::Parse { path, source } => format!("{}: {source}", path.display()),
    }
}

fn state_error_detail(error: &SetupStateError) -> String {
    match error {
        SetupStateError::Read { path, source } => format!("{}: {source}", path.display()),
        SetupStateError::Parse { path, source } => format!("{}: {source}", path.display()),
        SetupStateError::Render(source) => source.to_string(),
    }
}

pub async fn set_browser(browser: Browser) -> Result<AdminAction> {
    run_setup(["browser", "set", browser.id()]).await
}

pub async fn set_tool(tool: DeveloperTool, enabled: bool) -> Result<AdminAction> {
    let switch = if enabled { "on" } else { "off" };
    run_setup(["tools", "set", tool.id(), switch]).await
}

async fn run_setup<const COUNT: usize>(args: [&'static str; COUNT]) -> Result<AdminAction> {
    tokio::task::spawn_blocking(move || run_as_admin(HELPER, &args)).await?
}

#[cfg(test)]
mod tests {
    use super::*;

    fn state(status: TaskStatus) -> TaskState {
        TaskState {
            status,
            generation: 1,
            started_at_unix: None,
            finished_at_unix: None,
        }
    }

    fn system(tasks: &[(SetupTask, TaskStatus)]) -> SetupStatus {
        SetupStatus {
            tasks: tasks
                .iter()
                .map(|(task, status)| (*task, state(status.clone())))
                .collect(),
            tools_installed: DeveloperTool::ALL
                .into_iter()
                .map(|tool| (tool, false))
                .collect(),
            runner_available: true,
            ..SetupStatus::default()
        }
    }

    fn failure() -> TaskStatus {
        TaskStatus::Failed {
            message: "no network".to_owned(),
        }
    }

    fn browser(task: TaskStatus, installed: bool) -> String {
        let mut status = system(&[(SetupTask::Browser, task)]);
        status.browser_installed = installed;
        browser_line(&status)
    }

    fn playwright(task: Option<TaskStatus>, installed: bool, runner_available: bool) -> String {
        let mut status = system(&[]);
        status.tasks = task
            .map(|task| (SetupTask::Playwright, state(task)))
            .into_iter()
            .collect();
        status.playwright_installed = installed;
        status.runner_available = runner_available;
        status.config.browser = Browser::Firefox;
        playwright_line(&status)
    }

    fn chosen_codex(task: Option<TaskStatus>) -> String {
        let mut status = system(&[]);
        status.tasks = task
            .map(|task| (SetupTask::DeveloperTools, state(task)))
            .into_iter()
            .collect();
        status.config.developer_tools.codex = true;
        tool_line(&status, DeveloperTool::Codex)
    }

    #[test]
    fn browser_lines() {
        assert_eq!(browser(TaskStatus::Done, false), "Not installed");
        assert_eq!(
            browser(TaskStatus::Done, true),
            "Installed and set as default"
        );
        assert_eq!(
            browser(TaskStatus::Running, false),
            "Setting up in the background"
        );
        assert_eq!(
            browser(TaskStatus::Pending, false),
            "Waits for the computer to be online"
        );
        assert_eq!(browser(failure(), true), "Failed: no network");
        let mut status = system(&[]);
        status.browser_installed = true;
        assert_eq!(browser_line(&status), "Installed");
        status.browser_installed = false;
        status.runner_available = false;
        assert_eq!(browser_line(&status), "Not installed");
    }

    #[test]
    fn playwright_lines() {
        assert_eq!(
            playwright(Some(TaskStatus::Done), false, true),
            "Not installed"
        );
        assert_eq!(
            playwright(Some(TaskStatus::Running), false, true),
            "Setting up in the background"
        );
        assert_eq!(playwright(None, false, false), "Not installed");
        assert_eq!(
            playwright(None, false, true),
            "Waits for the computer to be online"
        );
        assert_eq!(
            playwright(Some(TaskStatus::Done), true, true),
            "Playwright drives Firefox"
        );
        assert_eq!(
            playwright(Some(failure()), true, true),
            "Failed: no network"
        );
    }

    #[test]
    fn tool_lines() {
        assert_eq!(chosen_codex(None), "Waits for the computer to be online");
        assert_eq!(chosen_codex(Some(TaskStatus::Done)), "Not installed");
        assert_eq!(
            chosen_codex(Some(TaskStatus::Running)),
            "Installing in the background"
        );
        assert_eq!(chosen_codex(Some(failure())), "Failed: no network");
    }

    #[test]
    fn the_shared_tools_task_only_describes_chosen_missing_tools() {
        let mut status = system(&[(SetupTask::DeveloperTools, failure())]);
        status.config.developer_tools.codex = true;
        status.tools_installed[0] = (DeveloperTool::ClaudeCode, true);
        assert_eq!(tool_line(&status, DeveloperTool::ClaudeCode), "Installed");
        assert_eq!(tool_line(&status, DeveloperTool::Cursor), "Not installed");

        status.tasks = vec![(SetupTask::DeveloperTools, state(TaskStatus::Running))];
        assert_eq!(
            tool_line(&status, DeveloperTool::ClaudeCode),
            "Removing in the background"
        );
        status.runner_available = false;
        assert_eq!(tool_line(&status, DeveloperTool::ClaudeCode), "Installed");
    }

    #[test]
    fn each_task_state_has_a_line() {
        let lines = [
            TaskStatus::Pending,
            TaskStatus::Running,
            TaskStatus::Done,
            failure(),
        ]
        .map(|status| task_line(&state(status)));
        assert_eq!(lines, ["Waiting", "Running", "Done", "Failed: no network"]);
    }

    #[test]
    fn busy_until_every_task_settles() {
        let mut settled = system(&[
            (SetupTask::Browser, TaskStatus::Done),
            (SetupTask::Playwright, TaskStatus::Done),
        ]);
        settled.browser_installed = true;
        settled.playwright_installed = true;
        assert!(!settled.is_busy());

        let mut pending = settled.clone();
        pending.tasks[0].1 = state(TaskStatus::Pending);
        assert!(pending.is_busy());

        let mut failed = settled.clone();
        failed.tasks[1].1 = state(failure());
        assert!(failed.is_busy(), "the runner retries a failed task");

        let mut chosen_tool_missing = settled.clone();
        chosen_tool_missing.config.developer_tools.cursor = true;
        assert!(chosen_tool_missing.is_busy(), "no state file yet");
        chosen_tool_missing.runner_available = false;
        assert!(!chosen_tool_missing.is_busy());
    }

    #[test]
    fn a_new_choice_with_the_same_tasks_still_awaits_the_runner() {
        let before = system(&[(SetupTask::Browser, TaskStatus::Done)]);
        let mut after = before.clone();
        after.config.browser = Browser::Firefox;
        assert!(tasks_unchanged(&before, &after));

        after.tasks[0].1.generation += 1;
        assert!(!tasks_unchanged(&before, &after));
    }

    #[test]
    fn notices_by_priority() {
        let mut status = system(&[]);
        assert_eq!(setup_notice(&status), None);
        status.runner_available = false;
        assert_eq!(
            setup_notice(&status).map(|notice| notice.detail),
            Some(Cow::Borrowed("Choices apply after it is."))
        );
        status.states_error = Some("browser.ron: bad".to_owned());
        assert_eq!(
            setup_notice(&status),
            Some(SetupNotice {
                title: "Could not read the setup state",
                detail: Cow::Borrowed("browser.ron: bad"),
            })
        );
    }
}
