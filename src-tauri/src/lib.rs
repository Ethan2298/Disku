mod commands;

use commands::AppState;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .manage(AppState::default())
        .setup(|app| {
            let window = app.get_webview_window("main").unwrap();

            #[cfg(target_os = "macos")]
            {
                use window_vibrancy::{NSVisualEffectMaterial, NSVisualEffectState};
                window_vibrancy::apply_vibrancy(
                    &window,
                    NSVisualEffectMaterial::HudWindow,
                    Some(NSVisualEffectState::Active),
                    Some(12.0),
                )
                .expect("Failed to apply vibrancy");
            }

            #[cfg(target_os = "windows")]
            {
                let _ = window_vibrancy::apply_blur(&window, Some((10, 14, 18, 166)));
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
