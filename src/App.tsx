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
  DeviceInfo,
  Observation,
  RunRecord,
  SaveSettingsInput,
  SettingsView,
  SkillRecord,
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
  const [skills, setSkills] = useState<SkillRecord[]>([]);
  const [runs, setRuns] = useState<RunRecord[]>([]);
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
    api
      .getSettings()
      .then(setSettings)
      .catch((error) => setNotice({ tone: "error", message: errorMessage(error) }));
  }, [refreshDevices, refreshRuns, refreshSkills]);

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
          }
        | null
        | undefined;

      if (data?.observation) {
        setObservation(data.observation);
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
  }, [refreshRuns, refreshSkills]);

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
    setEvents([]);
    setRecalledSkills([]);
    setCurrentThought("");
    setCurrentAction("planning");
    setCurrentStep(0);
    setRunning(true);
    setRunStatus("running");
    setActiveTab("timeline");

    try {
      const nextRunId = await api.startTask({
        instruction,
        deviceSerial: selectedDevice.serial,
        allowHighRisk,
      });
      setRunId(nextRunId);
    } catch (error) {
      setRunning(false);
      setRunStatus("failed");
      setTaskError(errorMessage(error));
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
      setRunSteps(await api.listRunSteps(id));
    } catch (error) {
      setNotice({ tone: "error", message: errorMessage(error) });
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
        skills={skills}
        runs={runs}
        runSteps={runSteps}
        selectedRunId={selectedRunId}
        observation={observation}
        selectedNode={selectedNode}
        onSelectNode={setSelectedNode}
        onSelectRun={(id) => void handleSelectRun(id)}
        onSkillStatus={(id, status) => void handleSkillStatus(id, status)}
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
