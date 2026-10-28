import { listen, type UnlistenFn } from "@tauri-apps/api/event";
import {
  Activity,
  Cable,
  Command,
  MonitorUp,
  RefreshCw,
  Settings2,
  ShieldCheck,
  Sparkles,
  X,
} from "lucide-react";
import { useCallback, useEffect, useMemo, useState } from "react";
import "./App.css";
import { AgentPanel } from "./components/AgentPanel";
import { BottomDock, type DockTab } from "./components/BottomDock";
import { DeviceRail } from "./components/DeviceRail";
import { DeviceStage } from "./components/DeviceStage";
import { SettingsDialog } from "./components/SettingsDialog";
import { api, errorMessage, isTauriRuntime } from "./lib/api";
import type {
  AgentEvent,
  AppKnowledgePlugin,
  AppPluginSummary,
  ConversationMessage,
  ConversationSummary,
  DebugSnapshot,
  DeviceInfo,
  EfficiencyStats,
  LearningLoopReport,
  ModelTraceRecord,
  Observation,
  RunRecord,
  SaveSettingsInput,
  SettingsView,
  SkillRecord,
  StepMetricRecord,
  ToolRecord,
  UiNode,
} from "./types";

type Notice = {
  tone: "info" | "error" | "success";
  message: string;
};

