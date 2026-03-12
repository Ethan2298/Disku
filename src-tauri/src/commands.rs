use std::path::{Path, PathBuf};
use std::sync::atomic::{AtomicBool, Ordering};
use std::sync::{Arc, Mutex};

use serde::Serialize;
use tauri::ipc::Channel;
use tauri::State;

use disku_core::scanner::ScanProgress;
use disku_core::tree::{self, FileNode};
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
pub struct DeleteResult {
    pub path: String,
    pub success: bool,
    pub error: Option<String>,
    pub bytes_freed: u64,
}

/// Check whether a path is a protected system location that must not be deleted.
fn is_protected_path(path: &Path) -> bool {
    let path_str = path.to_string_lossy();
    // Normalise to lowercase backslash form for Windows comparison.
    let norm = path_str.replace('/', "\\").to_lowercase();
    let norm = norm.trim_end_matches('\\');

    // Drive roots  (C:\, D:\, …)
    if norm.len() == 2 && norm.as_bytes()[1] == b':' {
        return true;
    }

    let blocked: &[&str] = &[
        r"c:\windows",
        r"c:\program files",
        r"c:\program files (x86)",
        r"c:\programdata",
        r"c:\recovery",
        r"c:\$recycle.bin",
        r"c:\boot",
        r"c:\bootmgr",
        r"c:\efi",
        r"c:\users",
    ];

    for &b in blocked {
        if norm == b || norm.starts_with(&format!("{b}\\")) {
            // Allow items *inside* a user profile (C:\Users\<name>\…), but
            // block C:\Users itself and the profile root (C:\Users\<name>).
            if b == r"c:\users" {
                // Count segments after "C:\Users\"
                let after = &norm[r"c:\users\".len()..];
                // If there are at least two more segments (profile + something inside),
                // the path is inside a profile → allow it.
                if after.contains('\\') {
                    return false;
                }
                return true;
            }
            return true;
        }
    }

    false
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
        let mut result = state.scan_result.lock().unwrap_or_else(|e| e.into_inner());
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
            let cp = current_path.lock().unwrap_or_else(|e| e.into_inner()).clone();
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
                    let drive_letter = match path_str.chars().next() {
                        Some(c) => c,
                        None => return,
                    };
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
            let mut result = scan_result.lock().unwrap_or_else(|e| e.into_inner());
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
    let mut result = state.scan_result.lock().unwrap_or_else(|e| e.into_inner());
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

#[tauri::command]
pub fn delete_entries(
    nav_path: Vec<usize>,
    entry_indices: Vec<usize>,
    sort_by_size: bool,
    state: State<'_, AppState>,
) -> Vec<DeleteResult> {
    let mut result = state.scan_result.lock().unwrap_or_else(|e| e.into_inner());
    let Some(root) = result.as_mut() else {
        return vec![];
    };

    // Apply the same sort so indices match the frontend view.
    if sort_by_size {
        root.sort_by_size();
    } else {
        root.sort_by_name();
    }

    // Resolve the absolute path of the root for later joining.
    let root_abs = PathBuf::from(&root.name);

    // Navigate to the parent node described by nav_path.
    let mut node_path = root_abs.clone();
    let mut parent = &mut *root;
    for &idx in &nav_path {
        if idx >= parent.children.len() {
            return vec![];
        }
        node_path = node_path.join(&parent.children[idx].name);
        parent = &mut parent.children[idx];
    }

    // Collect child names + sizes for the requested indices (resolve before mutating).
    let mut targets: Vec<(String, u64, PathBuf)> = Vec::new();
    for &idx in &entry_indices {
        if idx < parent.children.len() {
            let child = &parent.children[idx];
            let abs = node_path.join(&child.name);
            targets.push((child.name.clone(), child.size, abs));
        }
    }

    // Phase 1: attempt filesystem deletions (no tree mutation yet).
    let mut results = Vec::with_capacity(targets.len());
    let mut deleted_names: Vec<(String, u64)> = Vec::new();

    for (name, size, abs_path) in &targets {
        if is_protected_path(abs_path) {
            results.push(DeleteResult {
                path: abs_path.to_string_lossy().to_string(),
                success: false,
                error: Some("Protected system path".to_string()),
                bytes_freed: 0,
            });
            continue;
        }

        let delete_result = if abs_path.is_dir() {
            std::fs::remove_dir_all(abs_path)
        } else {
            std::fs::remove_file(abs_path)
        };

        match delete_result {
            Ok(()) => {
                deleted_names.push((name.clone(), *size));
                results.push(DeleteResult {
                    path: abs_path.to_string_lossy().to_string(),
                    success: true,
                    error: None,
                    bytes_freed: *size,
                });
            }
            Err(e) => {
                results.push(DeleteResult {
                    path: abs_path.to_string_lossy().to_string(),
                    success: false,
                    error: Some(e.to_string()),
                    bytes_freed: 0,
                });
            }
        }
    }

    // Phase 2: mutate the tree — remove children and propagate sizes.
    if !deleted_names.is_empty() {
        let total_freed: u64 = deleted_names.iter().map(|(_, s)| *s).sum();

        // Navigate again to the parent and remove children.
        let mut parent2 = &mut *root;
        for &idx in &nav_path {
            if idx < parent2.children.len() {
                parent2 = &mut parent2.children[idx];
            }
        }
        for (name, _) in &deleted_names {
            parent2.remove_child_by_name(name);
        }

        // Subtract total freed from the root and each ancestor along nav_path.
        root.size = root.size.saturating_sub(total_freed);
        let mut ancestor = &mut *root;
        for &idx in &nav_path {
            if idx < ancestor.children.len() {
                ancestor = &mut ancestor.children[idx];
                // The parent node already had its size reduced by remove_child_by_name,
                // but the *grandparent* chain needs manual adjustment. Only subtract
                // from nodes above the parent.
            }
        }
        // Actually, remove_child_by_name already adjusts the direct parent's size.
        // We only need to adjust ancestors *above* the parent. Re-walk, skipping the last:
        let mut anc = &mut *root;
        for &idx in &nav_path {
            // anc is an ancestor above the parent; subtract freed size.
            // (root.size was already adjusted above — but remove_child_by_name
            // doesn't touch it, so we need to fix every node from root down to
            // but NOT including the parent itself, since remove_child_by_name
            // already adjusted the parent.)
            if idx < anc.children.len() {
                anc = &mut anc.children[idx];
            }
        }
        // Simpler approach: just recalculate sizes along the path.
        // Walk root → parent, setting each node's size to sum of children.
        fn resum(node: &mut FileNode) {
            if node.is_dir {
                node.size = node.children.iter().map(|c| c.size).sum();
            }
        }
        resum(root);
        let mut anc2 = &mut *root;
        for &idx in &nav_path {
            if idx < anc2.children.len() {
                anc2 = &mut anc2.children[idx];
                resum(anc2);
            }
        }
    }

    results
}

#[tauri::command]
pub fn delete_entries_by_path(
    paths: Vec<String>,
    sort_by_size: bool,
    state: State<'_, AppState>,
) -> Vec<DeleteResult> {
    let mut result = state.scan_result.lock().unwrap_or_else(|e| e.into_inner());
    let Some(root) = result.as_mut() else {
        return vec![];
    };

    // Apply the same sort so tree indices match.
    if sort_by_size {
        root.sort_by_size();
    } else {
        root.sort_by_name();
    }

    let mut results = Vec::with_capacity(paths.len());
    let mut deleted_paths: Vec<PathBuf> = Vec::new();

    // Phase 1: attempt filesystem deletions.
    for path_str in &paths {
        let abs_path = PathBuf::from(path_str);

        if is_protected_path(&abs_path) {
            results.push(DeleteResult {
                path: path_str.clone(),
                success: false,
                error: Some("Protected system path".to_string()),
                bytes_freed: 0,
            });
            continue;
        }

        let delete_result = if abs_path.is_dir() {
            std::fs::remove_dir_all(&abs_path)
        } else {
            std::fs::remove_file(&abs_path)
        };

        match delete_result {
            Ok(()) => {
                deleted_paths.push(abs_path);
                results.push(DeleteResult {
                    path: path_str.clone(),
                    success: true,
                    error: None,
                    bytes_freed: 0, // Will be filled from tree
                });
            }
            Err(e) => {
                results.push(DeleteResult {
                    path: path_str.clone(),
                    success: false,
                    error: Some(e.to_string()),
                    bytes_freed: 0,
                });
            }
        }
    }

    // Phase 2: mutate the tree for successfully deleted paths.
    // Group by parent directory: Vec<(child_name, original_full_path)>
    let mut by_parent: std::collections::HashMap<PathBuf, Vec<(String, String)>> =
        std::collections::HashMap::new();

    for del_path in &deleted_paths {
        if let Some(parent) = del_path.parent() {
            if let Some(name) = del_path.file_name() {
                by_parent
                    .entry(parent.to_path_buf())
                    .or_default()
                    .push((
                        name.to_string_lossy().to_string(),
                        del_path.to_string_lossy().to_string(),
                    ));
            }
        }
    }

    for (parent_path, name_paths) in &by_parent {
        // Find nav_path for parent directory
        let Some(nav_indices) = tree::find_nav_path(root, parent_path) else {
            continue; // path not in tree, skip
        };

        // Navigate to parent in the tree
        let mut parent_node = &mut *root;
        let mut nav_ok = true;
        for &idx in &nav_indices {
            if idx < parent_node.children.len() {
                parent_node = &mut parent_node.children[idx];
            } else {
                nav_ok = false;
                break;
            }
        }
        if !nav_ok {
            continue; // index out of bounds, skip this parent
        }

        for (name, full_path) in name_paths {
            if let Some(freed) = parent_node.remove_child_by_name(name) {
                // Update bytes_freed in results — match by exact full path
                for r in results.iter_mut() {
                    if r.success && r.path == *full_path {
                        r.bytes_freed = freed;
                        break;
                    }
                }
            }
        }
    }

    // Phase 3: recalculate sizes from root down through all affected branches.
    fn resum_recursive(node: &mut FileNode) {
        if node.is_dir {
            for child in node.children.iter_mut() {
                resum_recursive(child);
            }
            node.size = node.children.iter().map(|c| c.size).sum();
        }
    }
    resum_recursive(root);

    results
}
