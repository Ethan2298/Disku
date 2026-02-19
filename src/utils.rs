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

/// Detect available drives on Windows by probing A:-Z:
pub fn detect_drives() -> Vec<DriveInfo> {
    use std::os::windows::ffi::OsStrExt;
    use std::ffi::OsString;

    // Use GetLogicalDrives to find which letters exist
    let mask = unsafe { windows_get_logical_drives() };
    let mut drives = Vec::new();

    for i in 0..26u32 {
        if mask & (1 << i) != 0 {
            let letter = (b'A' + i as u8) as char;
            let root = format!("{}:\\", letter);

            // Get disk space info
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

// FFI declarations for Windows APIs
extern "system" {
    fn GetLogicalDrives() -> u32;
    fn GetDiskFreeSpaceExW(
        lpDirectoryName: *const u16,
        lpFreeBytesAvailableToCaller: *mut u64,
        lpTotalNumberOfBytes: *mut u64,
        lpTotalNumberOfFreeBytes: *mut u64,
    ) -> i32;
}

unsafe fn windows_get_logical_drives() -> u32 {
    unsafe { GetLogicalDrives() }
}
