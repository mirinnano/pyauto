use serde::{Deserialize, Serialize};
use std::sync::Arc;
use tauri::{AppHandle, Emitter, Runtime, State};
use tauri_plugin_shell::process::CommandEvent;
use tauri_plugin_shell::ShellExt;
use tokio::sync::Mutex;

mod engine;
use engine::license;
use engine::RustBot;

use windows::Win32::Foundation::{BOOL, HWND, LPARAM};
use windows::Win32::UI::WindowsAndMessaging::{
    EnumWindows, GetWindowTextLengthW, GetWindowTextW, IsWindowVisible,
};

struct AppState {
    // In Tauri v2, the process handle is CommandChild
    bot_handle: Arc<Mutex<Option<tauri_plugin_shell::process::CommandChild>>>,
    rust_bot: Arc<Mutex<RustBot>>,
}

impl Default for AppState {
    fn default() -> Self {
        Self {
            bot_handle: Arc::new(Mutex::new(None)),
            rust_bot: Arc::new(Mutex::new(RustBot::new())),
        }
    }
}

#[derive(Serialize, Deserialize, Clone)]
struct BridgeMessage {
    r#type: String,
    data: serde_json::Value,
    message: Option<String>,
}

#[tauri::command]
async fn start_rust_engine(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
) -> Result<(), String> {
    // Load config
    let config_path = "../pyauto_config.json";
    let config_content = std::fs::read_to_string(config_path).unwrap_or_else(|_| "{}".to_string());
    let config: engine::AppConfig =
        serde_json::from_str(&config_content).unwrap_or_else(|_| engine::AppConfig {
            target_window: None,
            global_action_key: Some("e".to_string()),
            hold_duration: Some(1.2),
            rules: Some(vec![]),
            discord_webhook_url: None,
            notify_on_success: Some(true),
            notify_on_failure: Some(false),
            notify_on_error: Some(true),
            account_data: None,
            gas_url: None,
            api_secret: None,
        });

    let mut bot = state.rust_bot.lock().await;
    bot.start(app, config);
    Ok(())
}

#[tauri::command]
async fn stop_rust_engine(state: State<'_, AppState>) -> Result<(), String> {
    let mut bot = state.rust_bot.lock().await;
    bot.stop();
    Ok(())
}

#[tauri::command]
async fn start_automation<R: Runtime>(
    app: AppHandle<R>,
    state: State<'_, AppState>,
    config: serde_json::Value,
) -> Result<(), String> {
    let mut handle_lock = state.bot_handle.lock().await;

    // This will be replaced based on view_file content
    // CHECK FIRST  if let Some(child) = handle_lock.take() {
    //     let _ = child.kill();
    // }

    let shell = app.shell();
    let (mut rx, mut child) = shell
        .command("python")
        .args(["../sidecar/bridge.py"]) // Note the ../ because cargo run starts in src-tauri
        .spawn()
        .map_err(|e| e.to_string())?;

    // Send initial configuration via stdin
    let stdin_msg = serde_json::json!({
        "action": "start",
        "config": config
    });

    child
        .write(format!("{}\n", stdin_msg.to_string()).as_bytes())
        .map_err(|e| e.to_string())?;

    *handle_lock = Some(child);

    // Monitor output
    tauri::async_runtime::spawn(async move {
        while let Some(event) = rx.recv().await {
            match event {
                CommandEvent::Stdout(line_bytes) => {
                    let line_str = String::from_utf8_lossy(&line_bytes).to_string();

                    // Prevent terminal flooding from Base64 images or very long lines
                    if !line_str.contains("preview") && line_str.len() < 500 {
                        println!("Python: {}", line_str.trim());
                    }

                    // Parse JSON from Python
                    if let Ok(json_val) = serde_json::from_str::<serde_json::Value>(&line_str) {
                        let msg_type = json_val["type"].as_str().unwrap_or("unknown");

                        let _ = app.emit(
                            "bot-event",
                            BridgeMessage {
                                r#type: msg_type.to_string(),
                                data: json_val["data"].clone(),
                                message: json_val["message"].as_str().map(|s| s.to_string()),
                            },
                        );
                    }
                }
                CommandEvent::Stderr(line) => {
                    let line_str = String::from_utf8_lossy(&line);
                    println!("[SIDE_STDERR] {}", line_str.trim());
                }
                CommandEvent::Terminated(payload) => {
                    println!("[SIDE_TERM] Exit code: {:?}", payload.code);
                    let _ = app.emit(
                        "bot-event",
                        serde_json::json!({
                            "type": "status",
                            "data": { "isRunning": false, "message": "Process Terminated" }
                        }),
                    );
                }
                _ => {}
            }
        }
    });

    Ok(())
}

