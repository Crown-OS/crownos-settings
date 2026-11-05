//! How many bytes the files under a directory hold.

use std::fs;
use std::path::Path;

/// The total length of every file under `root`, symlinks counted as links.
/// Directories that cannot be read count as empty, so the total is a lower
/// bound; `None` when `root` itself cannot be listed.
pub fn tree_size(root: &Path) -> Option<u64> {
    let mut pending = vec![fs::read_dir(root).ok()?];
    let mut total = 0;
    while let Some(entries) = pending.pop() {
        for entry in entries.flatten() {
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            if metadata.is_dir() {
                pending.extend(fs::read_dir(entry.path()).ok());
            } else {
                total += metadata.len();
            }
        }
    }
    Some(total)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn sums_nested_files_without_following_links() -> std::io::Result<()> {
        let root = std::env::temp_dir().join(format!("crownsettings-tree-{}", std::process::id()));
        let _ = fs::remove_dir_all(&root);
        fs::create_dir_all(root.join("data/app"))?;
        fs::write(root.join("system.img"), vec![0u8; 3000])?;
        fs::write(root.join("data/app/base.apk"), vec![0u8; 500])?;
        let outside = root.with_extension("outside");
        fs::write(&outside, vec![0u8; 9000])?;
        std::os::unix::fs::symlink(&outside, root.join("data/link"))?;

        let link_bytes = fs::symlink_metadata(root.join("data/link"))?.len();
        assert_eq!(tree_size(&root), Some(3500 + link_bytes));
        assert_eq!(tree_size(&root.join("missing")), None);

        fs::remove_dir_all(&root)?;
        fs::remove_file(outside)
    }
}
