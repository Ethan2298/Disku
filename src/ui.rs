use ratatui::layout::{Constraint, Direction, Layout, Rect};
use ratatui::style::{Color, Modifier, Style};
use ratatui::text::{Line, Span};
use ratatui::widgets::{Block, Borders, List, ListItem, ListState, Paragraph};
use ratatui::Frame;

use crate::tree::FileNode;
use crate::utils::{format_size, percent, DriveInfo};

pub struct App {
    pub tree: FileNode,
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

fn centered_rect(percent_x: u16, percent_y: u16, r: Rect) -> Rect {
    let vert = Layout::default()
        .direction(Direction::Vertical)
        .constraints([
            Constraint::Percentage((100 - percent_y) / 2),
            Constraint::Percentage(percent_y),
            Constraint::Percentage((100 - percent_y) / 2),
        ])
        .split(r);
    Layout::default()
        .direction(Direction::Horizontal)
        .constraints([
            Constraint::Percentage((100 - percent_x) / 2),
            Constraint::Percentage(percent_x),
            Constraint::Percentage((100 - percent_x) / 2),
        ])
        .split(vert[1])[1]
}

pub fn draw_scanning(f: &mut Frame, files_scanned: u64, _errors: u64) {
    let area = centered_rect(44, 30, f.area());

    let block = Block::default()
        .title(" disku ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(70, 70, 70)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let top = inner.height.saturating_sub(4) / 2;
    let mut lines: Vec<Line> = (0..top).map(|_| Line::from("")).collect();

    let (status, status_color) = if files_scanned == 0 {
        ("  initializing...", Color::Rgb(150, 150, 150))
    } else {
        ("  scanning...", Color::Rgb(100, 200, 255))
    };

    lines.push(Line::from(Span::styled(
        status,
        Style::default()
            .fg(status_color)
            .add_modifier(Modifier::BOLD),
    )));
    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        if files_scanned == 0 {
            "  reading filesystem...".to_string()
        } else {
            format!("  {} files", files_scanned)
        },
        Style::default().fg(Color::Rgb(100, 100, 100)),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}

pub fn draw(f: &mut Frame, app: &mut App) {
    let area = centered_rect(88, 90, f.area());

    let path_str = app.current_path();
    let size_str = format_size(app.current().size);
    let count = app.current().children.len();
    let sort_label = if app.sort_by_size { "size" } else { "name" };

    let title = format!(
        " {}  {}  {} items  [{}] ",
        path_str, size_str, count, sort_label
    );

    let block = Block::default()
        .title(Span::styled(
            title,
            Style::default().fg(Color::Rgb(120, 120, 120)),
        ))
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(70, 70, 70)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);

    draw_file_list(f, app, chunks[0]);
    draw_footer(f, chunks[1]);
}

fn draw_file_list(f: &mut Frame, app: &mut App, area: Rect) {
    let visible_height = area.height as usize;
    let current = app.current();
    let total_size = current.size;
    let total_children = current.children.len();
    let available_width = area.width as usize;

    let selected = app.list_state.selected().unwrap_or(0);
    let window_start = selected.saturating_sub(visible_height);
    let window_end = (window_start + visible_height * 3).min(total_children);

    let items: Vec<ListItem> = current.children[window_start..window_end]
        .iter()
        .map(|child| format_child_item(child, total_size, available_width))
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::Rgb(35, 35, 50))
            .add_modifier(Modifier::BOLD),
    );

    let mut windowed_state = ListState::default();
    windowed_state.select(Some(selected - window_start));

    f.render_stateful_widget(list, area, &mut windowed_state);
}

