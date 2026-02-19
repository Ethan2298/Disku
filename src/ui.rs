use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::tree::FileNode;
use crate::utils::{format_size, percent, DriveInfo};

pub struct App {
    pub tree: FileNode,
    /// Index path from root to current directory (e.g. [2, 0] = root.children[2].children[0])
    pub nav_path: Vec<usize>,
    pub list_state: ListState,
    pub sort_by_size: bool,
}

impl App {
    pub fn new(root: FileNode) -> Self {
        let mut list_state = ListState::default();
        if !root.children.is_empty() {
            list_state.select(Some(0));
        }
        Self {
            tree: root,
            nav_path: Vec::new(),
            list_state,
            sort_by_size: true,
        }
    }

    pub fn current(&self) -> &FileNode {
        let mut node = &self.tree;
        for &idx in &self.nav_path {
            node = &node.children[idx];
        }
        node
    }

    fn current_mut(&mut self) -> &mut FileNode {
        let mut node = &mut self.tree;
        for &idx in &self.nav_path {
            node = &mut node.children[idx];
        }
        node
    }

    pub fn current_path(&self) -> String {
        let mut parts = vec![self.tree.name.clone()];
        let mut node = &self.tree;
        for &idx in &self.nav_path {
            node = &node.children[idx];
            parts.push(node.name.clone());
        }
        parts.join("\\")
    }

    pub fn move_up(&mut self) {
        if let Some(i) = self.list_state.selected() {
            if i > 0 {
                self.list_state.select(Some(i - 1));
            }
        }
    }

    pub fn move_down(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let len = self.current().children.len();
            if i + 1 < len {
                self.list_state.select(Some(i + 1));
            }
        }
    }

    pub fn enter(&mut self) {
        if let Some(i) = self.list_state.selected() {
            let current = self.current();
            if let Some(child) = current.children.get(i) {
                if child.is_dir && !child.children.is_empty() {
                    self.nav_path.push(i);
                    self.list_state.select(Some(0));
                }
            }
        }
    }

    pub fn go_back(&mut self) {
        if !self.nav_path.is_empty() {
            self.nav_path.pop();
            self.list_state.select(Some(0));
        }
    }

    pub fn toggle_sort(&mut self) {
        self.sort_by_size = !self.sort_by_size;
        let by_size = self.sort_by_size;
        let current = self.current_mut();
        if by_size {
            current.sort_by_size();
        } else {
            current.sort_by_name();
        }
    }
}

pub fn draw_scanning(f: &mut Frame, files_scanned: u64, errors: u64) {
    let area = f.area();
    let block = Block::default()
        .title(" diskus — scanning... ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 50)));

    let text = vec![
        Line::from(""),
        Line::from(Span::styled(
            "  Scanning filesystem...",
            Style::default()
                .fg(Color::Yellow)
                .add_modifier(Modifier::BOLD),
        )),
        Line::from(""),
        Line::from(format!("  Files scanned: {}", files_scanned)),
        Line::from(format!("  Permission errors: {}", errors)),
        Line::from(""),
        Line::from(Span::styled(
            "  Please wait...",
            Style::default().fg(Color::DarkGray),
        )),
    ];

    let paragraph = Paragraph::new(text).block(block);
    f.render_widget(paragraph, area);
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3), // header
            Constraint::Min(5),   // file list
            Constraint::Length(3), // footer
        ])
        .split(f.area());

    draw_header(f, app, chunks[0]);
    draw_file_list(f, app, chunks[1]);
    draw_footer(f, chunks[2]);
}

