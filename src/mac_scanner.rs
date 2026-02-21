use std::ffi::{CStr, CString};
use std::os::unix::ffi::OsStrExt;
use std::path::Path;
use std::sync::atomic::Ordering;

use crate::scanner::ScanProgress;
use crate::tree::FileNode;

// macOS attribute constants
const ATTR_BIT_MAP_COUNT: u16 = 5;
const ATTR_CMN_RETURNED_ATTRS: u32 = 0x80000000;
const ATTR_CMN_NAME: u32 = 0x00000001;
const ATTR_CMN_OBJTYPE: u32 = 0x00000008;
const ATTR_CMN_ERROR: u32 = 0x20000000;
const ATTR_FILE_DATALENGTH: u32 = 0x00000200;
const VREG: u32 = 1; // regular file
const VDIR: u32 = 2; // directory

const BULK_BUF_SIZE: usize = 256 * 1024; // 256 KB buffer

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

#[repr(C)]
struct AttrBuf {
    length: u32,
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

/// Scan a directory tree using macOS getattrlistbulk for fast enumeration.
pub fn scan_bulk(root: &Path, progress: &ScanProgress) -> FileNode {
    let root_name = root
        .file_name()
        .map(|n| n.to_string_lossy().to_string())
        .unwrap_or_else(|| root.to_string_lossy().to_string());

    let mut node = FileNode::new_dir(root_name);
    scan_dir_recursive(root, &mut node, progress);
    node.size = node.children.iter().map(|c| c.size).sum();
    node.sort_by_size();
    node
}

fn scan_dir_recursive(dir_path: &Path, parent_node: &mut FileNode, progress: &ScanProgress) {
    let entries = match read_dir_bulk(dir_path) {
        Some(e) => e,
        None => {
            // Fall back to simple readdir + stat for this directory
            read_dir_fallback(dir_path, progress, parent_node);
            return;
        }
    };

    for entry in entries {
        progress.files_scanned.fetch_add(1, Ordering::Relaxed);

        if entry.is_dir {
            let child_path = dir_path.join(&entry.name);
            let mut child_node = FileNode::new_dir(entry.name);
            scan_dir_recursive(&child_path, &mut child_node, progress);
            child_node.size = child_node.children.iter().map(|c| c.size).sum();
            parent_node.children.push(child_node);
        } else {
            parent_node
                .children
                .push(FileNode::new_file(entry.name, entry.size));
        }
    }
}

/// Use getattrlistbulk to read all entries in a directory in bulk.
/// Returns None if the syscall is unavailable or fails.
fn read_dir_bulk(dir_path: &Path) -> Option<Vec<BulkEntry>> {
    let c_path = CString::new(dir_path.as_os_str().as_bytes()).ok()?;
    let fd = unsafe { libc::open(c_path.as_ptr(), libc::O_RDONLY | libc::O_DIRECTORY) };
    if fd < 0 {
        return None;
    }

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
    let mut results = Vec::new();

    loop {
        let count = unsafe {
            getattrlistbulk(
                fd,
                &alist,
                buf.as_mut_ptr() as *mut libc::c_void,
                BULK_BUF_SIZE,
                0,
            )
        };

        if count < 0 {
            // syscall error — close fd and bail
            unsafe { libc::close(fd) };
            return None;
        }
        if count == 0 {
            // no more entries
            break;
        }

        let mut offset = 0usize;
        for _ in 0..count {
            if offset + 4 > BULK_BUF_SIZE {
                break;
            }

            let entry_length =
                u32::from_ne_bytes(buf[offset..offset + 4].try_into().unwrap()) as usize;

            if entry_length == 0 || offset + entry_length > BULK_BUF_SIZE {
                break;
            }

            if let Some(entry) = parse_bulk_entry(&buf[offset..offset + entry_length]) {
                results.push(entry);
            }

            offset += entry_length;
        }
    }

    unsafe { libc::close(fd) };
    Some(results)
}

/// Parse a single entry from the getattrlistbulk buffer.
fn parse_bulk_entry(data: &[u8]) -> Option<BulkEntry> {
    // Layout after the 4-byte length:
    //   returned_attrs: AttrList (20 bytes)
    //   error: u32 (4 bytes) — if ATTR_CMN_ERROR was returned
    //   name: attrreference_t { offset: i32, length: u32 } (8 bytes)
    //   objtype: u32 (4 bytes)
    //   [file_datalength: u64 (8 bytes)] — only for files if fileattr was returned

    if data.len() < 4 + 20 {
        return None;
    }

    let mut pos = 4; // skip entry length

    // Read returned attributes bitmap
    let ret_commonattr = u32::from_ne_bytes(data[pos + 4..pos + 8].try_into().ok()?);
    let ret_fileattr = u32::from_ne_bytes(data[pos + 16..pos + 20].try_into().ok()?);
    pos += 20; // skip returned AttrList

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
    let name_data_start = (pos as i32 + name_ref_offset) as usize;
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
fn read_dir_fallback(dir_path: &Path, progress: &ScanProgress, parent_node: &mut FileNode) {
    let entries = match std::fs::read_dir(dir_path) {
        Ok(e) => e,
        Err(_) => {
            progress.errors.fetch_add(1, Ordering::Relaxed);
            return;
        }
    };

    for entry in entries.flatten() {
        progress.files_scanned.fetch_add(1, Ordering::Relaxed);

        let meta = match entry.metadata() {
            Ok(m) => m,
            Err(_) => {
                progress.errors.fetch_add(1, Ordering::Relaxed);
                continue;
            }
        };

        let name = entry.file_name().to_string_lossy().to_string();

        if meta.is_dir() {
            let mut child_node = FileNode::new_dir(name);
            scan_dir_recursive(&entry.path(), &mut child_node, progress);
            child_node.size = child_node.children.iter().map(|c| c.size).sum();
            parent_node.children.push(child_node);
        } else {
            parent_node
                .children
                .push(FileNode::new_file(name, meta.len()));
        }
    }
}
