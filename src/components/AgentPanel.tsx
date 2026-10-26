import {
  BrainCircuit,
  Check,
  CircleStop,
  Play,
  ShieldAlert,
  Sparkles,
  WandSparkles,
  Zap,
} from "lucide-react";
import type { AgentEvent, DeviceInfo, SettingsView, SkillRecord } from "../types";

type Props = {
  settings: SettingsView | null;
  selectedDevice: DeviceInfo | null;
  instruction: string;
  onInstructionChange: (value: string) => void;
  allowHighRisk: boolean;
  onAllowHighRiskChange: (value: boolean) => void;
  running: boolean;
  runStatus: string;
  currentThought: string;
  currentAction: string;
  currentStep: number;
  runId: string | null;
  recalledSkills: SkillRecord[];
  events: AgentEvent[];
  error: string;
  onStart: () => void;
  onCancel: () => void;
  onOpenSettings: () => void;
};

const suggestions = [
  "打开设置，查看 Android 版本并返回",
  "在应用内搜索机械键盘，记录前三个商品名称和价格",
  "帮我完成当前页面的表单，停在提交前",
];

function statusLabel(status: string, running: boolean) {
  if (running) return "执行中";
  if (status === "completed") return "已完成";
  if (status === "failed") return "失败";
  if (status === "cancelled") return "已停止";
  if (status === "needs_input") return "等待补充";
  return "就绪";
}

function eventTone(type: string) {
  if (type.startsWith("run_failed") || type === "tool_error") return "danger";
  if (type === "run_completed") return "success";
  if (type === "skill_recall") return "skill";
  return "neutral";
}

export function AgentPanel({
  settings,
  selectedDevice,
  instruction,
  onInstructionChange,
  allowHighRisk,
  onAllowHighRiskChange,
  running,
  runStatus,
  currentThought,
  currentAction,
  currentStep,
  runId,
  recalledSkills,
  events,
  error,
  onStart,
  onCancel,
  onOpenSettings,
}: Props) {
  const recentEvents = events.slice(-4).reverse();
  const configured = Boolean(settings?.hasApiKey && settings.model);
  const canStart =
    configured &&
    Boolean(selectedDevice?.ready) &&
    Boolean(instruction.trim()) &&
    !running;

  return (
    <aside className="agent-panel">
      <div className="agent-heading">
        <div>
          <span className="eyebrow">AGENT RUNTIME</span>
          <h2>一句话执行</h2>
        </div>
        <span className={`run-state ${running ? "is-running" : runStatus}`}>
          <i />
          {statusLabel(runStatus, running)}
        </span>
      </div>

      <div className="agent-composer">
        <div className="composer-label">
          <span>任务意图</span>
          <span>{instruction.length}/600</span>
        </div>
        <textarea
          value={instruction}
          onChange={(event) => onInstructionChange(event.target.value)}
          placeholder="例如：打开小红书搜索机械键盘，按销量排序，把前三个商品名称和价格记录到备忘录。"
          maxLength={600}
          rows={6}
          disabled={running}
        />
        <div className="suggestions">
          {suggestions.map((suggestion) => (
            <button
              type="button"
              key={suggestion}
              onClick={() => onInstructionChange(suggestion)}
              disabled={running}
            >
              {suggestion}
            </button>
          ))}
        </div>
      </div>

      <label className="risk-toggle">
        <span className={`check-box ${allowHighRisk ? "checked" : ""}`}>
          {allowHighRisk ? <Check size={12} strokeWidth={3} /> : null}
        </span>
        <input
          type="checkbox"
          checked={allowHighRisk}
          onChange={(event) => onAllowHighRiskChange(event.target.checked)}
          disabled={running}
        />
        <span>
          <strong>允许高风险动作</strong>
          <small>支付、发送、删除、安装等仍应保持在人工监督下。</small>
        </span>
        <ShieldAlert size={16} />
      </label>

      {!configured ? (
        <button type="button" className="setup-warning" onClick={onOpenSettings}>
          <WandSparkles size={17} />
          <span>
            <strong>需要配置多模态模型</strong>
            <small>填写 OpenAI-compatible Base URL、Key 与 Model</small>
          </span>
        </button>
      ) : null}

      {error ? (
        <div className="agent-error">
          <ShieldAlert size={16} />
          <span>{error}</span>
        </div>
      ) : null}

      <div className="run-actions">
        {running ? (
          <button type="button" className="stop-button" onClick={onCancel}>
            <CircleStop size={17} />
            停止 Agent
          </button>
        ) : (
          <button
            type="button"
            className="run-button"
            onClick={onStart}
            disabled={!canStart}
          >
            <Play size={16} fill="currentColor" />
            执行任务
            <span>⌘↵</span>
          </button>
        )}
      </div>

      <div className="runtime-card">
        <div className="runtime-card-head">
          <span>
            <BrainCircuit size={15} />
            Live reasoning
          </span>
          <code>{runId ? runId.slice(0, 8) : "no run"}</code>
        </div>
        <div className="runtime-stepbar">
          <span className="step-pulse" />
          <div>
            <strong>
              {currentAction
                ? `Step ${currentStep} · ${currentAction}`
                : running
                  ? "正在观察设备"
                  : "等待任务"}
            </strong>
            <p>
              {currentThought ||
                "Agent 会先读取截图和 UI 树，再选择最小可验证的下一步工具。"}
            </p>
          </div>
        </div>
        <div className="runtime-metrics">
          <div>
            <Zap size={14} />
            <span>VISION</span>
            <strong>{settings?.useVision ? "HYBRID" : "TREE"}</strong>
          </div>
          <div>
            <Sparkles size={14} />
            <span>MODEL</span>
            <strong>{settings?.model || "unset"}</strong>
          </div>
          <div>
            <BrainCircuit size={14} />
            <span>SKILLS</span>
            <strong>{recalledSkills.length}</strong>
          </div>
        </div>
      </div>

      {recalledSkills.length > 0 ? (
        <div className="recall-row">
          <span className="eyebrow">RECALLED SKILLS</span>
          <div>
            {recalledSkills.map((skill) => (
              <span className="recall-chip" key={skill.id}>
                <Sparkles size={12} />
                {skill.name}
              </span>
            ))}
          </div>
        </div>
      ) : null}

      <div className="agent-feed">
        <div className="feed-heading">
          <span className="eyebrow">LATEST SIGNALS</span>
          <span>{events.length} events</span>
        </div>
        {recentEvents.length === 0 ? (
          <div className="feed-empty">
            任务启动后，这里会实时显示模型判断和工具回执。
          </div>
        ) : (
          recentEvents.map((event, index) => (
            <div
              className={`feed-row ${eventTone(event.eventType)}`}
              key={`${event.timestamp}-${index}`}
            >
              <span className="feed-step">{event.stepIndex || "•"}</span>
              <div>
                <strong>{event.title}</strong>
                <p>{event.detail}</p>
              </div>
              <time>
                {new Date(event.timestamp).toLocaleTimeString([], {
                  hour: "2-digit",
                  minute: "2-digit",
                })}
              </time>
            </div>
          ))
        )}
      </div>
    </aside>
  );
}
