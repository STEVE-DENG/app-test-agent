import { invoke } from "@tauri-apps/api/core";
import type {
  AppKnowledgePlugin,
  AppPluginSummary,
  AppTestCase,
  DebugSnapshot,
  DeviceInfo,
  EfficiencyStats,
  ModelTraceRecord,
  Observation,
  RunRecord,
  SaveSettingsInput,
  SettingsView,
  SkillRecord,
  StepMetricRecord,
  ToolRecord,
} from "../types";

export const isTauriRuntime =
  typeof window !== "undefined" && "__TAURI_INTERNALS__" in window;

let previewSettings: SettingsView = {
  baseUrl: "https://api.openai.com/v1",
  model: "gpt-4.1",
  useVision: true,
  temperature: 0.2,
  maxSteps: 24,
  hasApiKey: false,
  apiKeyPreview: "",
};

function previewOnly(feature: string): Promise<never> {
  return Promise.reject(
    new Error(`${feature}需要在 Tauri 桌面窗口中使用，请运行 npm run tauri dev`),
  );
}

const tauriApi = {
  listDevices: () => invoke<DeviceInfo[]>("list_devices"),
  captureObservation: (deviceSerial: string) =>
    invoke<Observation>("capture_observation", { deviceSerial }),
  listPackages: (deviceSerial: string) =>
    invoke<string[]>("list_packages", { deviceSerial }),
  openMirror: (deviceSerial: string) =>
    invoke<string>("open_mirror", { deviceSerial }),
  getSettings: () => invoke<SettingsView>("get_settings"),
  saveSettings: (input: SaveSettingsInput) =>
    invoke<SettingsView>("save_app_settings", { input }),
  testModelConnection: (input: SaveSettingsInput) =>
    invoke<string>("test_model_connection", { input }),
  startTask: (input: {
    instruction: string;
    deviceSerial: string;
    allowHighRisk: boolean;
  }) => invoke<string>("start_task", { input }),
  cancelTask: (runId: string) =>
    invoke<boolean>("cancel_task", { runId }),
  listRuns: (limit = 30) => invoke<RunRecord[]>("list_runs", { limit }),
  listRunSteps: (runId: string) =>
    invoke<ToolRecord[]>("list_run_steps", { runId }),
  listStepMetrics: (runId: string) =>
    invoke<StepMetricRecord[]>("list_step_metrics", { runId }),
  listModelTraces: (runId: string) =>
    invoke<ModelTraceRecord[]>("list_model_traces", { runId }),
  listSkills: () => invoke<SkillRecord[]>("list_skills"),
  setSkillStatus: (skillId: string, status: string) =>
    invoke<void>("set_skill_status", { skillId, status }),
  listAppPlugins: () => invoke<AppPluginSummary[]>("list_app_plugins"),
  getAppPlugin: (packageName: string) =>
    invoke<AppKnowledgePlugin>("get_app_plugin", { package: packageName }),
  exportAppPlugin: (packageName: string) =>
    invoke<string>("export_app_plugin", { package: packageName }),
  importAppPlugin: (json: string) =>
    invoke<AppPluginSummary>("import_app_plugin", { json }),
  generateAppTests: (packageName: string) =>
    invoke<AppTestCase[]>("generate_app_tests", { package: packageName }),
  readArtifact: (path: string) =>
    invoke<string>("read_artifact", { path }),
  readTextArtifact: (path: string) =>
    invoke<string>("read_text_artifact", { path }),
  getEfficiencyStats: (packageName?: string) =>
    invoke<EfficiencyStats>("get_efficiency_stats", {
      package: packageName || null,
    }),
  getDebugSnapshot: (input: { deviceSerial: string; package?: string }) =>
    invoke<DebugSnapshot>("get_debug_snapshot", { input }),
  exportDebugBundle: (input: { deviceSerial: string; package?: string }) =>
    invoke<string>("export_debug_bundle", { input }),
  startExploration: (input: {
    deviceSerial: string;
    package: string;
    maxSteps: number;
  }) => invoke<string>("start_exploration", { input }),
  startTestReplay: (input: {
    deviceSerial: string;
    package: string;
    testId: string;
  }) => invoke<string>("start_test_replay", input),
  startLearningLoop: (input: {
    deviceSerial: string;
    package: string;
    maxCycles: number;
    maxStepsPerCycle: number;
    stopAfterStagnantCycles: number;
  }) => invoke<string>("start_learning_loop", { input }),
};

const browserPreviewApi: typeof tauriApi = {
  listDevices: () => Promise.resolve([]),
  captureObservation: () => previewOnly("采集设备画面"),
  listPackages: () => previewOnly("读取应用列表"),
  openMirror: () => previewOnly("打开设备镜像"),
  getSettings: () => Promise.resolve(previewSettings),
  saveSettings: (input) => {
    previewSettings = {
      ...previewSettings,
      baseUrl: input.baseUrl,
      model: input.model,
      useVision: input.useVision,
      temperature: input.temperature,
      maxSteps: input.maxSteps,
      hasApiKey: Boolean(input.apiKey) || previewSettings.hasApiKey,
      apiKeyPreview: input.apiKey
        ? `${input.apiKey.slice(0, 4)}...${input.apiKey.slice(-4)}`
        : previewSettings.apiKeyPreview,
    };
    return Promise.resolve(previewSettings);
  },
  testModelConnection: () =>
    Promise.resolve("浏览器预览模式：连接检查未发送真实请求"),
  startTask: () => previewOnly("执行真实任务"),
  cancelTask: () => Promise.resolve(false),
  listRuns: () => Promise.resolve([]),
  listRunSteps: () => Promise.resolve([]),
  listStepMetrics: () => Promise.resolve([]),
  listModelTraces: () => Promise.resolve([]),
  listSkills: () => Promise.resolve([]),
  setSkillStatus: () => previewOnly("审核 Skill"),
  listAppPlugins: () => Promise.resolve([]),
  getAppPlugin: () => previewOnly("读取 App 插件"),
  exportAppPlugin: () => previewOnly("导出 App 插件"),
  importAppPlugin: () => previewOnly("导入 App 插件"),
  generateAppTests: () => previewOnly("生成测试用例"),
  readArtifact: () => previewOnly("读取页面截图"),
  readTextArtifact: () => previewOnly("读取调试文本"),
  getEfficiencyStats: () =>
    Promise.resolve({
      totalRuns: 0,
      completedRuns: 0,
      failedRuns: 0,
      totalSteps: 0,
      avgSteps: 0,
      totalModelMs: 0,
      totalToolMs: 0,
      totalObservationMs: 0,
      totalKnowledgeMs: 0,
      totalStepMs: 0,
      avgStepMs: 0,
      pendingSkills: 0,
      approvedSkills: 0,
      pages: 0,
      transitions: 0,
      testCases: 0,
      artifacts: 0,
      updatedAt: new Date().toISOString(),
    }),
  getDebugSnapshot: () => previewOnly("采集调试快照"),
  exportDebugBundle: () => previewOnly("导出调试包"),
  startExploration: () => previewOnly("自动遍历 App"),
  startTestReplay: () => previewOnly("回放测试用例"),
  startLearningLoop: () => previewOnly("启动持续学习"),
};

export const api = isTauriRuntime ? tauriApi : browserPreviewApi;

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "发生未知错误";
}
