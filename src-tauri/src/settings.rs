use std::fs;
use std::path::{Path, PathBuf};

use crate::models::{SaveSettingsInput, SettingsView, StoredSettings};

const SETTINGS_FILE: &str = "settings.json";

pub fn settings_path(data_dir: &Path) -> PathBuf {
    data_dir.join(SETTINGS_FILE)
}

pub fn load_settings(data_dir: &Path) -> Result<StoredSettings, String> {
    let path = settings_path(data_dir);
    if !path.exists() {
        return Ok(default_settings());
    }

    let raw = fs::read_to_string(&path).map_err(|error| format!("读取模型设置失败: {error}"))?;
    let settings: StoredSettings =
        serde_json::from_str(&raw).map_err(|error| format!("解析模型设置失败: {error}"))?;
    Ok(settings.normalized())
}

pub fn save_settings(
    data_dir: &Path,
    existing: &StoredSettings,
    input: SaveSettingsInput,
) -> Result<StoredSettings, String> {
    let settings = prepare_settings(existing, input)?;

    fs::create_dir_all(data_dir).map_err(|error| format!("创建设置目录失败: {error}"))?;
    let path = settings_path(data_dir);
    let temporary = data_dir.join("settings.json.tmp");
    let serialized = serde_json::to_string_pretty(&settings)
        .map_err(|error| format!("序列化设置失败: {error}"))?;
    fs::write(&temporary, serialized).map_err(|error| format!("写入临时设置失败: {error}"))?;

    #[cfg(unix)]
    {
        use std::os::unix::fs::PermissionsExt;
        let _ = fs::set_permissions(&temporary, fs::Permissions::from_mode(0o600));
    }

    fs::rename(&temporary, &path).map_err(|error| format!("保存模型设置失败: {error}"))?;
    Ok(settings)
}

pub fn prepare_settings(
    existing: &StoredSettings,
    input: SaveSettingsInput,
) -> Result<StoredSettings, String> {
    if input.base_url.trim().is_empty() {
        return Err("Base URL 不能为空".to_string());
    }
    if input.model.trim().is_empty() {
        return Err("Model 不能为空".to_string());
    }

    let api_key = match input.api_key {
        Some(value) if !value.trim().is_empty() => value.trim().to_string(),
        _ => existing.api_key.clone(),
    };

    Ok(StoredSettings {
        base_url: input.base_url,
        api_key,
        model: input.model,
        use_vision: input.use_vision,
        temperature: input.temperature,
        max_steps: input.max_steps,
    }
    .normalized())
}

pub fn settings_view(settings: &StoredSettings) -> SettingsView {
    SettingsView::from(settings)
}

fn default_settings() -> StoredSettings {
    StoredSettings {
        base_url: "http://127.0.0.1:3425/v1".to_string(),
        api_key: "magpie".to_string(),
        model: "opencode-go/deepseek-v4.1-flash".to_string(),
        use_vision: true,
        temperature: 0.2,
        max_steps: 24,
    }
}
