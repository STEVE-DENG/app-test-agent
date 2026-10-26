export type DeviceInfo = {
  serial: string;
  state: string;
  model: string | null;
  manufacturer: string | null;
  androidVersion: string | null;
  sdk: string | null;
  resolution: string | null;
  foregroundApp: string | null;
  ready: boolean;
};

export type Bounds = {
  left: number;
  top: number;
  right: number;
  bottom: number;
};

export type UiNode = {
  index: number;
  text: string;
  contentDescription: string;
  resourceId: string;
  className: string;
  package: string;
  bounds: Bounds;
  clickable: boolean;
  enabled: boolean;
  focusable: boolean;
  scrollable: boolean;
  visible: boolean;
  depth: number;
};

export type Observation = {
  deviceSerial: string;
  screenshotDataUrl: string;
  screenshotPath: string;
  uiPath: string;
  nodes: UiNode[];
  package: string;
  activity: string;
  pageSignature: string;
  capturedAt: string;
  width: number;
  height: number;
};

export type SettingsView = {
  baseUrl: string;
  model: string;
  useVision: boolean;
  temperature: number;
  maxSteps: number;
  hasApiKey: boolean;
  apiKeyPreview: string;
};

export type SaveSettingsInput = {
  baseUrl: string;
  apiKey?: string;
  model: string;
  useVision: boolean;
  temperature: number;
  maxSteps: number;
};

export type AgentEvent = {
  runId: string;
  eventType: string;
  title: string;
  detail: string;
  stepIndex: number;
  timestamp: string;
  data?: Record<string, unknown> | null;
};

export type RunRecord = {
  id: string;
  instruction: string;
  deviceSerial: string;
  status: string;
  summary: string;
  model: string;
  startedAt: string;
  finishedAt: string | null;
  steps: number;
};

export type SkillPlanStep = {
  tool: string;
  args: Record<string, unknown>;
  note: string;
};

export type SkillRecord = {
  id: string;
  name: string;
  description: string;
  package: string;
  keywords: string[];
  plan: SkillPlanStep[];
  status: "pending" | "approved" | "rejected" | "archived" | string;
  successCount: number;
  failureCount: number;
  reuseCount: number;
  sourceRunId: string;
  createdAt: string;
  updatedAt: string;
};

export type ToolRecord = {
  stepIndex: number;
  thought: string;
  tool: string;
  args: Record<string, unknown>;
  result: Record<string, unknown>;
  pageSignatureBefore: string;
  pageSignatureAfter: string;
  screenshotPath: string;
  createdAt: string;
};