fn draw_header(f: &mut Frame, app: &App, area: Rect) {
    let current = app.current();
    let path_str = app.current_path();
    let size_str = format_size(current.size);
    let sort_label = if app.sort_by_size { "size" } else { "name" };
    let count = current.children.len();

    let line = Line::from(vec![
        Span::styled(" diskus", Style::default().fg(Color::Rgb(100, 200, 255)).add_modifier(Modifier::BOLD)),
        Span::styled(" \u{2502} ", Style::default().fg(Color::Rgb(60, 60, 60))),
        Span::styled(path_str.to_string(), Style::default().fg(Color::White)),
        Span::styled("   ", Style::default()),
        Span::styled(size_str, Style::default().fg(Color::Rgb(255, 220, 80)).add_modifier(Modifier::BOLD)),
        Span::styled(format!("  {} items", count), Style::default().fg(Color::Rgb(120, 120, 120))),
        Span::styled(format!("  sort:{}", sort_label), Style::default().fg(Color::Rgb(80, 80, 80))),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 50)));

    let paragraph = Paragraph::new(line).block(block);
    f.render_widget(paragraph, area);
}

fn draw_file_list(f: &mut Frame, app: &mut App, area: Rect) {
    let visible_height = area.height.saturating_sub(2) as usize; // minus borders
    let current = app.current();
    let total_size = current.size;
    let total_children = current.children.len();
    let bar_width = (area.width as usize).saturating_sub(6);

    // Only format items in the visible window (+ buffer)
    let selected = app.list_state.selected().unwrap_or(0);
    let window_start = selected.saturating_sub(visible_height);
    let window_end = (window_start + visible_height * 3).min(total_children);

    let items: Vec<ListItem> = current.children[window_start..window_end]
        .iter()
        .map(|child| format_child_item(child, total_size, bar_width))
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 50)));

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    // Adjust list state to the windowed range
    let mut windowed_state = ListState::default();
    windowed_state.select(Some(selected - window_start));

    f.render_stateful_widget(list, area, &mut windowed_state);
}

/// Pick a bar color based on what percentage of the parent this item uses
fn bar_color(pct: f64) -> Color {
    if pct >= 50.0 {
        Color::Rgb(255, 80, 80)   // red — dominant
    } else if pct >= 25.0 {
        Color::Rgb(255, 180, 50)  // orange
    } else if pct >= 10.0 {
        Color::Rgb(255, 220, 80)  // yellow
    } else if pct >= 3.0 {
        Color::Rgb(100, 220, 100) // green
    } else {
        Color::Rgb(80, 160, 200)  // blue-gray — tiny
    }
}

fn format_child_item(child: &FileNode, total_size: u64, bar_width: usize) -> ListItem<'static> {
    let pct = percent(child.size, total_size);
    let size_str = format_size(child.size);

    let max_bar = bar_width.saturating_sub(42);
    let filled_f = (pct / 100.0) * max_bar as f64;
    let filled = filled_f as usize;
    let empty = max_bar.saturating_sub(filled + 1);

    // Smooth transition: use partial block at the boundary
    let partials = ['█', '▉', '▊', '▋', '▌', '▍', '▎', '▏'];
    let frac = filled_f - filled as f64;
    let partial_idx = (frac * 8.0) as usize;
    let partial_char = if filled < max_bar && partial_idx > 0 {
        partials[8 - partial_idx].to_string()
    } else {
        String::new()
    };

    let bar_filled = "█".repeat(filled);
    let bar_empty = " ".repeat(empty);

    let color = bar_color(pct);

    let name: String = if child.name.chars().count() > 24 {
        let truncated: String = child.name.chars().take(23).collect();
        format!("{}~", truncated)
    } else {
        format!("{:<24}", child.name)
    };

    let name_color = if child.is_dir {
        Color::Rgb(130, 180, 255) // light blue
    } else {
        Color::Rgb(200, 200, 200) // light gray
    };

    let icon = if child.is_dir { "+" } else { " " };

    let line = Line::from(vec![
        Span::styled(
            format!(" {} ", icon),
            Style::default().fg(if child.is_dir { Color::Rgb(130, 180, 255) } else { Color::DarkGray }),
        ),
        Span::styled(bar_filled, Style::default().fg(color)),
        Span::styled(partial_char, Style::default().fg(color)),
        Span::styled(bar_empty, Style::default().fg(Color::Rgb(40, 40, 40))),
        Span::raw("  "),
        Span::styled(name, Style::default().fg(name_color)),
        Span::styled(
            format!("{:>9}", size_str),
            Style::default().fg(Color::Rgb(220, 220, 220)),
        ),
        Span::styled(
            format!("  {:>5.1}%", pct),
            Style::default().fg(Color::Rgb(120, 120, 120)),
        ),
    ]);

    ListItem::new(line)
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let key_style = Style::default().fg(Color::Rgb(100, 200, 255));
    let sep = Span::styled(" \u{2502} ", Style::default().fg(Color::Rgb(50, 50, 50)));
    let desc = Style::default().fg(Color::Rgb(120, 120, 120));

    let help = Line::from(vec![
        Span::styled(" Enter ", key_style),
        Span::styled("open", desc),
        sep.clone(),
        Span::styled("Bksp ", key_style),
        Span::styled("back", desc),
        sep.clone(),
        Span::styled("j/k ", key_style),
        Span::styled("nav", desc),
        sep.clone(),
        Span::styled("s ", key_style),
        Span::styled("sort", desc),
        sep,
        Span::styled("q ", key_style),
        Span::styled("quit", desc),
    ]);

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 50)));

    let paragraph = Paragraph::new(help).block(block);
    f.render_widget(paragraph, area);
}

