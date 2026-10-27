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
  selected: boolean;
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
  pageId: string;
  pageSignature: string;
  capturedAt: string;
  width: number;
  height: number;
};

export type KnowledgeElement = {
  key: string;
  index: number;
  text: string;
  contentDescription: string;
  resourceId: string;
  className: string;
  bounds: Bounds;
  clickable: boolean;
  scrollable: boolean;
  focusable: boolean;
};

export type KnowledgePage = {
  id: string;
  name: string;
  signature: string;
  package: string;
  activity: string;
  screenshotPath: string;
  uiPath: string;
  width: number;
  height: number;
  firstSeenAt: string;
  lastSeenAt: string;
  visitCount: number;
  elements: KnowledgeElement[];
};

export type PageTransition = {
  id: string;
  fromPageId: string;
  toPageId: string;
  triggerKey: string;
  triggerLabel: string;
  tool: string;
  args: Record<string, unknown>;
  resultSummary: string;
  success: boolean;
  observedAt: string;
};

export type ActionTrace = {
  id: string;
  runId: string;
  stepIndex: number;
  fromPageId: string;
  toPageId: string;
  tool: string;
  args: Record<string, unknown>;
  result: Record<string, unknown>;
  success: boolean;
  screenshotBefore: string;
  screenshotAfter: string;
  createdAt: string;
};

export type TestCaseStep = {
  fromPageId: string;
  expectedPageId: string;
  elementKey: string;
  label: string;
  action: string;
  args: Record<string, unknown>;
};

export type AppTestCase = {
  id: string;
  name: string;
  package: string;
  startPageId: string;
  targetPageId: string;
  steps: TestCaseStep[];
  status: string;
  lastResult: string;
  createdAt: string;
  updatedAt: string;
};

export type AppKnowledgePlugin = {
  schemaVersion: number;
  package: string;
  name: string;
  createdAt: string;
  updatedAt: string;
  pages: KnowledgePage[];
  transitions: PageTransition[];
  actions: ActionTrace[];
  testCases: AppTestCase[];
};

export type AppPluginSummary = {
  package: string;
  name: string;
  pages: number;
  transitions: number;
  actions: number;
  testCases: number;
  updatedAt: string;
};

export type ArtifactInfo = {
  path: string;
  name: string;
  kind: string;
  size: number;
  modifiedAt: string;
};

export type EfficiencyStats = {
  totalRuns: number;
  completedRuns: number;
  failedRuns: number;
  totalSteps: number;
  avgSteps: number;
  totalModelMs: number;
  totalToolMs: number;
  totalObservationMs: number;
  totalKnowledgeMs: number;
  totalStepMs: number;
  avgStepMs: number;
  pendingSkills: number;
  approvedSkills: number;
  pages: number;
  transitions: number;
  testCases: number;
  artifacts: number;
  updatedAt: string;
};

export type DebugSnapshot = {
  deviceSerial: string;
  package: string;
  activity: string;
  pageId: string;
  pageSignature: string;
  capturedAt: string;
  screenshotPath: string;
  uiPath: string;
  screenshotDataUrl: string;
  rawUiXml: string;
  nodes: UiNode[];
  nodeCount: number;
  visibleNodeCount: number;
  actionableNodeCount: number;
  scrollableNodeCount: number;
  artifactsDir: string;
  dataDir: string;
  artifacts: ArtifactInfo[];
  efficiency: EfficiencyStats;
};

export type LearningRoundReport = {
  cycle: number;
  steps: number;
  pagesBefore: number;
  pagesAfter: number;
  transitionsBefore: number;
  transitionsAfter: number;
  newPages: number;
  newTransitions: number;
  elapsedMs: number;
  summary: string;
};

export type LearningLoopReport = {
  package: string;
  status: string;
  cyclesCompleted: number;
  totalSteps: number;
  pages: number;
  transitions: number;
  testCases: number;
  rounds: LearningRoundReport[];
  startedAt: string;
  finishedAt: string;
  summary: string;
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
