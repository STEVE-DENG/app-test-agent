import { invoke } from "@tauri-apps/api/core";
import type {
  DeviceInfo,
  Observation,
  RunRecord,
  SaveSettingsInput,
  SettingsView,
  SkillRecord,
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
  listSkills: () => invoke<SkillRecord[]>("list_skills"),
  setSkillStatus: (skillId: string, status: string) =>
    invoke<void>("set_skill_status", { skillId, status }),
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
  listSkills: () => Promise.resolve([]),
  setSkillStatus: () => previewOnly("审核 Skill"),
};

export const api = isTauriRuntime ? tauriApi : browserPreviewApi;

export function errorMessage(error: unknown): string {
  if (typeof error === "string") return error;
  if (error instanceof Error) return error.message;
  return "发生未知错误";
}
