//! Running a privileged helper through `pkexec`.

use std::path::Path;
use std::process::Command;

use anyhow::{Context, Result, anyhow};

/// pkexec exits with 126 when the user dismisses the password prompt and with
/// 127 when polkit refuses the request.
const DECLINED_STATUSES: [i32; 2] = [126, 127];

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum AdminAction {
    Applied,
    Declined,
}

/// Runs `helper` with `args` as root and waits for it to finish. A declined
/// password prompt is not an error. On failure, the error is the last line the
/// helper wrote to stderr, without the helper's own name in front of it.
pub fn run_as_admin(helper: &str, args: &[&str]) -> Result<AdminAction> {
    let output = Command::new("pkexec")
        .arg(helper)
        .args(args)
        .output()
        .context("couldn't ask for your password")?;
    outcome(
        helper,
        output.status.code(),
        &String::from_utf8_lossy(&output.stderr),
    )
}

fn outcome(helper: &str, exit_code: Option<i32>, stderr: &str) -> Result<AdminAction> {
    match exit_code {
        Some(0) => Ok(AdminAction::Applied),
        Some(code) if DECLINED_STATUSES.contains(&code) => Ok(AdminAction::Declined),
        _ => Err(anyhow!(failure_reason(helper, exit_code, stderr))),
    }
}

fn failure_reason(helper: &str, exit_code: Option<i32>, stderr: &str) -> String {
    let helper_name = Path::new(helper)
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or(helper);
    let last_line = stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty());
    match (last_line, exit_code) {
        (Some(line), _) => line
            .strip_prefix(helper_name)
            .and_then(|rest| rest.strip_prefix(": "))
            .unwrap_or(line)
            .to_owned(),
        (None, Some(code)) => format!("{helper_name} exited with status {code}"),
        (None, None) => format!("{helper_name} was stopped by a signal"),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    const HELPER: &str = "/usr/bin/crownos-memory";

    fn reason(exit_code: Option<i32>, stderr: &str) -> String {
        outcome(HELPER, exit_code, stderr)
            .err()
            .map(|error| error.to_string())
            .unwrap_or_default()
    }

    #[test]
    fn success_applies_and_a_closed_prompt_declines() {
        assert_eq!(
            outcome(HELPER, Some(0), "").ok(),
            Some(AdminAction::Applied)
        );
        assert_eq!(
            outcome(HELPER, Some(126), "").ok(),
            Some(AdminAction::Declined)
        );
        assert_eq!(
            outcome(HELPER, Some(127), "").ok(),
            Some(AdminAction::Declined)
        );
    }

    #[test]
    fn a_failure_reads_as_the_last_line_without_the_helper_name() {
        assert_eq!(
            reason(Some(1), "starting\ncrownos-memory: no room left\n\n"),
            "no room left"
        );
        assert_eq!(reason(Some(1), "plain message"), "plain message");
        assert_eq!(
            reason(Some(3), " \n"),
            "crownos-memory exited with status 3"
        );
        assert_eq!(reason(None, ""), "crownos-memory was stopped by a signal");
    }
}
