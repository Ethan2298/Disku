pub fn format_size(bytes: u64) -> String {
    const KB: u64 = 1024;
    const MB: u64 = 1024 * KB;
    const GB: u64 = 1024 * MB;
    const TB: u64 = 1024 * GB;

    if bytes >= TB {
        format!("{:.1} TB", bytes as f64 / TB as f64)
    } else if bytes >= GB {
        format!("{:.1} GB", bytes as f64 / GB as f64)
    } else if bytes >= MB {
        format!("{:.1} MB", bytes as f64 / MB as f64)
    } else if bytes >= KB {
        format!("{:.1} KB", bytes as f64 / KB as f64)
    } else {
        format!("{} B", bytes)
    }
}

pub fn percent(part: u64, total: u64) -> f64 {
    if total == 0 {
        0.0
    } else {
        (part as f64 / total as f64) * 100.0
    }
}

#[derive(Debug, Clone)]
pub struct DriveInfo {
    pub path: String,
    pub total: u64,
    pub free: u64,
}

/// Detect available drives/volumes on the current platform.
#[cfg(windows)]
pub fn detect_drives() -> Vec<DriveInfo> {
    use std::ffi::OsString;
    use std::os::windows::ffi::OsStrExt;

    let mask = unsafe { windows_get_logical_drives() };
    let mut drives = Vec::new();

    for i in 0..26u32 {
        if mask & (1 << i) != 0 {
            let letter = (b'A' + i as u8) as char;
            let root = format!("{}:\\", letter);

            let wide: Vec<u16> = OsString::from(&root)
                .encode_wide()
                .chain(std::iter::once(0))
                .collect();

            let mut free_bytes: u64 = 0;
            let mut total_bytes: u64 = 0;
            let ok = unsafe {
                GetDiskFreeSpaceExW(
                    wide.as_ptr(),
                    std::ptr::null_mut(),
                    &mut total_bytes,
                    &mut free_bytes,
                )
            };

            if ok != 0 {
                drives.push(DriveInfo {
                    path: root,
                    total: total_bytes,
                    free: free_bytes,
                });
            }
        }
    }

    drives
}

#[cfg(windows)]
extern "system" {
    fn GetLogicalDrives() -> u32;
    fn GetDiskFreeSpaceExW(
        lpDirectoryName: *const u16,
        lpFreeBytesAvailableToCaller: *mut u64,
        lpTotalNumberOfBytes: *mut u64,
        lpTotalNumberOfFreeBytes: *mut u64,
    ) -> i32;
}

#[cfg(windows)]
unsafe fn windows_get_logical_drives() -> u32 {
    unsafe { GetLogicalDrives() }
}

/// Detect mounted volumes on macOS.
#[cfg(target_os = "macos")]
pub fn detect_drives() -> Vec<DriveInfo> {
    let mut drives = Vec::new();

    // Always include root
    if let Some(info) = statvfs_drive("/") {
        drives.push(info);
    }

    // Enumerate /Volumes
    if let Ok(entries) = std::fs::read_dir("/Volumes") {
        for entry in entries.flatten() {
            let path = entry.path();
            let path_str = path.to_string_lossy().to_string();

            // Skip symlinks that point back to root
            if let Ok(target) = std::fs::read_link(&path) {
                if target == std::path::Path::new("/") {
                    continue;
                }
            }

            if let Some(info) = statvfs_drive(&path_str) {
                // Avoid duplicate of root
                if info.total == drives.first().map(|d| d.total).unwrap_or(0)
                    && info.free == drives.first().map(|d| d.free).unwrap_or(0)
                {
                    continue;
                }
                drives.push(info);
            }
        }
    }

    drives
}

/// Detect mounted filesystems on Linux.
#[cfg(target_os = "linux")]
pub fn detect_drives() -> Vec<DriveInfo> {
    let mut drives = Vec::new();
    let mut seen_devs = std::collections::HashSet::new();

    if let Ok(content) = std::fs::read_to_string("/proc/mounts") {
        for line in content.lines() {
            let parts: Vec<&str> = line.split_whitespace().collect();
            if parts.len() < 2 {
                continue;
            }
            let device = parts[0];
            let mount_point = parts[1];

            // Only real block devices
            if !device.starts_with("/dev/") {
                continue;
            }
            // Skip snap/loop
            if device.contains("loop") {
                continue;
            }
            if seen_devs.contains(device) {
                continue;
            }
            seen_devs.insert(device.to_string());

            if let Some(info) = statvfs_drive(mount_point) {
                if info.total > 0 {
                    drives.push(info);
                }
            }
        }
    }

    // Fallback: at least show root
    if drives.is_empty() {
        if let Some(info) = statvfs_drive("/") {
            drives.push(info);
        }
    }

    drives
}

/// Use statvfs to get total/free bytes for a mount point.
#[cfg(unix)]
fn statvfs_drive(path: &str) -> Option<DriveInfo> {
    use std::ffi::CString;
    use std::mem::MaybeUninit;

    let c_path = CString::new(path).ok()?;
    let mut stat = MaybeUninit::<libc::statvfs>::uninit();

    let ret = unsafe { libc::statvfs(c_path.as_ptr(), stat.as_mut_ptr()) };
    if ret != 0 {
        return None;
    }

    let stat = unsafe { stat.assume_init() };
    let total = stat.f_blocks as u64 * stat.f_frsize as u64;
    let free = stat.f_bavail as u64 * stat.f_frsize as u64;

    Some(DriveInfo {
        path: path.to_string(),
        total,
        free,
    })
}
