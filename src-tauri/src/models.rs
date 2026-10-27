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

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DeviceDiagnostics {
    pub serial: String,
    pub adb_version: String,
    pub model: String,
    pub manufacturer: String,
    pub android_version: String,
    pub sdk: String,
    pub resolution: String,
    pub density: String,
    pub screen_state: String,
    pub battery_level: Option<i32>,
    pub battery_status: String,
    pub current_activity: String,
    pub input_method: String,
    pub boot_completed: String,
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
    pub selected: bool,
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
    pub page_id: String,
    pub page_signature: String,
    pub captured_at: String,
    pub width: i32,
    pub height: i32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgeElement {
    pub key: String,
    pub index: usize,
    pub text: String,
    pub content_description: String,
    pub resource_id: String,
    pub class_name: String,
    pub bounds: Bounds,
    pub clickable: bool,
    pub scrollable: bool,
    pub focusable: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct KnowledgePage {
    pub id: String,
    pub name: String,
    pub signature: String,
    pub package: String,
    pub activity: String,
    pub screenshot_path: String,
    pub ui_path: String,
    pub width: i32,
    pub height: i32,
    pub first_seen_at: String,
    pub last_seen_at: String,
    pub visit_count: u32,
    pub elements: Vec<KnowledgeElement>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct PageTransition {
    pub id: String,
    pub from_page_id: String,
    pub to_page_id: String,
    pub trigger_key: String,
    pub trigger_label: String,
    pub tool: String,
    pub args: Value,
    pub result_summary: String,
    pub success: bool,
    pub observed_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ActionTrace {
    pub id: String,
    pub run_id: String,
    pub step_index: u32,
    pub from_page_id: String,
    pub to_page_id: String,
    pub tool: String,
    pub args: Value,
    pub result: Value,
    pub success: bool,
    pub screenshot_before: String,
    pub screenshot_after: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCaseStep {
    pub from_page_id: String,
    pub expected_page_id: String,
    pub element_key: String,
    pub label: String,
    pub action: String,
    pub args: Value,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppTestCase {
    pub id: String,
    pub name: String,
    pub package: String,
    pub start_page_id: String,
    pub target_page_id: String,
    pub steps: Vec<TestCaseStep>,
    pub status: String,
    pub last_result: String,
    pub created_at: String,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppKnowledgePlugin {
    pub schema_version: u32,
    pub package: String,
    pub name: String,
    pub created_at: String,
    pub updated_at: String,
    pub pages: Vec<KnowledgePage>,
    pub transitions: Vec<PageTransition>,
    pub actions: Vec<ActionTrace>,
    pub test_cases: Vec<AppTestCase>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct AppPluginSummary {
    pub package: String,
    pub name: String,
    pub pages: usize,
    pub transitions: usize,
    pub actions: usize,
    pub test_cases: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ArtifactInfo {
    pub path: String,
    pub name: String,
    pub kind: String,
    pub size: u64,
    pub modified_at: String,
}

#[derive(Debug, Clone, Default, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct EfficiencyStats {
    pub total_runs: u32,
    pub completed_runs: u32,
    pub failed_runs: u32,
    pub total_steps: u32,
    pub avg_steps: f64,
    pub total_model_ms: u64,
    pub total_tool_ms: u64,
    pub total_observation_ms: u64,
    pub total_knowledge_ms: u64,
    pub total_step_ms: u64,
    pub avg_step_ms: f64,
    pub pending_skills: u32,
    pub approved_skills: u32,
    pub pages: usize,
    pub transitions: usize,
    pub test_cases: usize,
    pub artifacts: usize,
    pub updated_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugSnapshot {
    pub device_serial: String,
    pub package: String,
    pub activity: String,
    pub page_id: String,
    pub page_signature: String,
    pub captured_at: String,
    pub screenshot_path: String,
    pub ui_path: String,
    pub screenshot_data_url: String,
    pub raw_ui_xml: String,
    pub nodes: Vec<UiNode>,
    pub node_count: usize,
    pub visible_node_count: usize,
    pub actionable_node_count: usize,
    pub scrollable_node_count: usize,
    pub artifacts_dir: String,
    pub data_dir: String,
    pub diagnostics: DeviceDiagnostics,
    pub artifacts: Vec<ArtifactInfo>,
    pub efficiency: EfficiencyStats,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct DebugSnapshotInput {
    pub device_serial: String,
    #[serde(default)]
    pub package: Option<String>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningRoundReport {
    pub cycle: u32,
    pub steps: u32,
    pub pages_before: usize,
    pub pages_after: usize,
    pub transitions_before: usize,
    pub transitions_after: usize,
    pub new_pages: usize,
    pub new_transitions: usize,
    pub elapsed_ms: u64,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningLoopReport {
    pub package: String,
    pub status: String,
    pub cycles_completed: u32,
    pub total_steps: u32,
    pub pages: usize,
    pub transitions: usize,
    pub test_cases: usize,
    pub rounds: Vec<LearningRoundReport>,
    pub started_at: String,
    pub finished_at: String,
    pub summary: String,
}

#[derive(Debug, Clone, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct LearningLoopInput {
    pub device_serial: String,
    pub package: String,
    pub max_cycles: u32,
    pub max_steps_per_cycle: u32,
    pub stop_after_stagnant_cycles: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ExplorationReport {
    pub package: String,
    pub status: String,
    pub steps: u32,
    pub pages_discovered: usize,
    pub transitions_discovered: usize,
    pub summary: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StartExplorationInput {
    pub device_serial: String,
    pub package: String,
    pub max_steps: u32,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct TestCaseRunResult {
    pub test_id: String,
    pub package: String,
    pub status: String,
    pub passed_steps: u32,
    pub total_steps: u32,
    pub message: String,
    pub finished_at: String,
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
    #[serde(default)]
    pub screenshot_after_path: String,
    #[serde(default)]
    pub page_id_before: String,
    #[serde(default)]
    pub page_id_after: String,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct StepMetricRecord {
    pub run_id: String,
    pub step_index: u32,
    pub model_ms: u64,
    pub tool_ms: u64,
    pub observation_ms: u64,
    pub knowledge_ms: u64,
    pub total_ms: u64,
    pub created_at: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(rename_all = "camelCase")]
pub struct ModelTraceRecord {
    pub run_id: String,
    pub step_index: u32,
    pub model: String,
    pub prompt_text: String,
    pub endpoint: String,
    pub prompt_chars: usize,
    pub system_chars: usize,
    pub node_count: usize,
    pub use_vision: bool,
    pub attempt_count: u32,
    pub raw_response: String,
    pub usage: Value,
    pub duration_ms: u64,
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
