use serde::{Deserialize, Serialize};
use serde_json::Value;

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceInfo {
    pub serial: String,
    pub state: String,
    pub model: Option<String>,
    pub manufacturer: Option<String>,
    pub android_version: Option<String>,
    pub sdk: Option<String>,
    pub resolution: Option<String>,
    pub foreground_app: Option<String>,
    pub ready: bool,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct Bounds {
    pub left: i32,
    pub top: i32,
    pub right: i32,
    pub bottom: i32,
}

impl Bounds {
    pub fn center(&self) -> (i32, i32) {
        ((self.left + self.right) / 2, (self.top + self.bottom) / 2)
    }

    pub fn width(&self) -> i32 {
        (self.right - self.left).max(0)
    }

    pub fn height(&self) -> i32 {
        (self.bottom - self.top).max(0)
    }

    pub fn area(&self) -> i32 {
        self.width() * self.height()
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct UiNode {
    pub index: usize,
    pub text: String,
    pub content_description: String,
    pub resource_id: String,
    pub class_name: String,
    pub package: String,
    pub bounds: Bounds,
    pub clickable: bool,
    pub enabled: bool,
    pub focusable: bool,
    pub scrollable: bool,
    pub visible: bool,
    pub depth: usize,
}

impl UiNode {
    pub fn is_actionable(&self) -> bool {
        self.visible && self.enabled && self.bounds.width() > 1 && self.bounds.height() > 1
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct Observation {
    pub device_serial: String,
    pub screenshot_data_url: String,
    pub screenshot_path: String,
    pub ui_path: String,
    pub nodes: Vec<UiNode>,
    pub package: String,
    pub activity: String,
    pub page_signature: String,
    pub captured_at: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize, Default)]
#[serde(rename_all = "camelCase")]
pub struct StoredSettings {
    pub base_url: String,
    pub api_key: String,
    pub model: String,
    pub use_vision: bool,
    pub temperature: f32,
    pub max_steps: u32,
}

impl StoredSettings {
    pub fn normalized(mut self) -> Self {
        self.base_url = self.base_url.trim().trim_end_matches('/').to_string();
        self.model = self.model.trim().to_string();
        self.max_steps = self.max_steps.clamp(1, 60);
        self.temperature = self.temperature.clamp(0.0, 1.5);
        self
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SettingsView {
    pub base_url: String,
    pub model: String,
    pub use_vision: bool,
    pub temperature: f32,
    pub max_steps: u32,
    pub has_api_key: bool,
    pub api_key_preview: String,
}

impl From<&StoredSettings> for SettingsView {
    fn from(value: &StoredSettings) -> Self {
        let key_chars = value.api_key.chars().collect::<Vec<_>>();
        let preview = if key_chars.len() > 8 {
            format!(
                "{}...{}",
                key_chars[..4].iter().collect::<String>(),
                key_chars[key_chars.len() - 4..].iter().collect::<String>()
            )
        } else if value.api_key.is_empty() {
            String::new()
        } else {
            "••••••••".to_string()
        };
        Self {
            base_url: value.base_url.clone(),
            model: value.model.clone(),
            use_vision: value.use_vision,
            temperature: value.temperature,
            max_steps: value.max_steps,
            has_api_key: !value.api_key.is_empty(),
            api_key_preview: preview,
        }
    }
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SaveSettingsInput {
    pub base_url: String,
    pub api_key: Option<String>,
    pub model: String,
    pub use_vision: bool,
    pub temperature: f32,
    pub max_steps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentEvent {
    pub run_id: String,
    pub event_type: String,
    pub title: String,
    pub detail: String,
    pub step_index: u32,
    pub timestamp: String,
    pub data: Option<Value>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ToolRecord {
    pub step_index: u32,
    pub thought: String,
    pub tool: String,
    pub args: Value,
    pub result: Value,
    pub page_signature_before: String,
    pub page_signature_after: String,
    pub screenshot_path: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AgentRunResult {
    pub run_id: String,
    pub status: String,
    pub summary: String,
    pub steps: u32,
    pub skill_candidate_id: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct RunRecord {
    pub id: String,
    pub instruction: String,
    pub device_serial: String,
    pub status: String,
    pub summary: String,
    pub model: String,
    pub started_at: String,
    pub finished_at: Option<String>,
    pub steps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillPlanStep {
    pub tool: String,
    pub args: Value,
    pub note: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillRecord {
    pub id: String,
    pub name: String,
    pub description: String,
    pub package: String,
    pub keywords: Vec<String>,
    pub plan: Vec<SkillPlanStep>,
    pub status: String,
    pub success_count: u32,
    pub failure_count: u32,
    pub reuse_count: u32,
    pub source_run_id: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct SkillHint {
    pub id: String,
    pub name: String,
    pub description: String,
    pub package: String,
    pub plan: Vec<SkillPlanStep>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartTaskInput {
    pub instruction: String,
    pub device_serial: String,
    pub allow_high_risk: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelDecision {
    pub thought: String,
    pub action: String,
    #[serde(default)]
    pub args: Value,
    #[serde(default)]
    pub summary: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ModelAction {
    pub name: String,
    #[serde(default)]
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RawModelDecision {
    #[serde(default)]
    pub thought: String,
    pub action: ModelAction,
    #[serde(default)]
    pub summary: Option<String>,
}

impl RawModelDecision {
    pub fn into_decision(self) -> ModelDecision {
        ModelDecision {
            thought: self.thought,
            action: self.action.name,
            args: self.action.args,
            summary: self.summary,
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn previews_multibyte_api_key_without_splitting_characters() {
        let settings = StoredSettings {
            api_key: "甲乙丙丁戊己庚辛壬癸".to_string(),
            ..StoredSettings::default()
        };
        let view = SettingsView::from(&settings);
        assert_eq!(view.api_key_preview, "甲乙丙丁...庚辛壬癸");
    }
}
