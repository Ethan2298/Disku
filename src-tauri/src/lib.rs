mod commands;

use commands::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let Some(window) = app.get_webview_window("main") else {
                return Ok(());
            };

            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState};
                let _ = window_vibrancy::apply_vibrancy(
                    &window,
                    NSVisualEffectMaterial::HudWindow,
                    Some(NSVisualEffectState::Active),
                    Some(10.0),
                );
            }

            #[cfg(target_os = "windows")]
            {
                // Set window icon for taskbar/title bar.
                // Try resource dir (bundled app), then fall back to relative path (dev mode).
                let icon_path = app
                    .path()
                    .resource_dir()
                    .map(|d| d.join("icons/128x128.png"))
                    .ok()
                    .filter(|p| p.exists())
                    .unwrap_or_else(|| {
                        std::path::PathBuf::from(env!("CARGO_MANIFEST_DIR"))
                            .join("icons/128x128.png")
                    });
                if let Ok(icon) = tauri::image::Image::from_path(&icon_path) {
                    let _ = window.set_icon(icon);
                }

                // Remove native title bar — custom window controls rendered by frontend.
                let _ = window.set_decorations(false);

                // Disable shadow to prevent green tint on transparent windows.
                let _ = window.set_shadow(false);

                // Acrylic background — same effect Warp uses on Windows.
                let _ = window_vibrancy::apply_acrylic(&window, Some((6, 8, 12, 250)));

                // Round window corners (Windows 11+, silently ignored on 10).
                use raw_window_handle::HasWindowHandle;
                if let Ok(wh) = window.window_handle() {
                    if let raw_window_handle::RawWindowHandle::Win32(handle) = wh.as_raw() {
                        const DWMWA_WINDOW_CORNER_PREFERENCE: u32 = 33;
                        const DWMWCP_ROUND: u32 = 2;
                        #[link(name = "dwmapi")]
                        extern "system" {
                            fn DwmSetWindowAttribute(
                                hwnd: isize,
                                attr: u32,
                                value: *const u32,
                                size: u32,
                            ) -> i32;
                        }
                        unsafe {
                            DwmSetWindowAttribute(
                                handle.hwnd.get(),
                                DWMWA_WINDOW_CORNER_PREFERENCE,
                                &DWMWCP_ROUND,
                                std::mem::size_of::<u32>() as u32,
                            );
                        }
                    }
                }
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_drives,
            commands::start_scan,
            commands::get_directory_view,
            commands::validate_path,
            commands::delete_entries,
            commands::delete_entries_by_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
