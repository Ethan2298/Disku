use std::collections::HashMap;
use std::path::{Path, PathBuf};

use rayon::prelude::*;

#[derive(Debug, Clone, serde::Serialize)]
pub struct FileNode {
    pub name: String,
    pub size: u64,
    pub is_dir: bool,
    pub children: Vec<FileNode>,
}

impl FileNode {
    pub fn new_file(name: String, size: u64) -> Self {
        Self {
            name,
            size,
            is_dir: false,
            children: Vec::new(),
        }
    }

    pub fn new_dir(name: String) -> Self {
        Self {
            name,
            size: 0,
            is_dir: true,
            children: Vec::new(),
        }
    }

    pub fn sort_by_size(&mut self) {
        self.children
            .par_sort_unstable_by(|a, b| b.size.cmp(&a.size));
        self.children
            .par_iter_mut()
            .for_each(|child| child.sort_by_size());
    }

    pub fn sort_by_name(&mut self) {
        self.children
            .par_sort_unstable_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        self.children
            .par_iter_mut()
            .for_each(|child| child.sort_by_name());
    }

    /// Remove a child by name and return its size so callers can adjust parent sizes.
    /// Uses case-insensitive comparison for NTFS compatibility.
    pub fn remove_child_by_name(&mut self, name: &str) -> Option<u64> {
        if let Some(pos) = self
            .children
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(name))
        {
            let removed = self.children.remove(pos);
            let freed = removed.size;
            self.size = self.size.saturating_sub(freed);
            Some(freed)
        } else {
            None
        }
    }
}

/// Given an absolute path to a directory and the tree root, find the nav_path
/// indices to navigate TO that directory. Returns empty vec if target is the root.
///
/// Uses case-insensitive comparison on Windows (NTFS is case-insensitive).
pub fn find_nav_path(root: &FileNode, target: &std::path::Path) -> Option<Vec<usize>> {
    let root_path = std::path::Path::new(&root.name);

    // If target IS the root, return empty nav path
    // Case-insensitive comparison for Windows paths
    if root_path
        .to_string_lossy()
        .eq_ignore_ascii_case(&target.to_string_lossy())
    {
        return Some(Vec::new());
    }

    // Strip the root prefix to get the relative portion
    let relative = target.strip_prefix(root_path).ok()?;
    let components: Vec<&str> = relative
        .components()
        .filter_map(|c| {
            if let std::path::Component::Normal(s) = c {
                s.to_str()
            } else {
                None
            }
        })
        .collect();

    if components.is_empty() {
        return Some(Vec::new());
    }

    let mut nav_path = Vec::new();
    let mut node = root;

    for &comp in &components {
        // Case-insensitive match for NTFS compatibility
        let idx = node
            .children
            .iter()
            .position(|c| c.name.eq_ignore_ascii_case(comp))?;
        nav_path.push(idx);
        node = &node.children[idx];
    }

    Some(nav_path)
}

const MAX_DEPTH: usize = 512;

/// Build a tree from a flat list of (path, is_dir, size) entries.
/// Used by the jwalk fallback scanner.
pub fn build_tree(root_path: &Path, entries: Vec<(PathBuf, bool, u64)>) -> FileNode {
    let root_name = root_path.to_string_lossy().to_string();
    let mut root = FileNode::new_dir(root_name);

    let mut dir_children: HashMap<PathBuf, Vec<(PathBuf, bool, u64)>> = HashMap::new();

    for (path, is_dir, size) in &entries {
        if path == root_path {
            continue;
        }
        if let Some(parent) = path.parent() {
            dir_children
                .entry(parent.to_path_buf())
                .or_default()
                .push((path.clone(), *is_dir, *size));
        }
    }

    fn build_recursive(
        node: &mut FileNode,
        node_path: &Path,
        dir_children: &HashMap<PathBuf, Vec<(PathBuf, bool, u64)>>,
        depth: usize,
    ) {
        if !node.is_dir || depth >= MAX_DEPTH {
            return;
        }

        if let Some(children) = dir_children.get(node_path) {
            for (path, is_dir, size) in children {
                let name = path
                    .file_name()
                    .map(|n| n.to_string_lossy().to_string())
                    .unwrap_or_else(|| path.to_string_lossy().to_string());
                if *is_dir {
                    let mut child = FileNode::new_dir(name);
                    build_recursive(&mut child, path, dir_children, depth + 1);
                    child.size = child.children.iter().map(|c| c.size).sum();
                    node.children.push(child);
                } else {
                    node.children.push(FileNode::new_file(name, *size));
                }
            }
        }

        node.size = node.children.iter().map(|c| c.size).sum();
    }

    build_recursive(&mut root, root_path, &dir_children, 0);
    root.sort_by_size();
    root
}
