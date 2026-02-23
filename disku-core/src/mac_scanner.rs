use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::Ordering;

use rayon::prelude::*;

use crate::scanner::ScanProgress;
use crate::tree::FileNode;

// macOS attribute constants
const ATTR_BIT_MAP_COUNT: u16 = 5;
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x80000000;
const ATTR_CMN_NAME: u32 = 0x00000001;
const ATTR_CMN_OBJTYPE: u32 = 0x00000008;
const ATTR_CMN_ERROR: u32 = 0x20000000;
const ATTR_FILE_DATALENGTH: u32 = 0x00000200;
const VDIR: u32 = 2; // directory

const BULK_BUF_SIZE: usize = 256 * 1024; // 256 KB buffer
const MAX_DEPTH: usize = 512;

#[repr(C, packed(4))]
struct AttrList {
    bitmapcount: u16,
    reserved: u16,
    commonattr: u32,
    volattr: u32,
    dirattr: u32,
    fileattr: u32,
    forkattr: u32,
}

/// RAII wrapper for a file descriptor that closes on drop.
struct OwnedFd(libc::c_int);

impl Drop for OwnedFd {
    fn drop(&mut self) {
        unsafe { libc::close(self.0) };
    }
}

extern "C" {
    fn getattrlistbulk(
        dirfd: libc::c_int,
        alist: *const AttrList,
        attribute_buffer: *mut libc::c_void,
        buffer_size: libc::size_t,
        options: u64,
    ) -> libc::c_int;
}

struct BulkEntry {
    name: String,
    is_dir: bool,
    size: u64,
}

/// Get the device ID for a path (used to avoid crossing filesystem boundaries).
fn get_dev(path: &Path) -> Option<u64> {
    use std::os::unix::fs::MetadataExt;
    std::fs::symlink_metadata(path).map(|m| m.dev()).ok()
}

/// Scan a directory tree using macOS getattrlistbulk for fast enumeration.
pub fn scan_bulk(root: &Path, progress: &ScanProgress) -> FileNode {
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    let root_dev = get_dev(root);
    let children = scan_dir_recursive(root, progress, root_dev, 0);
    let mut node = FileNode::new_dir(root_name);
    node.children = children;
    node.size = node.children.iter().map(|c| c.size).sum();
    node.sort_by_size();
    node
}

fn scan_dir_recursive(dir_path: &Path, progress: &ScanProgress, root_dev: Option<u64>, depth: usize) -> Vec<FileNode> {
    if depth >= MAX_DEPTH {
        return Vec::new();
    }

    if let Ok(mut cp) = progress.current_path.try_lock() {
        *cp = dir_path.to_string_lossy().to_string();
    }

    let entries = match read_dir_bulk(dir_path) {
        Some(e) => e,
        None => {
            return read_dir_fallback(dir_path, progress, root_dev, depth);
        }
    };

    let mut file_nodes: Vec<FileNode> = Vec::with_capacity(entries.len());
    let mut dir_entries: Vec<(String, std::path::PathBuf)> = Vec::with_capacity(entries.len() / 8);

    for entry in entries {
        if entry.is_dir {
            progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);
        } else {
            progress.files_scanned.fetch_add(1, Ordering::Relaxed);
        }

        if entry.is_dir {
            let child_path = dir_path.join(&entry.name);
            // Skip directories on different filesystems (network mounts, iCloud, etc.)
            if let Some(rd) = root_dev {
                if get_dev(&child_path) != Some(rd) {
                    continue;
                }
            }
            dir_entries.push((entry.name, child_path));
        } else {
            file_nodes.push(FileNode::new_file(entry.name, entry.size));
        }
    }

    let dir_nodes: Vec<FileNode> = dir_entries
        .into_par_iter()
        .map(|(name, child_path)| {
            let children = scan_dir_recursive(&child_path, progress, root_dev, depth + 1);
            let mut child_node = FileNode::new_dir(name);
            child_node.children = children;
            child_node.size = child_node.children.iter().map(|c| c.size).sum();
            child_node
        })
        .collect();

    file_nodes.extend(dir_nodes);
    file_nodes
}

/// Use getattrlistbulk to read all entries in a directory in bulk.
/// Returns None if the syscall is unavailable or fails.
fn read_dir_bulk(dir_path: &Path) -> Option<Vec<BulkEntry>> {
    let c_path = CString::new(dir_path.as_os_str().as_bytes()).ok()?;
    let raw_fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
    if raw_fd < 0 {
        return None;
    }
    let fd = OwnedFd(raw_fd);

    let alist = AttrList {
        bitmapcount: ATTR_BIT_MAP_COUNT,
        reserved: 0,
        commonattr: ATTR_CMN_RETURNED_ATTRS | ATTR_CMN_NAME | ATTR_CMN_OBJTYPE | ATTR_CMN_ERROR,
        volattr: 0,
        dirattr: 0,
        fileattr: ATTR_FILE_DATALENGTH,
        forkattr: 0,
    };

    let mut buf = vec![0u8; BULK_BUF_SIZE];
    let mut results = Vec::with_capacity(256);

    loop {
        let count = unsafe {
            getattrlistbulk(
                fd.0,
                &alist,
                buf.as_mut_ptr() as *mut libc::c_void,
                BULK_BUF_SIZE,
                0,
            )
        };

        if count < 0 {
            return None;
        }
        if count == 0 {
            break;
        }

        let mut offset = 0usize;
        for _ in 0..count {
            if offset + 4 > BULK_BUF_SIZE {
                break;
            }

            let Ok(len_bytes) = buf[offset..offset + 4].try_into() else {
                break;
            };
            let entry_length = u32::from_ne_bytes(len_bytes) as usize;

            if entry_length == 0 || offset + entry_length > BULK_BUF_SIZE {
                break;
            }

            if let Some(entry) = parse_bulk_entry(&buf[offset..offset + entry_length]) {
                results.push(entry);
            }

            offset += entry_length;
        }
    }

    Some(results)
}

