use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::Arc;

use jwalk::WalkDir;

use crate::tree::{build_tree, FileNode};

pub struct ScanProgress {
    pub files_scanned: Arc<AtomicU64>,
    pub errors: Arc<AtomicU64>,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            files_scanned: Arc::new(AtomicU64::new(0)),
            errors: Arc::new(AtomicU64::new(0)),
        }
    }
}

pub fn scan(root: &Path, progress: &ScanProgress) -> FileNode {
    // jwalk parallelizes directory reading across threads
    let flat: Vec<(PathBuf, bool, u64)> = WalkDir::new(root)
        .skip_hidden(false)
        .into_iter()
        .filter_map(|entry| {
            match entry {
                Ok(e) => {
                    progress.files_scanned.fetch_add(1, Ordering::Relaxed);
                    let path = e.path();
                    let is_dir = e.file_type().is_dir();
                    let size = if is_dir {
                        0
                    } else {
                        e.metadata().map(|m| m.len()).unwrap_or(0)
                    };
                    Some((path, is_dir, size))
                }
                Err(_) => {
                    progress.errors.fetch_add(1, Ordering::Relaxed);
                    None
                }
            }
        })
        .collect();

    build_tree(root, flat)
}