fn format_child_item(child: &FileNode, total_size: u64, available_width: usize) -> ListItem<'static> {
    let pct = percent(child.size, total_size);
    let size_str = format_size(child.size);

    // Right side: "  1.23 GB   45.3%" — fixed 18 chars
    let right_width = 18usize;
    // Icon: " + " = 3 chars
    let icon_width = 3usize;
    let name_max = available_width.saturating_sub(right_width + icon_width);

    let name: String = if child.name.chars().count() > name_max {
        let truncated: String = child.name.chars().take(name_max.saturating_sub(1)).collect();
        format!("{}~", truncated)
    } else {
        format!("{:<width$}", child.name, width = name_max)
    };

    let icon = if child.is_dir { "+" } else { " " };
    let name_color = if child.is_dir {
        Color::Rgb(120, 170, 255)
    } else {
        Color::Rgb(180, 180, 180)
    };
    let icon_color = if child.is_dir {
        Color::Rgb(100, 150, 255)
    } else {
        Color::Rgb(60, 60, 60)
    };

    let line = Line::from(vec![
        Span::styled(format!(" {} ", icon), Style::default().fg(icon_color)),
        Span::styled(name, Style::default().fg(name_color)),
        Span::styled(
            format!("{:>9}", size_str),
            Style::default().fg(Color::Rgb(200, 200, 200)),
        ),
        Span::styled(
            format!("  {:>5.1}%", pct),
            Style::default().fg(Color::Rgb(100, 100, 100)),
        ),
    ]);

    ListItem::new(line)
}

fn draw_footer(f: &mut Frame, area: Rect) {
    let k = Style::default().fg(Color::Rgb(100, 200, 255));
    let d = Style::default().fg(Color::Rgb(65, 65, 65));
    let sp = Span::styled("  ", d);

    let line = Line::from(vec![
        Span::styled(" enter", k),
        Span::styled(" open", d),
        sp.clone(),
        Span::styled("bksp", k),
        Span::styled(" back", d),
        sp.clone(),
        Span::styled("j/k", k),
        Span::styled(" nav", d),
        sp.clone(),
        Span::styled("s", k),
        Span::styled(" sort", d),
        sp.clone(),
        Span::styled("q", k),
        Span::styled(" quit", d),
    ]);

    f.render_widget(Paragraph::new(line), area);
}