#[tauri::command]
async fn stop_automation(state: State<'_, AppState>) -> Result<(), String> {
    let mut handle_lock = state.bot_handle.lock().await;
    if let Some(child) = handle_lock.take() {
        child.kill().map_err(|e| e.to_string())?;
    }
    Ok(())
}

#[tauri::command]
fn list_windows() -> Vec<String> {
    let mut windows: Vec<String> = Vec::new();

    unsafe {
        let _ = EnumWindows(
            Some(enum_window),
            LPARAM(&mut windows as *mut Vec<String> as isize),
        );
    }

    windows.sort();
    windows.dedup();
    windows
}

extern "system" fn enum_window(hwnd: HWND, lparam: LPARAM) -> BOOL {
    let windows = unsafe { &mut *(lparam.0 as *mut Vec<String>) };

    unsafe {
        if IsWindowVisible(hwnd).as_bool() {
            let length = GetWindowTextLengthW(hwnd);
            if length > 0 {
                let mut buffer = vec![0u16; (length + 1) as usize];
                GetWindowTextW(hwnd, &mut buffer);
                let title = String::from_utf16_lossy(&buffer[..length as usize]);
                if !title.is_empty() && title != "Program Manager" && title != "Settings" {
                    windows.push(title);
                }
            }
        }
    }
    BOOL::from(true)
}

#[tauri::command]
fn get_config() -> serde_json::Value {
    // Read from parent directory to avoid triggering cargo-watch rebuilds in src-tauri
    if let Ok(content) = std::fs::read_to_string("../pyauto_config.json") {
        if let Ok(json) = serde_json::from_str(&content) {
            return json;
        }
    }
    serde_json::json!({})
}

#[tauri::command]
fn update_config(new_config: serde_json::Value) -> Result<(), String> {
    let json_str = serde_json::to_string_pretty(&new_config).map_err(|e| e.to_string())?;
    std::fs::write("../pyauto_config.json", json_str).map_err(|e| e.to_string())?;
    Ok(())
}

#[tauri::command]
fn get_machine_id() -> String {
    license::get_hardware_id()
}

#[tauri::command]
fn verify_activation_key(key: String) -> bool {
    let hwid = license::get_hardware_id();
    license::verify_signature(&hwid, &key)
}

#[tauri::command]
fn generate_admin_keys() -> (String, String) {
    license::data_generate_admin_keys()
}

#[tauri::command]
async fn manual_ingest(
    app: tauri::AppHandle,
    file_name: String,
    file_data: Vec<u8>,
) -> Result<String, String> {
    // We need config to pass to logic
    let config_path = "../pyauto_config.json";
    let config_content = std::fs::read_to_string(config_path).unwrap_or_else(|_| "{}".to_string());
    let config: engine::AppConfig =
        serde_json::from_str(&config_content).unwrap_or_else(|_| engine::AppConfig {
            target_window: None,
            global_action_key: Some("e".to_string()),
            hold_duration: Some(1.2),
            rules: Some(vec![]),
            discord_webhook_url: None,
            notify_on_success: Some(true),
            notify_on_failure: Some(false),
            notify_on_error: Some(true),
            account_data: None,
            gas_url: None,
            api_secret: None,
        });

    // Run logic on thread pool to avoid blocking async runtime
    let app_clone = app.clone();
    let result = tauri::async_runtime::spawn_blocking(move || {
        engine::manual_ingest_logic(file_data, file_name, config, app_clone)
    })
    .await
    .map_err(|e| e.to_string())??;

    Ok(result)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .manage(AppState::default())
        .plugin(tauri_plugin_shell::init())
        .plugin(tauri_plugin_updater::Builder::new().build())
        .plugin(tauri_plugin_opener::init())
        .invoke_handler(tauri::generate_handler![
            start_automation,
            stop_automation,
            list_windows,
            get_config,
            update_config,
            start_rust_engine,
            stop_rust_engine,
            get_machine_id,
            verify_activation_key,
            generate_admin_keys,
            manual_ingest
        ])
        .run(tauri::generate_context!())
        .expect("error while running tauri application");
}
