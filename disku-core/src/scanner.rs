use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicU64, Ordering};
use std::sync::{Arc, Mutex};

use jwalk::WalkDir;

use crate::tree::{build_tree, FileNode};

pub struct ScanProgress {
    pub files_scanned: Arc<AtomicU64>,
    pub dirs_scanned: Arc<AtomicU64>,
    pub errors: Arc<AtomicU64>,
    pub current_path: Arc<Mutex<String>>,
}

impl ScanProgress {
    pub fn new() -> Self {
        Self {
            files_scanned: Arc::new(AtomicU64::new(0)),
            dirs_scanned: Arc::new(AtomicU64::new(0)),
            errors: Arc::new(AtomicU64::new(0)),
            current_path: Arc::new(Mutex::new(String::new())),
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
                    let path = e.path();
                    let is_dir = e.file_type().is_dir();
                    if is_dir {
                        progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);
                        if let Ok(mut cp) = progress.current_path.try_lock() {
                            *cp = path.to_string_lossy().to_string();
                        }
                    } else {
                        progress.files_scanned.fetch_add(1, Ordering::Relaxed);
                    }
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