function App() {
  const [devices, setDevices] = useState<DeviceInfo[]>([]);
  const [selectedSerial, setSelectedSerial] = useState<string | null>(null);
  const [observation, setObservation] = useState<Observation | null>(null);
  const [selectedNode, setSelectedNode] = useState<UiNode | null>(null);
  const [settings, setSettings] = useState<SettingsView | null>(null);
  const [appPlugins, setAppPlugins] = useState<AppPluginSummary[]>([]);
  const [appPlugin, setAppPlugin] = useState<AppKnowledgePlugin | null>(null);
  const [selectedPackage, setSelectedPackage] = useState("");
  const [selectedPageId, setSelectedPageId] = useState<string | null>(null);
  const [exploring, setExploring] = useState(false);
  const [explorationRunId, setExplorationRunId] = useState<string | null>(null);
  const [knowledgeError, setKnowledgeError] = useState("");
  const [skills, setSkills] = useState<SkillRecord[]>([]);
  const [runs, setRuns] = useState<RunRecord[]>([]);
  const [conversations, setConversations] = useState<ConversationSummary[]>([]);
  const [conversationId, setConversationId] = useState<string | null>(null);
  const [conversationMessages, setConversationMessages] = useState<
    ConversationMessage[]
  >([]);
  const [events, setEvents] = useState<AgentEvent[]>([]);
  const [runId, setRunId] = useState<string | null>(null);
  const [runStatus, setRunStatus] = useState("idle");
  const [running, setRunning] = useState(false);
  const [currentThought, setCurrentThought] = useState("");
  const [currentAction, setCurrentAction] = useState("");
  const [currentStep, setCurrentStep] = useState(0);
  const [recalledSkills, setRecalledSkills] = useState<SkillRecord[]>([]);
  const [instruction, setInstruction] = useState("");
  const [allowHighRisk, setAllowHighRisk] = useState(false);
  const [activeTab, setActiveTab] = useState<DockTab>("timeline");
  const [selectedRunId, setSelectedRunId] = useState<string | null>(null);
  const [runSteps, setRunSteps] = useState<ToolRecord[]>([]);
  const [runMetrics, setRunMetrics] = useState<StepMetricRecord[]>([]);
  const [runModelTraces, setRunModelTraces] = useState<ModelTraceRecord[]>([]);
  const [loadingDevices, setLoadingDevices] = useState(false);
  const [loadingObservation, setLoadingObservation] = useState(false);
  const [stageError, setStageError] = useState("");
  const [taskError, setTaskError] = useState("");
  const [settingsOpen, setSettingsOpen] = useState(false);
  const [settingsSaving, setSettingsSaving] = useState(false);
  const [settingsTesting, setSettingsTesting] = useState(false);
  const [settingsTestResult, setSettingsTestResult] = useState("");
  const [settingsError, setSettingsError] = useState("");
  const [notice, setNotice] = useState<Notice | null>(null);
  const [debugSnapshot, setDebugSnapshot] = useState<DebugSnapshot | null>(null);
  const [efficiency, setEfficiency] = useState<EfficiencyStats | null>(null);
  const [debugLoading, setDebugLoading] = useState(false);
  const [debugError, setDebugError] = useState("");
  const [learning, setLearning] = useState(false);
  const [learningRunId, setLearningRunId] = useState<string | null>(null);
  const [learningReport, setLearningReport] =
    useState<LearningLoopReport | null>(null);

  const selectedDevice = useMemo(
    () => devices.find((device) => device.serial === selectedSerial) || null,
    [devices, selectedSerial],
  );

  const refreshSkills = useCallback(async () => {
    try {
      setSkills(await api.listSkills());
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  }, []);

  const refreshRuns = useCallback(async () => {
    try {
      setRuns(await api.listRuns());
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  }, []);

  const refreshConversations = useCallback(async (preferredId?: string) => {
    try {
      const next = await api.listConversations();
      setConversations(next);
      const targetId =
        preferredId || next.find((item) => item.messageCount > 0)?.id || null;
      if (targetId) {
        setConversationId(targetId);
        setConversationMessages(await api.getConversationMessages(targetId));
      }
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  }, []);

  const refreshEfficiency = useCallback(
    async (packageName?: string) => {
      try {
        setEfficiency(
          await api.getEfficiencyStats(packageName || selectedPackage || undefined),
        );
      } catch (error) {
        setDebugError(errorMessage(error));
      }
    },
    [selectedPackage],
  );

  const refreshAppPlugins = useCallback(async (preferredPackage?: string) => {
    try {
      const summaries = await api.listAppPlugins();
      setAppPlugins(summaries);
      const targetPackage = preferredPackage || summaries[0]?.package || "";
      setSelectedPackage((current) => preferredPackage || current || targetPackage);
      if (!targetPackage) {
        setAppPlugin(null);
        setSelectedPageId(null);
        return;
      }
      const plugin = await api.getAppPlugin(targetPackage);
      setAppPlugin(plugin);
      setSelectedPageId((current) =>
        plugin.pages.some((page) => page.id === current)
          ? current
          : plugin.pages.find((page) => page.activity.endsWith("MainActivity"))
              ?.id ||
            plugin.pages[0]?.id ||
            null,
      );
      setKnowledgeError("");
    } catch (error) {
      setKnowledgeError(errorMessage(error));
    }
  }, []);

  const refreshDevices = useCallback(async () => {
    setLoadingDevices(true);
    try {
      const nextDevices = await api.listDevices();
      setDevices(nextDevices);
      setSelectedSerial((current) => {
        if (current && nextDevices.some((device) => device.serial === current)) {
          return current;
        }
        return (
          nextDevices.find((device) => device.ready)?.serial ||
          nextDevices[0]?.serial ||
          null
        );
      });
      setStageError("");
    } catch (error) {
      setStageError(errorMessage(error));
    } finally {
      setLoadingDevices(false);
    }
  }, []);

  useEffect(() => {
    void refreshDevices();
    void refreshSkills();
    void refreshRuns();
    void refreshConversations();
    void refreshAppPlugins();
    void refreshEfficiency();
    api
      .getSettings()
      .then(setSettings)
      .catch((error) => setNotice({ tone: "error", message: errorMessage(error) }));
  }, [
    refreshAppPlugins,
    refreshConversations,
    refreshDevices,
    refreshEfficiency,
    refreshRuns,
    refreshSkills,
  ]);

  useEffect(() => {
    if (!isTauriRuntime) return;

    let disposed = false;
    let unlisten: UnlistenFn | undefined;

    listen<AgentEvent>("agent-event", (payload) => {
      const event = payload.payload;
      setEvents((current) => [...current, event].slice(-240));
      setRunId(event.runId);
      setCurrentStep(event.stepIndex);
      const data = event.data as
        | {
            observation?: Observation;
            skills?: SkillRecord[];
            action?: string;
            package?: string;
            pageId?: string;
            conversationId?: string;
          }
        | null
        | undefined;

      if (data?.observation) {
        setObservation(data.observation);
        setSelectedPageId(data.observation.pageId);
        setSelectedNode(null);
      }
      if (data?.skills) {
        setRecalledSkills(data.skills);
      }
      if (data?.action) {
        setCurrentAction(String(data.action));
      }
      if (event.eventType === "thought") {
        setCurrentThought(event.detail);
      }
      if (event.eventType === "run_started") {
        setRunning(true);
        setRunStatus("running");
      }
      if (event.eventType === "exploration_started") {
        setExploring(true);
        setExplorationRunId(event.runId);
      }
      if (event.eventType === "learning_started") {
        setLearning(true);
        setLearningRunId(event.runId);
        setLearningReport(null);
      }
      if (event.eventType === "learning_round_completed") {
        setLearning(true);
      }
      if (event.eventType === "learning_failed") {
        setLearning(false);
        setLearningRunId(null);
        setNotice({ tone: "error", message: event.detail });
      }
      if (
        ["exploration_completed", "exploration_cancelled"].includes(
          event.eventType,
        )
      ) {
        setExploring(false);
        setExplorationRunId(null);
        setNotice({ tone: "success", message: event.detail });
      }
      if (["learning_completed", "learning_cancelled"].includes(event.eventType)) {
        setLearning(false);
        setLearningRunId(null);
        const report = (event.data as { report?: LearningLoopReport } | null)?.report;
        if (report) setLearningReport(report);
        setNotice({
          tone: event.eventType === "learning_completed" ? "success" : "info",
          message: event.detail,
        });
        void refreshAppPlugins(data?.package || selectedPackage || undefined);
        void refreshEfficiency(data?.package || selectedPackage || undefined);
      }
      if (event.eventType.startsWith("test_")) {
        void refreshAppPlugins(
          data?.package ||
            event.data?.package?.toString() ||
            selectedPackage ||
            undefined,
        );
      }
      if (
        ["test_completed", "test_failed"].includes(event.eventType)
      ) {
        setNotice({
          tone: event.eventType === "test_completed" ? "success" : "info",
          message: event.detail,
        });
      }
      if (
        data?.observation ||
        event.eventType.startsWith("exploration_") ||
        event.eventType.startsWith("knowledge_")
      ) {
        void refreshAppPlugins(
          data?.package ||
            data?.observation?.package ||
            event.data?.package?.toString(),
        );
      }
      if (
        [
          "run_completed",
          "run_failed",
          "run_cancelled",
          "needs_input",
          "conversation_updated",
        ].includes(event.eventType)
      ) {
        const eventConversationId = data?.conversationId;
        const targetConversationId =
          typeof eventConversationId === "string"
            ? eventConversationId
            : conversationId;
        if (targetConversationId) {
          void api
            .getConversationMessages(targetConversationId)
            .then(setConversationMessages)
            .catch((error) =>
              setNotice({ tone: "error", message: errorMessage(error) }),
            );
        }
        void refreshConversations(targetConversationId || undefined);
      }
      if (
        ["run_completed", "run_failed", "run_cancelled", "needs_input"].includes(
          event.eventType,
        )
      ) {
        setRunning(false);
        setCurrentAction("");
        setRunStatus(
          event.eventType === "run_completed"
            ? "completed"
            : event.eventType === "run_cancelled"
              ? "cancelled"
              : event.eventType === "needs_input"
                ? "needs_input"
                : "failed",
        );
        setNotice({
          tone: event.eventType === "run_completed" ? "success" : "info",
          message: event.detail,
        });
        void refreshSkills();
        void refreshRuns();
        void refreshEfficiency(selectedPackage || undefined);
      }
    })
      .then((cleanup) => {
        if (disposed) cleanup();
        else unlisten = cleanup;
      })
      .catch((error) =>
        setNotice({ tone: "error", message: errorMessage(error) }),
      );

    return () => {
      disposed = true;
      unlisten?.();
    };
  }, [
    refreshAppPlugins,
    refreshConversations,
    refreshEfficiency,
    refreshRuns,
    refreshSkills,
    conversationId,
    selectedPackage,
  ]);

  useEffect(() => {
    if (!notice) return;
    const timer = window.setTimeout(() => setNotice(null), 4800);
    return () => window.clearTimeout(timer);
  }, [notice]);

  const handleSelectDevice = (serial: string) => {
    setSelectedSerial(serial);
    setObservation(null);
    setSelectedNode(null);
    setStageError("");
  };

  const handleCapture = async () => {
    if (!selectedDevice?.ready) return;
    setLoadingObservation(true);
    setStageError("");
    try {
      const nextObservation = await api.captureObservation(selectedDevice.serial);
      setObservation(nextObservation);
      setSelectedNode(null);
      setSelectedPageId(nextObservation.pageId);
      await refreshAppPlugins(nextObservation.package);
      void refreshEfficiency(nextObservation.package);
      setNotice({
        tone: "success",
        message: `已采集 ${nextObservation.nodes.length} 个 UI 节点`,
      });
    } catch (error) {
      setStageError(errorMessage(error));
    } finally {
      setLoadingObservation(false);
    }
  };

  const handleOpenMirror = async () => {
    if (!selectedDevice?.ready) return;
    try {
      setNotice({
        tone: "info",
        message: await api.openMirror(selectedDevice.serial),
      });
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  };

  const handleStart = async () => {
    if (!selectedDevice) {
      setTaskError("请先选择一台 Android 设备");
      return;
    }
    if (!instruction.trim()) {
      setTaskError("请先输入一句话任务");
      return;
    }
    setTaskError("");
    setStageError("");
    setNotice(null);
    setRecalledSkills([]);
    setCurrentThought("");
    setCurrentAction("planning");
    setCurrentStep(0);
    setRunning(true);
    setRunStatus("running");
    setActiveTab("timeline");

    try {
      const started = await api.startTask({
        instruction,
        deviceSerial: selectedDevice.serial,
        allowHighRisk,
        conversationId,
      });
      setRunId(started.runId);
      setConversationId(started.conversationId);
      setInstruction("");
      setConversationMessages((current) => [
        ...current,
        {
          id: `local-user-${Date.now()}`,
          conversationId: started.conversationId,
          runId: started.runId,
          role: "user",
          content: instruction,
          status: "sent",
          createdAt: new Date().toISOString(),
        },
      ]);
    } catch (error) {
      setRunning(false);
      setRunStatus("failed");
      setTaskError(errorMessage(error));
    }
  };

  const handleNewConversation = () => {
    setConversationId(null);
    setConversationMessages([]);
    setEvents([]);
    setRunId(null);
    setRunStatus("idle");
    setCurrentThought("");
    setCurrentAction("");
    setCurrentStep(0);
    setTaskError("");
  };

  const handleSelectConversation = async (id: string) => {
    setConversationId(id);
    setEvents([]);
    setRunStatus("idle");
    setCurrentAction("");
    setCurrentThought("");
    setTaskError("");
    try {
      setConversationMessages(await api.getConversationMessages(id));
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  };

  const handleArchiveConversation = async () => {
    if (!conversationId || running) return;
    try {
      await api.archiveConversation(conversationId);
      setConversations((current) =>
        current.filter((conversation) => conversation.id !== conversationId),
      );
      setConversationId(null);
      setConversationMessages([]);
      setEvents([]);
      setRunStatus("idle");
      setNotice({ tone: "success", message: "会话已归档" });
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  };

  const handleCancel = async () => {
    if (!runId) return;
    try {
      const cancelled = await api.cancelTask(runId);
      if (cancelled) {
        setCurrentThought("正在安全停止 Agent...");
      }
    } catch (error) {
      setTaskError(errorMessage(error));
    }
  };

  const handleSaveSettings = async (input: SaveSettingsInput) => {
    setSettingsSaving(true);
    setSettingsError("");
    try {
      const nextSettings = await api.saveSettings(input);
      setSettings(nextSettings);
      setSettingsOpen(false);
      setNotice({ tone: "success", message: "模型配置已保存" });
    } catch (error) {
      setSettingsError(errorMessage(error));
    } finally {
      setSettingsSaving(false);
    }
  };

  const handleTestSettings = async (input: SaveSettingsInput) => {
    setSettingsTesting(true);
    setSettingsError("");
    setSettingsTestResult("");
    try {
      setSettingsTestResult(await api.testModelConnection(input));
    } catch (error) {
      setSettingsError(errorMessage(error));
    } finally {
      setSettingsTesting(false);
    }
  };

  const handleSkillStatus = async (skillId: string, status: string) => {
    try {
      await api.setSkillStatus(skillId, status);
      await refreshSkills();
      setNotice({
        tone: "success",
        message:
          status === "approved" ? "Skill 已批准，可用于后续召回" : "Skill 已拒绝",
      });
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  };

  const handleSelectRun = async (id: string) => {
    setSelectedRunId(id);
    try {
      const [steps, metrics, modelTraces] = await Promise.all([
        api.listRunSteps(id),
        api.listStepMetrics(id),
        api.listModelTraces(id),
      ]);
      setRunSteps(steps);
      setRunMetrics(metrics);
      setRunModelTraces(modelTraces);
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
    }
  };

  const handleSelectPackage = async (packageName: string) => {
    setSelectedPackage(packageName);
    setSelectedPageId(null);
    setAppPlugin(null);
    setKnowledgeError("");
    if (!packageName) return;
    try {
      const plugin = await api.getAppPlugin(packageName);
      setAppPlugin(plugin);
      setSelectedPageId(
        plugin.pages.find((page) => page.activity.endsWith("MainActivity"))
          ?.id ||
          plugin.pages[0]?.id ||
          null,
      );
      void refreshEfficiency(packageName);
    } catch (error) {
      setKnowledgeError(errorMessage(error));
    }
  };

  const handleStartExploration = async (maxSteps: number) => {
    const packageName = observation?.package || selectedPackage;
    if (!selectedDevice?.ready) {
      setKnowledgeError("请先连接并选择一台 Android 设备");
      return;
    }
    if (!packageName) {
      setKnowledgeError("请先采集当前 App 页面，确定包名");
      return;
    }
    setKnowledgeError("");
    setExploring(true);
    setActiveTab("knowledge");
    try {
      const nextRunId = await api.startExploration({
        deviceSerial: selectedDevice.serial,
        package: packageName,
        maxSteps,
      });
      setExplorationRunId(nextRunId);
      setNotice({
        tone: "info",
        message: `自动遍历已启动，最多 ${maxSteps} 步`,
      });
    } catch (error) {
      setExploring(false);
      setKnowledgeError(errorMessage(error));
    }
  };

  const handleStopExploration = async () => {
    if (!explorationRunId) return;
    try {
      await api.cancelTask(explorationRunId);
      setNotice({ tone: "info", message: "正在停止自动遍历..." });
    } catch (error) {
      setKnowledgeError(errorMessage(error));
    }
  };

  const handleDebugCapture = async () => {
    if (!selectedDevice?.ready) {
      setDebugError("请先连接并选择一台 Android 设备");
      return;
    }
    setDebugLoading(true);
    setDebugError("");
    try {
      const snapshot = await api.getDebugSnapshot({
        deviceSerial: selectedDevice.serial,
        package: selectedPackage || observation?.package || undefined,
      });
      setDebugSnapshot(snapshot);
      setEfficiency(snapshot.efficiency);
      setNotice({
        tone: "success",
        message: `调试快照已采集：${snapshot.nodeCount} 个节点，${snapshot.actionableNodeCount} 个可操作节点`,
      });
    } catch (error) {
      setDebugError(errorMessage(error));
    } finally {
      setDebugLoading(false);
    }
  };

  const handleStartLearning = async (input: {
    maxCycles: number;
    maxStepsPerCycle: number;
    stopAfterStagnantCycles: number;
  }) => {
    if (!selectedDevice?.ready) {
      setDebugError("请先连接并选择一台 Android 设备");
      return;
    }
    const packageName = selectedPackage || observation?.package;
    if (!packageName) {
      setDebugError("请先采集当前 App 页面，确定包名");
      return;
    }
    setDebugError("");
    setLearning(true);
    setLearningReport(null);
    setActiveTab("debug");
    try {
      const nextRunId = await api.startLearningLoop({
        deviceSerial: selectedDevice.serial,
        package: packageName,
        ...input,
      });
      setLearningRunId(nextRunId);
      setNotice({
        tone: "info",
        message: `持续学习已启动，最多 ${input.maxCycles} 轮`,
      });
    } catch (error) {
      setLearning(false);
      setDebugError(errorMessage(error));
    }
  };

  const handleStopLearning = async () => {
    if (!learningRunId) return;
    try {
      await api.cancelTask(learningRunId);
      setNotice({ tone: "info", message: "正在停止持续学习..." });
    } catch (error) {
      setDebugError(errorMessage(error));
    }
  };

  const handleExportDebug = async () => {
    if (!selectedDevice?.ready) return;
    try {
      const json = await api.exportDebugBundle({
        deviceSerial: selectedDevice.serial,
        package: selectedPackage || observation?.package || undefined,
      });
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `debug-bundle-${Date.now()}.json`;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
      setNotice({ tone: "success", message: "调试包已导出" });
    } catch (error) {
      setDebugError(errorMessage(error));
    }
  };

  const handleGenerateTests = async () => {
    if (!selectedPackage) return;
    try {
      const generated = await api.generateAppTests(selectedPackage);
      await refreshAppPlugins(selectedPackage);
      setNotice({
        tone: "success",
        message: `已生成 ${generated.length} 条页面导航用例`,
      });
    } catch (error) {
      setKnowledgeError(errorMessage(error));
    }
  };

  const handleCompactPlugin = async () => {
    if (!selectedPackage) return;
    try {
      const summary = await api.compactAppPlugin(selectedPackage);
      await refreshAppPlugins(selectedPackage);
      setNotice({
        tone: "success",
        message: `页面关系已压缩：${summary.pages} 个页面，${summary.transitions} 条跳转`,
      });
    } catch (error) {
      setKnowledgeError(errorMessage(error));
    }
  };

  const handleRunTest = async (testId: string) => {
    if (!selectedDevice?.ready) {
      setKnowledgeError("请先连接并选择一台 Android 设备");
      return;
    }
    if (!selectedPackage) {
      setKnowledgeError("请先选择 App 插件");
      return;
    }
    try {
      await api.startTestReplay({
        deviceSerial: selectedDevice.serial,
        package: selectedPackage,
        testId,
      });
      setNotice({ tone: "info", message: "测试用例开始回放" });
    } catch (error) {
      setKnowledgeError(errorMessage(error));
    }
  };

  const handleExportPlugin = async () => {
    if (!selectedPackage) return;
    try {
      const json = await api.exportAppPlugin(selectedPackage);
      const blob = new Blob([json], { type: "application/json" });
      const url = URL.createObjectURL(blob);
      const anchor = document.createElement("a");
      anchor.href = url;
      anchor.download = `${selectedPackage}.app-plugin.json`;
      document.body.appendChild(anchor);
      anchor.click();
      anchor.remove();
      URL.revokeObjectURL(url);
      setNotice({ tone: "success", message: "App 插件 JSON 已导出" });
    } catch (error) {
      setKnowledgeError(errorMessage(error));
    }
  };

  const handleImportPlugin = async (file: File) => {
    try {
      const summary = await api.importAppPlugin(await file.text());
      await refreshAppPlugins(summary.package);
      setSelectedPageId(null);
      setActiveTab("knowledge");
      setNotice({
        tone: "success",
        message: `已导入 ${summary.name}：${summary.pages} 页面，${summary.testCases} 用例`,
      });
    } catch (error) {
      setKnowledgeError(errorMessage(error));
    }
  };

  useEffect(() => {
    const handleKey = (event: KeyboardEvent) => {
      if ((event.metaKey || event.ctrlKey) && event.key === "Enter" && !running) {
        event.preventDefault();
        void handleStart();
      }
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  });

  return (
    <div className="app-frame">
      <div className="ambient-grid" aria-hidden="true" />
      <div className="ambient-glow glow-a" aria-hidden="true" />
      <div className="ambient-glow glow-b" aria-hidden="true" />

      <header className="topbar" data-tauri-drag-region>
        <div className="brand-block" data-tauri-drag-region>
          <span className="brand-mark">
            <span />
            <Command size={18} strokeWidth={2.1} />
          </span>
          <div>
            <strong>APP PILOT</strong>
            <span>Android Agent Lab</span>
          </div>
        </div>

        <div className="signal-path" data-tauri-drag-region>
          <span>
            <Cable size={13} />
            ADB
          </span>
          <i />
          <span>
            <Activity size={13} />
            HYBRID SENSE
          </span>
          <i />
          <span>
            <Sparkles size={13} />
            REACT
          </span>
          <i />
          <span>
            <ShieldCheck size={13} />
            SKILL
          </span>
        </div>

        <div className="top-actions">
          <span
            className={`connection-pill ${selectedDevice?.ready ? "online" : ""} ${
              isTauriRuntime ? "" : "preview"
            }`}
          >
            <i />
            {!isTauriRuntime
              ? "BROWSER PREVIEW"
              : selectedDevice?.ready
                ? "DEVICE ONLINE"
                : "NO DEVICE"}
          </span>
          <button
            type="button"
            className="icon-button"
            onClick={handleOpenMirror}
            disabled={!selectedDevice?.ready}
            title="打开 scrcpy"
            aria-label="打开 scrcpy"
          >
            <MonitorUp size={16} />
          </button>
          <button
            type="button"
            className="icon-button"
            onClick={() => void refreshDevices()}
            disabled={loadingDevices}
            title="刷新设备"
            aria-label="刷新设备"
          >
            <RefreshCw size={16} className={loadingDevices ? "spin" : ""} />
          </button>
          <button
            type="button"
            className="icon-button"
            onClick={() => {
              setSettingsError("");
              setSettingsTestResult("");
              setSettingsOpen(true);
            }}
            title="设置"
            aria-label="设置"
          >
            <Settings2 size={16} />
          </button>
        </div>
      </header>

      <main className="workspace">
        <DeviceRail
          devices={devices}
          selectedSerial={selectedSerial}
          loading={loadingDevices}
          onSelect={handleSelectDevice}
          onRefresh={() => void refreshDevices()}
          onOpenMirror={() => void handleOpenMirror()}
          onOpenSettings={() => {
            setSettingsError("");
            setSettingsTestResult("");
            setSettingsOpen(true);
          }}
        />
        <DeviceStage
          device={selectedDevice}
          observation={observation}
          selectedNode={selectedNode}
          loading={loadingObservation}
          error={stageError}
          onCapture={() => void handleCapture()}
          onOpenMirror={() => void handleOpenMirror()}
          onSelectNode={setSelectedNode}
        />
        <AgentPanel
          settings={settings}
          selectedDevice={selectedDevice}
          conversations={conversations}
          conversationId={conversationId}
          messages={conversationMessages}
          instruction={instruction}
          onInstructionChange={setInstruction}
          allowHighRisk={allowHighRisk}
          onAllowHighRiskChange={setAllowHighRisk}
          running={running}
          runStatus={runStatus}
          currentThought={currentThought}
          currentAction={currentAction}
          currentStep={currentStep}
          runId={runId}
          recalledSkills={recalledSkills}
          events={events}
          error={taskError}
          onStart={() => void handleStart()}
          onCancel={() => void handleCancel()}
          onNewConversation={handleNewConversation}
          onSelectConversation={(id) => void handleSelectConversation(id)}
          onArchiveConversation={() => void handleArchiveConversation()}
          onOpenSettings={() => {
            setSettingsError("");
            setSettingsTestResult("");
            setSettingsOpen(true);
          }}
        />
      </main>

      <BottomDock
        activeTab={activeTab}
        onTabChange={setActiveTab}
        events={events}
        appPlugins={appPlugins}
        appPlugin={appPlugin}
        selectedPackage={selectedPackage}
        selectedDevice={selectedDevice}
        selectedPageId={selectedPageId}
        exploring={exploring}
        knowledgeError={knowledgeError}
        skills={skills}
        runs={runs}
        runSteps={runSteps}
        runMetrics={runMetrics}
        runModelTraces={runModelTraces}
        selectedRunId={selectedRunId}
        observation={observation}
        selectedNode={selectedNode}
        debugSnapshot={debugSnapshot}
        efficiency={efficiency}
        debugLoading={debugLoading}
        debugError={debugError}
        learning={learning}
        learningReport={learningReport}
        onDebugCapture={() => void handleDebugCapture()}
        onRefreshStats={() =>
          void refreshEfficiency(selectedPackage || observation?.package)
        }
        onStartLearning={(input) => void handleStartLearning(input)}
        onStopLearning={() => void handleStopLearning()}
        onExportDebug={() => void handleExportDebug()}
        onSelectNode={setSelectedNode}
        onSelectRun={(id) => void handleSelectRun(id)}
        onOpenDebug={() => setActiveTab("debug")}
        onOpenPage={(pageId) => {
          setSelectedPageId(pageId);
          setActiveTab("knowledge");
        }}
        onSkillStatus={(id, status) => void handleSkillStatus(id, status)}
        onSelectPackage={(packageName) => void handleSelectPackage(packageName)}
        onSelectPage={setSelectedPageId}
        onRefreshKnowledge={() =>
          void refreshAppPlugins(selectedPackage || observation?.package)
        }
        onStartExploration={(maxSteps) => void handleStartExploration(maxSteps)}
        onStopExploration={() => void handleStopExploration()}
        onGenerateTests={() => void handleGenerateTests()}
        onCompactPlugin={() => void handleCompactPlugin()}
        onRunTest={(testId) => void handleRunTest(testId)}
        onExportPlugin={() => void handleExportPlugin()}
        onImportPlugin={(file) => void handleImportPlugin(file)}
      />

      <SettingsDialog
        open={settingsOpen}
        settings={settings}
        saving={settingsSaving}
        testing={settingsTesting}
        testResult={settingsTestResult}
        error={settingsError}
        onClose={() => {
          setSettingsOpen(false);
          setSettingsTestResult("");
        }}
        onSave={(input) => void handleSaveSettings(input)}
        onTest={(input) => void handleTestSettings(input)}
      />

      {notice ? (
        <div className={`toast ${notice.tone}`}>
          <span>{notice.message}</span>
          <button type="button" onClick={() => setNotice(null)} aria-label="关闭提示">
            <X size={14} />
          </button>
        </div>
      ) : null}
    </div>
  );
}

export default App;
