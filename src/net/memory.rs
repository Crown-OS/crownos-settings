//! Compressed memory (zram) and the swap partition, both sized by
//! `crownos-memory`: read without privileges, changed through `pkexec`.
//!
//! crownos-installer sets both up. The live system has neither, so its
//! [`Memory`] reads as nothing to resize.

use std::process::Command;

use anyhow::{Context, Result, bail};

/// Run through `pkexec` to change anything; its polkit action is
/// `org.crownos.memory.manage`.
const HELPER: &str = "/usr/bin/crownos-memory";
const HELPER_NAME: &str = "crownos-memory: ";
/// pkexec's exit statuses for a dismissed or refused password prompt.
const PKEXEC_DECLINED: [i32; 2] = [126, 127];
const MIB_PER_GIB: u64 = 1024;

pub const ZRAM_PERCENT_MIN: u32 = 10;
pub const ZRAM_PERCENT_MAX: u32 = 100;
pub const ZRAM_PERCENT_STEP: u32 = 5;
pub const SWAP_GIB_MIN: u32 = 1;

#[derive(Debug, Clone, Copy, Default, PartialEq, Eq)]
pub struct Memory {
    pub ram_mib: u64,
    pub zram_percent: Option<u32>,
    pub swap: Option<SwapPartition>,
}

/// A swap partition right behind a btrfs root: the only kind whose size can
/// change while the system runs.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub struct SwapPartition {
    pub gib: u32,
    pub max_gib: u32,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Resize {
    Zram { percent: u32 },
    Swap { gib: u32 },
}

impl Memory {
    pub async fn read() -> Self {
        tokio::task::spawn_blocking(|| Command::new(HELPER).arg("status").output())
            .await
            .ok()
            .and_then(Result::ok)
            .map(|output| Self::parse(&String::from_utf8_lossy(&output.stdout)))
            .unwrap_or_default()
    }

    fn parse(status: &str) -> Self {
        let field = |name: &str| {
            status.lines().find_map(|line| {
                line.strip_prefix(name)?
                    .strip_prefix('=')?
                    .parse::<u64>()
                    .ok()
            })
        };
        let gib = |name| field(name).and_then(|mib| u32::try_from(mib / MIB_PER_GIB).ok());
        Self {
            ram_mib: field("ram_mib").unwrap_or_default(),
            zram_percent: field("zram_percent").and_then(|percent| u32::try_from(percent).ok()),
            swap: gib("swap_mib")
                .zip(gib("swap_max_mib"))
                .map(|(gib, max_gib)| SwapPartition {
                    gib,
                    max_gib: max_gib.max(gib),
                }),
        }
    }

    pub fn zram_gib(&self, percent: u32) -> f64 {
        (self.ram_mib * u64::from(percent)) as f64 / 100.0 / MIB_PER_GIB as f64
    }
}

impl Resize {
    fn args(self) -> [String; 2] {
        match self {
            Self::Zram { percent } => ["zram".to_owned(), percent.to_string()],
            Self::Swap { gib } => [
                "swap".to_owned(),
                (u64::from(gib) * MIB_PER_GIB).to_string(),
            ],
        }
    }

    fn run(self) -> Result<()> {
        let output = Command::new("pkexec")
            .arg(HELPER)
            .args(self.args())
            .output()
            .context("couldn't ask for your password")?;
        let declined = output
            .status
            .code()
            .is_some_and(|code| PKEXEC_DECLINED.contains(&code));
        if output.status.success() || declined {
            return Ok(());
        }
        let stderr = String::from_utf8_lossy(&output.stderr);
        let reason = stderr.lines().last().unwrap_or("the change failed");
        bail!("{}", reason.trim_start_matches(HELPER_NAME))
    }
}

/// Asks for `change`; a dismissed password prompt changes nothing and is not
/// an error.
pub async fn resize(change: Resize) -> Result<()> {
    tokio::task::spawn_blocking(move || change.run()).await?
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn an_installed_system_reads_both_sizes() {
        let memory =
            Memory::parse("ram_mib=15156\nzram_percent=50\nswap_mib=15156\nswap_max_mib=40960\n");
        assert_eq!(memory.zram_percent, Some(50));
        assert_eq!(
            memory.swap,
            Some(SwapPartition {
                gib: 14,
                max_gib: 40
            })
        );
        assert!((memory.zram_gib(50) - 7.4).abs() < 0.01);
    }

    #[test]
    fn the_live_system_has_nothing_to_resize() {
        let memory = Memory::parse("ram_mib=4096\nzram_percent=\n");
        assert_eq!(memory.zram_percent, None);
        assert_eq!(memory.swap, None);
    }

    #[test]
    fn swap_is_asked_for_in_mebibytes() {
        assert_eq!(Resize::Swap { gib: 8 }.args(), ["swap", "8192"]);
        assert_eq!(Resize::Zram { percent: 75 }.args(), ["zram", "75"]);
    }
}