pub fn draw_start_screen(f: &mut Frame, selected: usize, menu_items: &[&str]) {
    let area = f.area();

    let ascii_art = vec![
        r"    ██████╗ ██╗███████╗██╗  ██╗██╗   ██╗",
        r"    ██╔══██╗██║██╔════╝██║ ██╔╝██║   ██║",
        r"    ██║  ██║██║███████╗█████╔╝ ██║   ██║",
        r"    ██║  ██║██║╚════██║██╔═██╗ ██║   ██║",
        r"    ██████╔╝██║███████║██║  ██╗╚██████╔╝",
        r"    ╚═════╝ ╚═╝╚══════╝╚═╝  ╚═╝ ╚═════╝",
    ];

    let art_height = ascii_art.len() as u16;
    let menu_height = menu_items.len() as u16;
    let content_height = art_height + 1 + 1 + 2 + menu_height + 1 + 1;
    let top_pad = area.height.saturating_sub(content_height) / 2;

    let mut lines: Vec<Line> = Vec::new();

    for _ in 0..top_pad {
        lines.push(Line::from(""));
    }

    let art_width = ascii_art[0].chars().count();
    let left_pad = (area.width as usize).saturating_sub(art_width) / 2;
    let pad_str = " ".repeat(left_pad);

    for row in &ascii_art {
        lines.push(Line::from(Span::styled(
            format!("{}{}", pad_str, row),
            Style::default().fg(Color::Rgb(100, 200, 255)),
        )));
    }

    lines.push(Line::from(""));
    let tagline = "Fast disk usage analyzer for Windows";
    let tagline_pad = " ".repeat((area.width as usize).saturating_sub(tagline.len()) / 2);
    lines.push(Line::from(Span::styled(
        format!("{}{}", tagline_pad, tagline),
        Style::default().fg(Color::Rgb(100, 100, 100)),
    )));

    lines.push(Line::from(""));
    lines.push(Line::from(""));

    let menu_width = 30;
    let menu_pad = " ".repeat((area.width as usize).saturating_sub(menu_width) / 2);

    for (i, item) in menu_items.iter().enumerate() {
        if i == selected {
            lines.push(Line::from(vec![
                Span::raw(&menu_pad),
                Span::styled(
                    "  ▸ ",
                    Style::default()
                        .fg(Color::Rgb(100, 200, 255))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::styled(
                    (*item).to_string(),
                    Style::default()
                        .fg(Color::White)
                        .add_modifier(Modifier::BOLD),
                ),
            ]));
        } else {
            lines.push(Line::from(vec![
                Span::raw(&menu_pad),
                Span::styled("    ", Style::default()),
                Span::styled(
                    (*item).to_string(),
                    Style::default().fg(Color::Rgb(100, 100, 100)),
                ),
            ]));
        }
    }

    lines.push(Line::from(""));
    let hint = "↑/↓ navigate  ·  Enter select  ·  q quit";
    let hint_pad = " ".repeat((area.width as usize).saturating_sub(hint.len()) / 2);
    lines.push(Line::from(Span::styled(
        format!("{}{}", hint_pad, hint),
        Style::default().fg(Color::Rgb(60, 60, 60)),
    )));

    f.render_widget(Paragraph::new(lines), area);
}

pub fn draw_path_input(f: &mut Frame, input: &str) {
    let area = centered_rect(50, 30, f.area());

    let block = Block::default()
        .title(" scan directory ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(70, 70, 70)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let top = inner.height.saturating_sub(5) / 2;
    let mut lines: Vec<Line> = (0..top).map(|_| Line::from("")).collect();

    lines.push(Line::from(Span::styled(
        " path:",
        Style::default().fg(Color::Rgb(100, 100, 100)),
    )));
    lines.push(Line::from(""));

    let field_width = (inner.width as usize).saturating_sub(2);
    let display_input = if input.len() > field_width.saturating_sub(1) {
        &input[input.len() - field_width.saturating_sub(1)..]
    } else {
        input
    };

    lines.push(Line::from(vec![
        Span::raw(" "),
        Span::styled(display_input.to_string(), Style::default().fg(Color::White)),
        Span::styled("█", Style::default().fg(Color::Rgb(100, 200, 255))),
    ]));

    lines.push(Line::from(""));
    lines.push(Line::from(Span::styled(
        " enter confirm  esc cancel",
        Style::default().fg(Color::Rgb(60, 60, 60)),
    )));

    f.render_widget(Paragraph::new(lines), inner);
}

pub fn draw_drive_picker(f: &mut Frame, drives: &[DriveInfo], selected: usize) {
    let area = centered_rect(60, 70, f.area());

    let block = Block::default()
        .title(" select drive ")
        .borders(Borders::ALL)
        .border_style(Style::default().fg(Color::Rgb(70, 70, 70)));

    let inner = block.inner(area);
    f.render_widget(block, area);

    let chunks = Layout::default()
        .direction(Direction::Vertical)
        .constraints([Constraint::Min(3), Constraint::Length(1)])
        .split(inner);

    let available_width = chunks[0].width as usize;

    let items: Vec<ListItem> = drives
        .iter()
        .map(|drive| {
            let used = drive.total.saturating_sub(drive.free);
            let pct = percent(used, drive.total);

            let left = format!(" {}  ", drive.path);
            let right = format!(
                "{}  /  {}   {:>5.1}%",
                format_size(used),
                format_size(drive.total),
                pct
            );
            let gap = available_width
                .saturating_sub(left.chars().count() + right.chars().count());

            let line = Line::from(vec![
                Span::styled(
                    left,
                    Style::default()
                        .fg(Color::Rgb(255, 220, 80))
                        .add_modifier(Modifier::BOLD),
                ),
                Span::raw(" ".repeat(gap)),
                Span::styled(right, Style::default().fg(Color::Rgb(160, 160, 160))),
            ]);

            ListItem::new(line)
        })
        .collect();

    let list = List::new(items).highlight_style(
        Style::default()
            .bg(Color::Rgb(35, 35, 50))
            .add_modifier(Modifier::BOLD),
    );

    let mut state = ListState::default();
    state.select(Some(selected));

    f.render_stateful_widget(list, chunks[0], &mut state);

    f.render_widget(
        Paragraph::new(Line::from(Span::styled(
            " enter scan  j/k nav  q quit",
            Style::default().fg(Color::Rgb(60, 60, 60)),
        ))),
        chunks[1],
    );
}
