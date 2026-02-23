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
                // Disable shadow to prevent green tint on transparent windows.
                let _ = window.set_shadow(false);

                // Acrylic background — same effect Warp uses on Windows.
                let _ = window_vibrancy::apply_acrylic(&window, Some((10, 14, 18, 166)));
            }

            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            commands::get_drives,
            commands::start_scan,
            commands::get_directory_view,
            commands::validate_path,
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
