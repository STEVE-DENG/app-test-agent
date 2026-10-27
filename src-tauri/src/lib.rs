pub mod agent;
pub mod device;
pub mod explorer;
pub mod knowledge;
pub mod learning;
pub mod models;
pub mod replay;
pub mod settings;
pub mod storage;

use std::collections::HashMap;
use std::path::PathBuf;
use std::sync::atomic::AtomicBool;
use std::sync::{Arc, Mutex, RwLock};

use base64::Engine;
use chrono::Utc;
use knowledge::KnowledgeStore;
use models::{
    AgentRunResult, AppKnowledgePlugin, AppPluginSummary, AppTestCase, ArtifactInfo, DebugSnapshot,
    DebugSnapshotInput, DeviceInfo, EfficiencyStats, LearningLoopInput, Observation, RunRecord,
    SaveSettingsInput, SettingsView, SkillRecord, StartExplorationInput, StartTaskInput,
    StoredSettings, ToolRecord,
};
use storage::AppStore;
use tauri::{Manager, State};
use uuid::Uuid;

struct AppState {
    store: Arc<AppStore>,
    knowledge: Arc<KnowledgeStore>,
    artifacts_dir: PathBuf,
    settings: Arc<RwLock<StoredSettings>>,
    cancellations: Arc<Mutex<HashMap<String, Arc<AtomicBool>>>>,
}

pub async fn run_headless_exploration(
    data_dir: PathBuf,
    device_serial: String,
    package: String,
    max_steps: u32,
) -> Result<models::ExplorationReport, String> {
    let artifacts_dir = data_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir)
        .map_err(|error| format!("创建证据目录失败: {error}"))?;
    let knowledge = Arc::new(KnowledgeStore::new(&data_dir)?);
    let run_id = format!("headless-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    explorer::run_exploration(
        explorer::ExplorerDeps {
            app: None,
            knowledge,
            artifacts_dir,
            cancellation: Arc::new(AtomicBool::new(false)),
        },
        run_id,
        device_serial,
        package,
        max_steps,
    )
    .await
}

pub async fn run_headless_test(
    data_dir: PathBuf,
    device_serial: String,
    package: String,
    test_id: String,
) -> Result<models::TestCaseRunResult, String> {
    let artifacts_dir = data_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir)
        .map_err(|error| format!("创建证据目录失败: {error}"))?;
    let knowledge = Arc::new(KnowledgeStore::new(&data_dir)?);
    replay::run_test_case(
        replay::ReplayDeps {
            app: None,
            knowledge,
            artifacts_dir,
        },
        format!("headless-test-{}", Utc::now().format("%Y%m%d-%H%M%S")),
        device_serial,
        package,
        test_id,
    )
    .await
}

pub async fn run_headless_task(
    data_dir: PathBuf,
    device_serial: String,
    instruction: String,
    allow_high_risk: bool,
) -> Result<models::AgentRunResult, String> {
    let artifacts_dir = data_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir)
        .map_err(|error| format!("创建证据目录失败: {error}"))?;
    let store = Arc::new(AppStore::new(data_dir.clone())?);
    let knowledge = Arc::new(KnowledgeStore::new(&data_dir)?);
    let settings = settings::load_settings(&data_dir)?;
    let run_id = format!("headless-task-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    agent::run_agent(
        agent::AgentDeps {
            app: None,
            store,
            knowledge,
            settings,
            artifacts_dir,
            cancellation: Arc::new(AtomicBool::new(false)),
        },
        run_id,
        instruction,
        device_serial,
        allow_high_risk,
    )
    .await
}

