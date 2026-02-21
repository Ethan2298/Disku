mod ui;

use std::io;
use std::path::PathBuf;
use std::sync::atomic::Ordering;
use std::thread;
use std::time::Duration;

use crossterm::event::{self, Event, KeyCode, KeyEventKind};
use crossterm::terminal::{disable_raw_mode, enable_raw_mode, EnterAlternateScreen, LeaveAlternateScreen};
use crossterm::execute;
use ratatui::backend::CrosstermBackend;
use ratatui::Terminal;

use disku_core::scanner::{scan, ScanProgress};
use disku_core::tree::FileNode;
use ui::{draw, draw_drive_picker, draw_scanning, draw_start_screen, App};
use disku_core::utils::detect_drives;

fn main() -> io::Result<()> {
    // If a path was passed as CLI arg, use it directly
    let explicit_path = std::env::args().nth(1).map(PathBuf::from);

    // Set up terminal
    enable_raw_mode()?;
    let mut stdout = io::stdout();
    execute!(stdout, EnterAlternateScreen)?;
    let backend = CrosstermBackend::new(stdout);
    let mut terminal = Terminal::new(backend)?;

    // Determine root path: either from CLI arg, or start screen -> drive picker
    let root_path = if let Some(path) = explicit_path {
        path.canonicalize().unwrap_or(path)
    } else {
        // Show start screen
        let menu_items = if cfg!(windows) {
            vec!["Scan Drive", "Scan Directory", "Quit"]
        } else {
            vec!["Scan Volume", "Scan Directory", "Quit"]
        };
        let mut menu_sel: usize = 0;

        let menu_choice = loop {
            let sel = menu_sel;
            let items = &menu_items;
            terminal.draw(|f| draw_start_screen(f, sel, items))?;

            if event::poll(Duration::from_millis(50))? {
                if let Event::Key(key) = event::read()? {
                    if key.kind != KeyEventKind::Press {
                        continue;
                    }
                    match key.code {
                        KeyCode::Char('q') | KeyCode::Esc => {
                            cleanup_terminal()?;
                            return Ok(());
                        }
                        KeyCode::Up | KeyCode::Char('k') => {
                            if menu_sel > 0 {
                                menu_sel -= 1;
                            }
                        }
                        KeyCode::Down | KeyCode::Char('j') => {
                            if menu_sel + 1 < menu_items.len() {
                                menu_sel += 1;
                            }
                        }
                        KeyCode::Enter => {
                            break menu_sel;
                        }
                        _ => {}
                    }
                }
            }
        };

        match menu_choice {
            0 => {
                // Scan Drive/Volume -- show drive picker
                let drives = detect_drives();
                if drives.is_empty() {
                    cleanup_terminal()?;
                    eprintln!("No drives found.");
                    return Ok(());
                }

                let mut selected: usize = 0;
                let chosen = loop {
                    let drives_ref = &drives;
                    let sel = selected;
                    terminal.draw(|f| draw_drive_picker(f, drives_ref, sel))?;

                    if event::poll(Duration::from_millis(50))? {
                        if let Event::Key(key) = event::read()? {
                            if key.kind != KeyEventKind::Press {
                                continue;
                            }
                            match key.code {
                                KeyCode::Char('q') | KeyCode::Esc => {
                                    cleanup_terminal()?;
                                    return Ok(());
                                }
                                KeyCode::Up | KeyCode::Char('k') => {
                                    if selected > 0 {
                                        selected -= 1;
                                    }
                                }
                                KeyCode::Down | KeyCode::Char('j') => {
                                    if selected + 1 < drives.len() {
                                        selected += 1;
                                    }
                                }
                                KeyCode::Enter => {
                                    break drives[selected].path.clone();
                                }
                                _ => {}
                            }
                        }
                    }
                };
                PathBuf::from(chosen)
            }
            1 => {
                // Scan Directory -- prompt for path input
                let mut input = String::new();
                loop {
                    let input_ref = &input;
                    terminal.draw(|f| {
                        ui::draw_path_input(f, input_ref);
                    })?;

                    if event::poll(Duration::from_millis(50))? {
                        if let Event::Key(key) = event::read()? {
                            if key.kind != KeyEventKind::Press {
                                continue;
                            }
                            match key.code {
                                KeyCode::Esc => {
                                    cleanup_terminal()?;
                                    return Ok(());
                                }
                                KeyCode::Enter => {
                                    let p = PathBuf::from(&input);
                                    if p.is_dir() {
                                        break;
                                    }
                                    // If invalid, just keep looping
                                }
                                KeyCode::Backspace => {
                                    input.pop();
                                }
                                KeyCode::Char(c) => {
                                    input.push(c);
                                }
                                _ => {}
                            }
                        }
                    }
                }
                let p = PathBuf::from(&input);
                p.canonicalize().unwrap_or(p)
            }
            _ => {
                // Quit
                cleanup_terminal()?;
                return Ok(());
            }
        }
    };

    // Scan in background thread
    let progress = ScanProgress::new();
    let scan_files = progress.files_scanned.clone();
    let scan_errors = progress.errors.clone();
    let scan_path = root_path.clone();

    let scan_handle = thread::spawn(move || {
        let p = ScanProgress {
            files_scanned: scan_files,
            dirs_scanned: progress.dirs_scanned.clone(),
            errors: scan_errors,
            current_path: progress.current_path.clone(),
        };

        // Platform-specific fast path, falling back to jwalk
        #[cfg(windows)]
        {
            let path_str = scan_path.to_string_lossy();
            if path_str.len() >= 2 && path_str.as_bytes()[1] == b':' {
                let drive_letter = path_str.chars().next().unwrap();
                if let Some(root) = disku_core::mft_scanner::scan_mft(drive_letter, &p) {
                    return root;
                }
            }
        }

        #[cfg(target_os = "macos")]
        {
            return disku_core::mac_scanner::scan_bulk(&scan_path, &p);
        }

        // Universal fallback (Windows non-NTFS, Linux, etc.)
        #[allow(unreachable_code)]
        scan(&scan_path, &p)
    });

    // Show scanning progress
    loop {
        let files = progress.files_scanned.load(Ordering::Relaxed);
        let errors = progress.errors.load(Ordering::Relaxed);

        terminal.draw(|f| draw_scanning(f, files, errors))?;

        if scan_handle.is_finished() {
            break;
        }

        if event::poll(Duration::from_millis(100))? {
            if let Event::Key(key) = event::read()? {
                if key.kind == KeyEventKind::Press
                    && matches!(key.code, KeyCode::Char('q') | KeyCode::Esc)
                {
                    cleanup_terminal()?;
                    return Ok(());
                }
            }
        }
    }

    let root: FileNode = scan_handle.join().expect("scan thread panicked");

    // Run the interactive TUI
    let mut app = App::new(root);

    loop {
        terminal.draw(|f| draw(f, &mut app))?;

        if event::poll(Duration::from_millis(50))? {
            if let Event::Key(key) = event::read()? {
                if key.kind != KeyEventKind::Press {
                    continue;
                }
                match key.code {
                    KeyCode::Char('q') | KeyCode::Esc => break,
                    KeyCode::Up | KeyCode::Char('k') => app.move_up(),
                    KeyCode::Down | KeyCode::Char('j') => app.move_down(),
                    KeyCode::Enter => app.enter(),
                    KeyCode::Backspace => app.go_back(),
                    KeyCode::Char('s') => app.toggle_sort(),
                    _ => {}
                }
            }
        }
    }

    cleanup_terminal()?;
    Ok(())
}

fn cleanup_terminal() -> io::Result<()> {
    disable_raw_mode()?;
    execute!(io::stdout(), LeaveAlternateScreen)?;
    Ok(())
}
