mod agent;
mod device;
mod models;
mod settings;
mod storage;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use chrono::Utc;
use models::{
    AgentRunResult, DeviceInfo, Observation, RunRecord, SaveSettingsInput, SettingsView,
    SkillRecord, StartTaskInput, StoredSettings, ToolRecord,
};
use storage::AppStore;
use tauri::{Manager, State};
use uuid::Uuid;

struct AppState {
    store: Arc<AppStore>,
    artifacts_dir: PathBuf,
    settings: Arc<RwLock<StoredSettings>>,
    cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

#[tauri::command]
async fn list_devices() -> Result<Vec<DeviceInfo>, String> {
    device::list_devices().await
}

#[tauri::command]
async fn capture_observation(
    state: State<'_, AppState>,
    device_serial: String,
) -> Result<Observation, String> {
    let label = format!("preview-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    device::capture_observation(&device_serial, &state.artifacts_dir, "_preview", &label).await
}

#[tauri::command]
async fn list_packages(device_serial: String) -> Result<Vec<String>, String> {
    device::list_packages(&device_serial).await
}

#[tauri::command]
async fn open_mirror(device_serial: String) -> Result<String, String> {
    device::launch_scrcpy(&device_serial).await
}

#[tauri::command]
fn get_settings(state: State<'_, AppState>) -> Result<SettingsView, String> {
    let settings = state
        .settings
        .read()
        .map_err(|_| "模型设置锁已损坏".to_string())?;
    Ok(settings::settings_view(&settings))
}

#[tauri::command]
fn save_app_settings(
    state: State<'_, AppState>,
    input: SaveSettingsInput,
) -> Result<SettingsView, String> {
    let mut current = state
        .settings
        .write()
        .map_err(|_| "模型设置锁已损坏".to_string())?;
    let updated = settings::save_settings(&state.store.data_dir, &current, input)?;
    *current = updated.clone();
    Ok(settings::settings_view(&updated))
}

#[tauri::command]
async fn test_model_connection(
    state: State<'_, AppState>,
    input: SaveSettingsInput,
) -> Result<String, String> {
    let current = state
        .settings
        .read()
        .map_err(|_| "模型设置锁已损坏".to_string())?
        .clone();
    let draft = settings::prepare_settings(&current, input)?;
    agent::test_model_connection(&draft).await
}

#[tauri::command]
async fn start_task(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: StartTaskInput,
) -> Result<String, String> {
    let instruction = input.instruction.trim().to_string();
    if instruction.is_empty() {
        return Err("任务描述不能为空".to_string());
    }
    let current_settings = state
        .settings
        .read()
        .map_err(|_| "模型设置锁已损坏".to_string())?
        .clone();
    if current_settings.api_key.trim().is_empty() {
        return Err("请先在设置中配置多模态模型 API Key".to_string());
    }
    if !input.allow_high_risk && agent::contains_high_risk(&instruction) {
        return Err("任务包含高风险动作，请确认允许高风险操作".to_string());
    }
    device::ensure_device_ready(&input.device_serial).await?;

    let run_id = Uuid::new_v4().to_string();
    let cancellation = Arc::new(AtomicBool::new(false));
    state
        .cancellations
        .lock()
        .map_err(|_| "任务取消状态锁已损坏".to_string())?
        .insert(run_id.clone(), cancellation.clone());

    let deps = agent::AgentDeps {
        app,
        store: state.store.clone(),
        settings: current_settings,
        artifacts_dir: state.artifacts_dir.clone(),
        cancellation,
    };
    let cancellations = state.cancellations.clone();
    let background_run_id = run_id.clone();
    let cleanup_run_id = run_id.clone();
    tauri::async_runtime::spawn(async move {
        let _: Result<AgentRunResult, String> = agent::run_agent(
            deps,
            background_run_id,
            instruction,
            input.device_serial,
            input.allow_high_risk,
        )
        .await;
        if let Ok(mut active) = cancellations.lock() {
            active.remove(&cleanup_run_id);
        }
    });

    Ok(run_id)
}

#[tauri::command]
fn cancel_task(state: State<'_, AppState>, run_id: String) -> Result<bool, String> {
    let active = state
        .cancellations
        .lock()
        .map_err(|_| "任务取消状态锁已损坏".to_string())?;
    if let Some(flag) = active.get(&run_id) {
        flag.store(true, std::sync::atomic::Ordering::Relaxed);
        Ok(true)
    } else {
        Ok(false)
    }
}

#[tauri::command]
fn list_runs(state: State<'_, AppState>, limit: Option<u32>) -> Result<Vec<RunRecord>, String> {
    state.store.list_runs(limit.unwrap_or(30))
}

#[tauri::command]
fn list_run_steps(state: State<'_, AppState>, run_id: String) -> Result<Vec<ToolRecord>, String> {
    state.store.list_run_steps(&run_id)
}

#[tauri::command]
fn list_skills(state: State<'_, AppState>) -> Result<Vec<SkillRecord>, String> {
    state.store.list_skills()
}

#[tauri::command]
fn set_skill_status(
    state: State<'_, AppState>,
    skill_id: String,
    status: String,
) -> Result<(), String> {
    state.store.set_skill_status(&skill_id, &status)
}

#[cfg_attr(mobile, tauri::mobile_entry_point)]
pub fn run() {
    tauri::Builder::default()
        .plugin(tauri_plugin_opener::init())
        .setup(|app| {
            let data_dir = app
                .path()
                .app_data_dir()
                .map_err(|error| std::io::Error::other(error.to_string()))?;
            let artifacts_dir = data_dir.join("artifacts");
            std::fs::create_dir_all(&artifacts_dir)?;
            let store = Arc::new(AppStore::new(data_dir.clone()).map_err(std::io::Error::other)?);
            let model_settings =
                settings::load_settings(&data_dir).map_err(std::io::Error::other)?;
            app.manage(AppState {
                store,
                artifacts_dir,
                settings: Arc::new(RwLock::new(model_settings)),
                cancellations: Arc::new(Mutex::new(HashMap::new())),
            });
            Ok(())
        })
        .invoke_handler(tauri::generate_handler![
            list_devices,
            capture_observation,
            list_packages,
            open_mirror,
            get_settings,
            save_app_settings,
            test_model_connection,
            start_task,
            cancel_task,
            list_runs,
            list_run_steps,
            list_skills,
            set_skill_status
        ])
        .run(tauri::generate_context!())
        .expect("error while running AI App Test Agent");
}