pub async fn run_headless_learning(
    data_dir: PathBuf,
    device_serial: String,
    package: String,
    max_cycles: u32,
    max_steps_per_cycle: u32,
    stop_after_stagnant_cycles: u32,
) -> Result<models::LearningLoopReport, String> {
    let artifacts_dir = data_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir)
        .map_err(|error| format!("创建证据目录失败: {error}"))?;
    let knowledge = Arc::new(KnowledgeStore::new(&data_dir)?);
    learning::run_learning_loop(
        learning::LearningDeps {
            app: None,
            knowledge,
            artifacts_dir,
            cancellation: Arc::new(AtomicBool::new(false)),
        },
        format!("headless-learning-{}", Utc::now().format("%Y%m%d-%H%M%S")),
        device_serial,
        package,
        max_cycles,
        max_steps_per_cycle,
        stop_after_stagnant_cycles,
    )
    .await
}

pub async fn run_headless_debug(
    data_dir: PathBuf,
    device_serial: String,
    package: Option<String>,
) -> Result<DebugSnapshot, String> {
    let artifacts_dir = data_dir.join("artifacts");
    std::fs::create_dir_all(&artifacts_dir)
        .map_err(|error| format!("创建证据目录失败: {error}"))?;
    let store = AppStore::new(data_dir.clone())?;
    let knowledge = KnowledgeStore::new(&data_dir)?;
    device::ensure_device_ready(&device_serial).await?;
    let label = format!("headless-debug-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    let observation =
        device::capture_observation(&device_serial, &artifacts_dir, "_debug", &label).await?;
    knowledge.register_observation(&observation)?;
    let raw_ui_xml = std::fs::read_to_string(&observation.ui_path).unwrap_or_default();
    let package = package
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| observation.package.clone());
    let mut efficiency = store.efficiency_stats()?;
    if !package.trim().is_empty() {
        let plugin = knowledge.get_plugin(&package)?;
        efficiency.pages = plugin.pages.len();
        efficiency.transitions = plugin.transitions.len();
        efficiency.test_cases = plugin.test_cases.len();
    }
    efficiency.artifacts = collect_artifact_infos(&artifacts_dir, 500).len();
    Ok(DebugSnapshot {
        device_serial: observation.device_serial.clone(),
        package: observation.package.clone(),
        activity: observation.activity.clone(),
        page_id: observation.page_id.clone(),
        page_signature: observation.page_signature.clone(),
        captured_at: observation.captured_at.clone(),
        screenshot_path: observation.screenshot_path.clone(),
        ui_path: observation.ui_path.clone(),
        screenshot_data_url: observation.screenshot_data_url.clone(),
        raw_ui_xml,
        nodes: observation.nodes.clone(),
        node_count: observation.nodes.len(),
        visible_node_count: observation.nodes.iter().filter(|node| node.visible).count(),
        actionable_node_count: observation
            .nodes
            .iter()
            .filter(|node| node.is_actionable())
            .count(),
        scrollable_node_count: observation
            .nodes
            .iter()
            .filter(|node| node.scrollable && node.visible)
            .count(),
        artifacts_dir: artifacts_dir.to_string_lossy().to_string(),
        data_dir: store.data_dir.to_string_lossy().to_string(),
        artifacts: collect_artifact_infos(&artifacts_dir, 80),
        efficiency,
    })
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
    let observation =
        device::capture_observation(&device_serial, &state.artifacts_dir, "_preview", &label)
            .await?;
    state.knowledge.register_observation(&observation)?;
    Ok(observation)
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
        app: Some(app),
        store: state.store.clone(),
        knowledge: state.knowledge.clone(),
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

#[tauri::command]
fn list_app_plugins(state: State<'_, AppState>) -> Result<Vec<AppPluginSummary>, String> {
    state.knowledge.list_plugins()
}

#[tauri::command]
fn get_app_plugin(
    state: State<'_, AppState>,
    package: String,
) -> Result<AppKnowledgePlugin, String> {
    state.knowledge.get_plugin(&package)
}

#[tauri::command]
fn export_app_plugin(state: State<'_, AppState>, package: String) -> Result<String, String> {
    state.knowledge.export_plugin(&package)
}

#[tauri::command]
fn import_app_plugin(state: State<'_, AppState>, json: String) -> Result<AppPluginSummary, String> {
    state.knowledge.import_plugin(&json)
}

#[tauri::command]
fn generate_app_tests(
    state: State<'_, AppState>,
    package: String,
) -> Result<Vec<AppTestCase>, String> {
    state.knowledge.generate_test_cases(&package)
}

#[tauri::command]
fn read_artifact(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let requested = PathBuf::from(&path)
        .canonicalize()
        .map_err(|error| format!("读取证据文件失败: {error}"))?;
    let artifacts_root = state
        .artifacts_dir
        .canonicalize()
        .map_err(|error| format!("读取证据目录失败: {error}"))?;
    if !requested.starts_with(&artifacts_root) {
        return Err("不允许读取 App 数据目录之外的证据文件".to_string());
    }
    let bytes = std::fs::read(&requested).map_err(|error| format!("读取证据文件失败: {error}"))?;
    Ok(format!(
        "data:image/png;base64,{}",
        base64::engine::general_purpose::STANDARD.encode(bytes)
    ))
}

#[tauri::command]
fn read_text_artifact(state: State<'_, AppState>, path: String) -> Result<String, String> {
    let requested = PathBuf::from(&path)
        .canonicalize()
        .map_err(|error| format!("读取证据文件失败: {error}"))?;
    let artifacts_root = state
        .artifacts_dir
        .canonicalize()
        .map_err(|error| format!("读取证据目录失败: {error}"))?;
    if !requested.starts_with(&artifacts_root) {
        return Err("不允许读取 App 数据目录之外的证据文件".to_string());
    }
    let metadata =
        std::fs::metadata(&requested).map_err(|error| format!("读取证据信息失败: {error}"))?;
    if metadata.len() > 4 * 1024 * 1024 {
        return Err("证据文件超过 4 MB，请在文件中直接查看".to_string());
    }
    std::fs::read_to_string(&requested).map_err(|error| format!("读取文本证据失败: {error}"))
}

#[tauri::command]
fn get_efficiency_stats(
    state: State<'_, AppState>,
    package: Option<String>,
) -> Result<EfficiencyStats, String> {
    let mut stats = state.store.efficiency_stats()?;
    if let Some(package) = package.filter(|value| !value.trim().is_empty()) {
        let plugin = state.knowledge.get_plugin(&package)?;
        stats.pages = plugin.pages.len();
        stats.transitions = plugin.transitions.len();
        stats.test_cases = plugin.test_cases.len();
    }
    stats.artifacts = collect_artifact_infos(&state.artifacts_dir, 500).len();
    Ok(stats)
}

#[tauri::command]
async fn get_debug_snapshot(
    state: State<'_, AppState>,
    input: DebugSnapshotInput,
) -> Result<DebugSnapshot, String> {
    build_debug_snapshot(state.inner(), input).await
}

async fn build_debug_snapshot(
    state: &AppState,
    input: DebugSnapshotInput,
) -> Result<DebugSnapshot, String> {
    device::ensure_device_ready(&input.device_serial).await?;
    let label = format!("debug-{}", Utc::now().format("%Y%m%d-%H%M%S"));
    let observation =
        device::capture_observation(&input.device_serial, &state.artifacts_dir, "_debug", &label)
            .await?;
    state.knowledge.register_observation(&observation)?;
    let raw_ui_xml = std::fs::read_to_string(&observation.ui_path).unwrap_or_default();
    let package = input
        .package
        .filter(|value| !value.trim().is_empty())
        .unwrap_or_else(|| observation.package.clone());
    let mut efficiency = state.store.efficiency_stats()?;
    if !package.trim().is_empty() {
        let plugin = state.knowledge.get_plugin(&package)?;
        efficiency.pages = plugin.pages.len();
        efficiency.transitions = plugin.transitions.len();
        efficiency.test_cases = plugin.test_cases.len();
    }
    efficiency.artifacts = collect_artifact_infos(&state.artifacts_dir, 500).len();
    let artifacts = collect_artifact_infos(&state.artifacts_dir, 80);
    Ok(DebugSnapshot {
        device_serial: observation.device_serial.clone(),
        package: observation.package.clone(),
        activity: observation.activity.clone(),
        page_id: observation.page_id.clone(),
        page_signature: observation.page_signature.clone(),
        captured_at: observation.captured_at.clone(),
        screenshot_path: observation.screenshot_path.clone(),
        ui_path: observation.ui_path.clone(),
        screenshot_data_url: observation.screenshot_data_url.clone(),
        raw_ui_xml,
        nodes: observation.nodes.clone(),
        node_count: observation.nodes.len(),
        visible_node_count: observation.nodes.iter().filter(|node| node.visible).count(),
        actionable_node_count: observation
            .nodes
            .iter()
            .filter(|node| node.is_actionable())
            .count(),
        scrollable_node_count: observation
            .nodes
            .iter()
            .filter(|node| node.scrollable && node.visible)
            .count(),
        artifacts_dir: state.artifacts_dir.to_string_lossy().to_string(),
        data_dir: state.store.data_dir.to_string_lossy().to_string(),
        artifacts,
        efficiency,
    })
}

#[tauri::command]
async fn export_debug_bundle(
    state: State<'_, AppState>,
    input: DebugSnapshotInput,
) -> Result<String, String> {
    let snapshot = build_debug_snapshot(state.inner(), input).await?;
    let package = if snapshot.package.trim().is_empty() {
        None
    } else {
        Some(snapshot.package.clone())
    };
    let plugin = package
        .as_deref()
        .map(|package| state.knowledge.get_plugin(package))
        .transpose()?;
    let runs = state.store.list_runs(20)?;
    serde_json::to_string_pretty(&serde_json::json!({
        "generatedAt": Utc::now().to_rfc3339(),
        "app": "AI App Test Agent",
        "snapshot": snapshot,
        "plugin": plugin,
        "recentRuns": runs
    }))
    .map_err(|error| format!("生成调试包失败: {error}"))
}

fn collect_artifact_infos(root: &std::path::Path, limit: usize) -> Vec<ArtifactInfo> {
    let mut stack = vec![root.to_path_buf()];
    let mut files = Vec::new();
    while let Some(directory) = stack.pop() {
        let Ok(entries) = std::fs::read_dir(directory) else {
            continue;
        };
        for entry in entries.flatten() {
            let path = entry.path();
            if path.is_dir() {
                stack.push(path);
                continue;
            }
            let Ok(metadata) = entry.metadata() else {
                continue;
            };
            let extension = path
                .extension()
                .and_then(|value| value.to_str())
                .unwrap_or_default()
                .to_lowercase();
            let kind = match extension.as_str() {
                "png" | "jpg" | "jpeg" | "webp" => "image",
                "xml" => "ui-tree",
                "json" => "json",
                "db" | "sqlite" => "database",
                _ => "file",
            };
            let modified_at = metadata
                .modified()
                .ok()
                .map(chrono::DateTime::<Utc>::from)
                .map(|value| value.to_rfc3339())
                .unwrap_or_default();
            files.push(ArtifactInfo {
                path: path.to_string_lossy().to_string(),
                name: path
                    .file_name()
                    .and_then(|value| value.to_str())
                    .unwrap_or_default()
                    .to_string(),
                kind: kind.to_string(),
                size: metadata.len(),
                modified_at,
            });
        }
    }
    files.sort_by(|left, right| right.modified_at.cmp(&left.modified_at));
    files.truncate(limit);
    files
}

#[tauri::command]
async fn start_exploration(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: StartExplorationInput,
) -> Result<String, String> {
    let package = input.package.trim().to_string();
    if package.is_empty() {
        return Err("请先确定当前 App 包名".to_string());
    }
    device::ensure_device_ready(&input.device_serial).await?;

    let run_id = Uuid::new_v4().to_string();
    let cancellation = Arc::new(AtomicBool::new(false));
    state
        .cancellations
        .lock()
        .map_err(|_| "任务取消状态锁已损坏".to_string())?
        .insert(run_id.clone(), cancellation.clone());

    let deps = explorer::ExplorerDeps {
        app: Some(app),
        knowledge: state.knowledge.clone(),
        artifacts_dir: state.artifacts_dir.clone(),
        cancellation,
    };
    let cancellations = state.cancellations.clone();
    let background_run_id = run_id.clone();
    let cleanup_run_id = run_id.clone();
    tauri::async_runtime::spawn(async move {
        let _ = explorer::run_exploration(
            deps,
            background_run_id,
            input.device_serial,
            package,
            input.max_steps,
        )
        .await;
        if let Ok(mut active) = cancellations.lock() {
            active.remove(&cleanup_run_id);
        }
    });

    Ok(run_id)
}

#[tauri::command]
async fn start_learning_loop(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    input: LearningLoopInput,
) -> Result<String, String> {
    let package = input.package.trim().to_string();
    if package.is_empty() {
        return Err("请先确定当前 App 包名".to_string());
    }
    device::ensure_device_ready(&input.device_serial).await?;

    let run_id = Uuid::new_v4().to_string();
    let cancellation = Arc::new(AtomicBool::new(false));
    state
        .cancellations
        .lock()
        .map_err(|_| "任务取消状态锁已损坏".to_string())?
        .insert(run_id.clone(), cancellation.clone());
    let deps = learning::LearningDeps {
        app: Some(app),
        knowledge: state.knowledge.clone(),
        artifacts_dir: state.artifacts_dir.clone(),
        cancellation,
    };
    let cancellations = state.cancellations.clone();
    let background_run_id = run_id.clone();
    let cleanup_run_id = run_id.clone();
    tauri::async_runtime::spawn(async move {
        let _ = learning::run_learning_loop(
            deps,
            background_run_id,
            input.device_serial,
            package,
            input.max_cycles,
            input.max_steps_per_cycle,
            input.stop_after_stagnant_cycles,
        )
        .await;
        if let Ok(mut active) = cancellations.lock() {
            active.remove(&cleanup_run_id);
        }
    });
    Ok(run_id)
}

#[tauri::command]
async fn start_test_replay(
    app: tauri::AppHandle,
    state: State<'_, AppState>,
    device_serial: String,
    package: String,
    test_id: String,
) -> Result<String, String> {
    if package.trim().is_empty() || test_id.trim().is_empty() {
        return Err("缺少 App 包名或测试用例 ID".to_string());
    }
    device::ensure_device_ready(&device_serial).await?;
    let run_id = Uuid::new_v4().to_string();
    let deps = replay::ReplayDeps {
        app: Some(app),
        knowledge: state.knowledge.clone(),
        artifacts_dir: state.artifacts_dir.clone(),
    };
    let background_run_id = run_id.clone();
    tauri::async_runtime::spawn(async move {
        let _ =
            replay::run_test_case(deps, background_run_id, device_serial, package, test_id).await;
    });
    Ok(run_id)
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
            let knowledge =
                Arc::new(KnowledgeStore::new(&data_dir).map_err(std::io::Error::other)?);
            let model_settings =
                settings::load_settings(&data_dir).map_err(std::io::Error::other)?;
            app.manage(AppState {
                store,
                knowledge,
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
            set_skill_status,
            list_app_plugins,
            get_app_plugin,
            export_app_plugin,
            import_app_plugin,
            generate_app_tests,
            read_artifact,
            read_text_artifact,
            get_efficiency_stats,
            get_debug_snapshot,
            export_debug_bundle,
            start_exploration,
            start_learning_loop,
            start_test_replay
        ])
        .run(tauri::generate_context!())
        .expect("error while running AI App Test Agent");
}