pub fn draw_drive_picker(f: &mut Frame, drives: &[DriveInfo], selected: usize) {
    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Length(3),
            Constraint::Min(5),
            Constraint::Length(3),
        ])
        .split(f.area());

    let header = Block::default()
        .title(" diskus — Select a drive to scan ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 50)));
    f.render_widget(header, chunks[0]);

    let bar_width = (chunks[1].width as usize).saturating_sub(6);
    let items: Vec<ListItem> = drives
        .iter()
        .map(|drive| {
            let used = drive.total.saturating_sub(drive.free);
            let pct = percent(used, drive.total);
            let max_bar = bar_width.saturating_sub(50);
            let filled = ((pct / 100.0) * max_bar as f64) as usize;
            let empty = max_bar.saturating_sub(filled);

            let bar_filled = "█".repeat(filled);
            let bar_empty = " ".repeat(empty);

            let color = bar_color(pct);

            let line = Line::from(vec![
                Span::styled(
                    format!("  {}  ", drive.path),
                    Style::default().fg(Color::Rgb(255, 220, 80)).add_modifier(Modifier::BOLD),
                ),
                Span::styled(bar_filled, Style::default().fg(color)),
                Span::styled(bar_empty, Style::default().fg(Color::Rgb(40, 40, 40))),
                Span::styled(
                    format!("  {} / {}", format_size(used), format_size(drive.total)),
                    Style::default().fg(Color::Rgb(200, 200, 200)),
                ),
                Span::styled(
                    format!("  {:.0}%", pct),
                    Style::default().fg(Color::Rgb(120, 120, 120)),
                ),
            ]);

            ListItem::new(line)
        })
        .collect();

    let block = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 50)));

    let mut state = ListState::default();
    state.select(Some(selected));

    let list = List::new(items)
        .block(block)
        .highlight_style(
            Style::default()
                .bg(Color::DarkGray)
                .add_modifier(Modifier::BOLD),
        );

    f.render_stateful_widget(list, chunks[1], &mut state);

    let help = Line::from(vec![
        Span::styled(" [Enter] ", Style::default().fg(Color::Yellow)),
        Span::raw("scan drive  "),
        Span::styled("[Up/Down] ", Style::default().fg(Color::Yellow)),
        Span::raw("select  "),
        Span::styled("[q/Esc] ", Style::default().fg(Color::Yellow)),
        Span::raw("quit"),
    ]);
    let footer = Block::default()
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(50, 50, 50)));
    let paragraph = Paragraph::new(help).block(footer);
    f.render_widget(paragraph, chunks[2]);
}