/// Parse a single entry from the getattrlistbulk buffer.
fn parse_bulk_entry(data: &[u8]) -> Option<BulkEntry> {
    // Layout after the 4-byte length:
    //   returned_attrs: attribute_set_t (20 bytes: 5 x u32)
    //     { commonattr, volattr, dirattr, fileattr, forkattr }
    //   error: u32 (4 bytes) — only if ATTR_CMN_ERROR bit set in returned commonattr
    //   name: attrreference_t { offset: i32, length: u32 } (8 bytes)
    //   objtype: u32 (4 bytes)
    //   [file_datalength: u64 (8 bytes)] — only for files if fileattr was returned

    const ATTR_SET_SIZE: usize = 20; // attribute_set_t = 5 x u32
    if data.len() < 4 + ATTR_SET_SIZE {
        return None;
    }

    let mut pos = 4; // skip entry length

    // Read returned attribute_set_t (NOT AttrList — no bitmapcount/reserved header)
    let ret_commonattr = u32::from_ne_bytes(data[pos..pos + 4].try_into().ok()?);
    let ret_fileattr = u32::from_ne_bytes(data[pos + 12..pos + 16].try_into().ok()?);
    pos += ATTR_SET_SIZE; // skip attribute_set_t (20 bytes)

    // Error attribute (if present)
    if ret_commonattr & ATTR_CMN_ERROR != 0 {
        let err = u32::from_ne_bytes(data[pos..pos + 4].try_into().ok()?);
        pos += 4;
        if err != 0 {
            return None; // skip entries with errors
        }
    }

    // Name attribute: attrreference_t { offset: i32, length: u32 }
    if ret_commonattr & ATTR_CMN_NAME == 0 {
        return None;
    }
    let name_ref_offset = i32::from_ne_bytes(data[pos..pos + 4].try_into().ok()?);
    let _name_ref_length = u32::from_ne_bytes(data[pos + 4..pos + 8].try_into().ok()?);
    let name_data_start = usize::try_from(
        (pos as i64).checked_add(name_ref_offset as i64)?
    ).ok()?;
    pos += 8;

    let name = if name_data_start < data.len() {
        let name_slice = &data[name_data_start..];
        match CStr::from_bytes_until_nul(name_slice) {
            Ok(cs) => cs.to_string_lossy().to_string(),
            Err(_) => {
                // manually find null terminator
                let end = name_slice.iter().position(|&b| b == 0).unwrap_or(name_slice.len());
                String::from_utf8_lossy(&name_slice[..end]).to_string()
            }
        }
    } else {
        return None;
    };

    // Skip . and ..
    if name == "." || name == ".." {
        return None;
    }

    // Object type
    let obj_type = if ret_commonattr & ATTR_CMN_OBJTYPE != 0 {
        let t = u32::from_ne_bytes(data[pos..pos + 4].try_into().ok()?);
        pos += 4;
        t
    } else {
        return None;
    };

    let is_dir = obj_type == VDIR;

    // File data length (only present for regular files when fileattr returned)
    let size = if !is_dir && (ret_fileattr & ATTR_FILE_DATALENGTH != 0) {
        u64::from_ne_bytes(data[pos..pos + 8].try_into().ok()?)
    } else {
        0
    };

    Some(BulkEntry { name, is_dir, size })
}

/// Simple readdir + stat fallback for a single directory when getattrlistbulk fails.
fn read_dir_fallback(dir_path: &Path, progress: &ScanProgress, root_dev: Option<u64>, depth: usize) -> Vec<FileNode> {
    let entries = match std::fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return Vec::new();
        }
    };

    let mut file_nodes: Vec<FileNode> = Vec::new();
    let mut dir_entries: Vec<(String, std::path::PathBuf)> = Vec::new();

    for entry in entries {
        let entry = match entry {
            Ok(e) => e,
            Err(_) => {
                progress.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };
        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                progress.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        let name = entry.file_name().to_string_lossy().to_string();

        if meta.is_dir() {
            progress.dirs_scanned.fetch_add(1, Ordering::Relaxed);
            // Skip directories on different filesystems (network mounts, iCloud, etc.)
            if let Some(rd) = root_dev {
                if get_dev(&entry.path()) != Some(rd) {
                    continue;
                }
            }
            dir_entries.push((name, entry.path()));
        } else {
            progress.files_scanned.fetch_add(1, Ordering::Relaxed);
            file_nodes.push(FileNode::new_file(name, meta.len()));
        }
    }

    let dir_nodes: Vec<FileNode> = dir_entries
        .into_par_iter()
        .map(|(name, child_path)| {
            let children = scan_dir_recursive(&child_path, progress, root_dev, depth + 1);
            let mut child_node = FileNode::new_dir(name);
            child_node.children = children;
            child_node.size = child_node.children.iter().map(|c| c.size).sum();
            child_node
        })
        .collect();

    file_nodes.extend(dir_nodes);
    file_nodes
}
