use std::collections::HashMap;
use std::path::{Path, PathBuf};

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
        self.children.sort_unstable_by(|a, b| b.size.cmp(&a.size));
        for child in &mut self.children {
            child.sort_by_size();
        }
    }

    pub fn sort_by_name(&mut self) {
        self.children.sort_unstable_by(|a, b| a.name.to_lowercase().cmp(&b.name.to_lowercase()));
        for child in &mut self.children {
            child.sort_by_name();
        }
    }
}

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
    ) {
        if !node.is_dir {
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
                    build_recursive(&mut child, path, dir_children);
                    child.size = child.children.iter().map(|c| c.size).sum();
                    node.children.push(child);
                } else {
                    node.children.push(FileNode::new_file(name, *size));
                }
            }
        }

        node.size = node.children.iter().map(|c| c.size).sum();
    }

    build_recursive(&mut root, root_path, &dir_children);
    root.sort_by_size();
    root
}
