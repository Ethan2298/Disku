use std::collections::HashMap;
use std::sync::atomic::Ordering;

use ntfs_reader::api::NtfsAttributeType;
use ntfs_reader::mft::Mft;
use ntfs_reader::volume::Volume;

use crate::scanner::ScanProgress;
use crate::tree::FileNode;

const ROOT_RECORD: u64 = 5;

struct MftEntry {
    name: String,
    parent_ref: u64,
    size: u64,
    is_dir: bool,
}

/// Scan an NTFS volume by reading the MFT directly.
/// Requires admin privileges. Returns None on any failure.
pub fn scan_mft(drive_letter: char, progress: &ScanProgress) -> Option<FileNode> {
    let volume_path = format!("\\\\.\\{}:", drive_letter);
    let volume = Volume::new(&volume_path).ok()?;
    let mft = Mft::new(volume).ok()?;

    // Use Vec indexed by record number for O(1) lookups
    let max_record = mft.max_record as usize;
    let mut entries: Vec<Option<MftEntry>> = Vec::with_capacity(max_record);
    entries.resize_with(max_record + 1, || None);

    mft.iterate_files(|file| {
        progress.files_scanned.fetch_add(1, Ordering::Relaxed);

        let record_num = file.number() as usize;
        let is_dir = file.is_directory();

        let Some(fname) = file.get_best_file_name(&mft) else {
            return;
        };

        let name = fname.to_string();
        let parent_ref = fname.parent();

        let size = if is_dir {
            0
        } else {
            get_data_size(file)
        };

        if record_num < entries.len() {
            entries[record_num] = Some(MftEntry {
                name,
                parent_ref,
                size,
                is_dir,
            });
        }
    });

    // Build parent -> children map
    let mut children_map: HashMap<u64, Vec<usize>> = HashMap::new();
    for (ref_num, entry) in entries.iter().enumerate() {
        if let Some(e) = entry {
            if e.parent_ref != ref_num as u64 {
                children_map
                    .entry(e.parent_ref)
                    .or_default()
                    .push(ref_num);
            }
        }
    }

    let root_name = format!("{}:\\", drive_letter);

    let mut root = FileNode::new_dir(root_name.clone());
    if let Some(child_refs) = children_map.get(&ROOT_RECORD) {
        for &child_ref in child_refs {
            root.children.push(build_subtree(
                child_ref, &entries, &children_map,
            ));
        }
    }
    root.size = root.children.iter().map(|c| c.size).sum();
    root.name = root_name;
    root.sort_by_size();
    Some(root)
}

fn get_data_size(file: &ntfs_reader::file::NtfsFile) -> u64 {
    file.get_attribute(NtfsAttributeType::Data)
        .map(|attr| {
            if attr.header.is_non_resident == 0 {
                attr.resident_header()
                    .map(|rh| rh.value_length as u64)
                    .unwrap_or(0)
            } else {
                attr.nonresident_header()
                    .map(|nrh| nrh.data_size)
                    .unwrap_or(0)
            }
        })
        .unwrap_or(0)
}

fn build_subtree(
    ref_num: usize,
    entries: &[Option<MftEntry>],
    children_map: &HashMap<u64, Vec<usize>>,
) -> FileNode {
    let entry = entries[ref_num].as_ref().unwrap();

    let mut children = Vec::new();
    if entry.is_dir {
        if let Some(child_refs) = children_map.get(&(ref_num as u64)) {
            children.reserve(child_refs.len());
            for &child_ref in child_refs {
                if child_ref == ref_num {
                    continue;
                }
                if entries.get(child_ref).and_then(|e| e.as_ref()).is_some() {
                    children.push(build_subtree(child_ref, entries, children_map));
                }
            }
        }
    }

    let size = if entry.is_dir {
        children.iter().map(|c| c.size).sum()
    } else {
        entry.size
    };

    FileNode {
        name: entry.name.clone(),
        size,
        is_dir: entry.is_dir,
        children,
    }
}
