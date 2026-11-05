//! Resetting and removing Android, as root.
//!
//! Both go through `crownos-android-setup remove`, which `pkexec` runs under
//! the `org.crownos.android.setup` polkit action. A reset then runs
//! `crownos-android-setup enable` the same way, which turns Android apps back on
//! and starts the setup service again.

use std::process::{Command, Stdio};

use super::Removal;

const PKEXEC: &str = "pkexec";
/// Outside `$PATH`, so `pkexec` is handed the path its polkit action names.
const SETUP: &str = "/usr/lib/crownos-android/crownos-android-setup";

/// `pkexec`'s exit codes for a dismissed or refused authentication.
const PKEXEC_NOT_AUTHORIZED: i32 = 126;
const PKEXEC_AUTHENTICATION_FAILED: i32 = 127;

/// One command line, and what it failing means to the user.
#[derive(Debug, PartialEq, Eq)]
struct Task {
    line: &'static [&'static str],
    failing: &'static str,
}

const WIPE: Task = Task {
    line: &[PKEXEC, SETUP, "remove", "--wipe-data"],
    failing: "Android data could not be reset",
};
const SET_UP_AGAIN: Task = Task {
    line: &[PKEXEC, SETUP, "enable"],
    failing: "Android could not be set up again",
};
const UNINSTALL: Task = Task {
    line: &[PKEXEC, SETUP, "remove"],
    failing: "Android apps could not be removed",
};

/// What `removal` runs, in order, each only after the one before succeeded.
fn tasks(removal: Removal) -> &'static [Task] {
    match removal {
        Removal::ResetData => &[WIPE, SET_UP_AGAIN],
        Removal::Uninstall => &[UNINSTALL],
    }
}

/// Runs `removal` on the blocking pool, where waiting for the password
/// prompt costs the UI nothing.
pub(super) async fn remove(removal: Removal) -> Result<(), String> {
    tokio::task::spawn_blocking(move || tasks(removal).iter().try_for_each(run))
        .await
        .unwrap_or_else(|_| Err("The removal stopped unexpectedly.".to_owned()))
}

fn run(task: &Task) -> Result<(), String> {
    let Some((program, args)) = task.line.split_first() else {
        return Ok(());
    };
    let output = Command::new(program)
        .args(args)
        .stdin(Stdio::null())
        .output()
        .map_err(|error| format!("{}: {error}", task.failing))?;
    if output.status.success() {
        return Ok(());
    }
    Err(failure(task, output.status.code(), &output.stderr))
}

/// What to tell the user about `task` exiting with `code`.
fn failure(task: &Task, code: Option<i32>, stderr: &[u8]) -> String {
    if matches!(
        code,
        Some(PKEXEC_NOT_AUTHORIZED | PKEXEC_AUTHENTICATION_FAILED)
    ) && task.line.first() == Some(&PKEXEC)
    {
        return "Permission was not given.".to_owned();
    }
    let stderr = String::from_utf8_lossy(stderr);
    match stderr
        .lines()
        .rev()
        .map(str::trim)
        .find(|line| !line.is_empty())
    {
        Some(reason) => format!("{}: {reason}", task.failing),
        None => format!("{}.", task.failing),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn a_reset_wipes_the_data_then_starts_setup_again() {
        assert_eq!(tasks(Removal::ResetData), [WIPE, SET_UP_AGAIN]);
        assert_eq!(WIPE.line[2..], ["remove", "--wipe-data"]);
        assert_eq!(SET_UP_AGAIN.line[2..], ["enable"]);
        assert_eq!(tasks(Removal::Uninstall), [UNINSTALL]);
        assert_eq!(UNINSTALL.line[2..], ["remove"]);
    }

    #[test]
    fn a_refused_prompt_reads_as_such_and_other_failures_say_why() {
        assert_eq!(failure(&WIPE, Some(126), b""), "Permission was not given.");
        assert_eq!(
            failure(&UNINSTALL, Some(127), b"x"),
            "Permission was not given."
        );
        assert_eq!(
            failure(
                &UNINSTALL,
                Some(1),
                b"stopping waydroid\nimages are busy\n\n"
            ),
            "Android apps could not be removed: images are busy"
        );
        assert_eq!(
            failure(&SET_UP_AGAIN, Some(1), b""),
            "Android could not be set up again."
        );
    }
}
