mod execution;
mod scenario;
use tauri::Manager;

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(execution::RunRegistry::default())
        .on_window_event(|window, event| {
            if matches!(event, tauri::WindowEvent::Destroyed) {
                window.state::<execution::RunRegistry>().cancel_active();
            }
        })
        .plugin(tauri_plugin_opener::init())
        .plugin(tauri_plugin_dialog::init())
        .invoke_handler(tauri::generate_handler![
            scenario::validate_scenario,
            scenario::open_scenario,
            scenario::save_scenario_json,
            execution::start_scenario,
            execution::cancel_scenario,
            execution::save_run_report
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
