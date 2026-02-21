use std::path::PathBuf;
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use disku_core::scanner::ScanProgress;
use disku_core::tree::FileNode;
use disku_core::utils::{self, DriveInfo};

pub struct AppState {
    pub scan_result: Arc<Mutex<Option<FileNode>>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            scan_result: Arc::new(Mutex::new(None)),
        }
    }
}

#[derive(Clone, Serialize)]
#[serde(tag = "kind")]
pub enum ScanEvent {
    Progress {
        files_scanned: u64,
        dirs_scanned: u64,
        errors: u64,
        current_path: String,
    },
    Complete,
}

#[derive(Serialize)]
pub struct DirectoryEntry {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub has_children: bool,
}

#[derive(Serialize)]
pub struct DirectoryView {
    pub path: String,
    pub total_size: u64,
    pub entries: Vec<DirectoryEntry>,
    pub item_count: usize,
}

#[tauri::command]
pub fn get_drives() -> Vec<DriveInfo> {
    utils::detect_drives()
}

#[tauri::command]
pub fn validate_path(path: String) -> bool {
    PathBuf::from(&path).is_dir()
}

#[tauri::command]
pub fn start_scan(
    path: String,
    on_event: Channel<ScanEvent>,
    state: State<'_, AppState>,
) {
    // Clear previous result
    {
        let mut result = state.scan_result.lock().unwrap();
        *result = None;
    }

    let scan_path = PathBuf::from(&path);
    let progress = ScanProgress::new();
    let files_counter = progress.files_scanned.clone();
    let errors_counter = progress.errors.clone();

    let on_event_progress = on_event.clone();
    let current_path = progress.current_path.clone();
    let dirs_counter = progress.dirs_scanned.clone();
    let scan_done = Arc::new(AtomicBool::new(false));
    let done_flag = scan_done.clone();

    // Spawn progress reporter
    let files_for_progress = files_counter.clone();
    let dirs_for_progress = dirs_counter.clone();
    let errors_for_progress = errors_counter.clone();
    let progress_handle = std::thread::spawn(move || {
        loop {
            std::thread::sleep(std::time::Duration::from_millis(100));
            let files = files_for_progress.load(Ordering::Relaxed);
            let dirs = dirs_for_progress.load(Ordering::Relaxed);
            let errors = errors_for_progress.load(Ordering::Relaxed);
            let cp = current_path.lock().unwrap().clone();
            let _ = on_event_progress.send(ScanEvent::Progress {
                files_scanned: files,
                dirs_scanned: dirs,
                errors,
                current_path: cp,
            });
            if done_flag.load(Ordering::Relaxed) {
                break;
            }
        }
    });

    // Clone the Arc to move into the scan thread
    let scan_result = state.scan_result.clone();

    std::thread::spawn(move || {
        let p = ScanProgress {
            files_scanned: files_counter,
            dirs_scanned: dirs_counter,
            errors: errors_counter,
            current_path: progress.current_path.clone(),
        };

        let root = {
            #[cfg(windows)]
            {
                let path_str = scan_path.to_string_lossy();
                if path_str.len() >= 2 && path_str.as_bytes()[1] == b':' {
                    let drive_letter = path_str.chars().next().unwrap();
                    if let Some(root) = disku_core::mft_scanner::scan_mft(drive_letter, &p) {
                        root
                    } else {
                        disku_core::scanner::scan(&scan_path, &p)
                    }
                } else {
                    disku_core::scanner::scan(&scan_path, &p)
                }
            }

            #[cfg(target_os = "macos")]
            {
                disku_core::mac_scanner::scan_bulk(&scan_path, &p)
            }

            #[cfg(all(not(windows), not(target_os = "macos")))]
            {
                disku_core::scanner::scan(&scan_path, &p)
            }
        };

        // Store result
        {
            let mut result = scan_result.lock().unwrap();
            *result = Some(root);
        }

        // Signal progress reporter to stop
        scan_done.store(true, Ordering::Relaxed);
        let _ = progress_handle.join();

        // Send complete event
        let _ = on_event.send(ScanEvent::Complete);
    });
}

#[tauri::command]
pub fn get_directory_view(
    nav_path: Vec<usize>,
    sort_by_size: bool,
    state: State<'_, AppState>,
) -> Option<DirectoryView> {
    let mut result = state.scan_result.lock().unwrap();
    let root = result.as_mut()?;

    // Apply sort
    if sort_by_size {
        root.sort_by_size();
    } else {
        root.sort_by_name();
    }

    // Navigate to the requested node
    let mut node = &*root;
    let mut path_parts = vec![node.name.clone()];
    for &idx in &nav_path {
        if idx < node.children.len() {
            node = &node.children[idx];
            path_parts.push(node.name.clone());
        } else {
            return None;
        }
    }

    let entries: Vec<DirectoryEntry> = node
        .children
        .iter()
        .map(|child| DirectoryEntry {
            name: child.name.clone(),
            size: child.size,
            is_dir: child.is_dir,
            has_children: child.is_dir && !child.children.is_empty(),
        })
        .collect();

    let item_count = entries.len();

    Some(DirectoryView {
        path: path_parts.join(std::path::MAIN_SEPARATOR_STR),
        total_size: node.size,
        entries,
        item_count,
    })
}
