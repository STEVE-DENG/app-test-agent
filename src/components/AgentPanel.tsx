import {
  BrainCircuit,
  Check,
  ChevronDown,
  CircleStop,
  MessageSquarePlus,
  Archive,
  Play,
  ShieldAlert,
  Sparkles,
  Zap,
} from "lucide-react";
import { useEffect, useRef, useState } from "react";
import type {
  AgentEvent,
  ConversationMessage,
  ConversationSummary,
  DeviceInfo,
  SettingsView,
  SkillRecord,
} from "../types";

type Props = {
  settings: SettingsView | null;
  selectedDevice: DeviceInfo | null;
  conversations: ConversationSummary[];
  conversationId: string | null;
  messages: ConversationMessage[];
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
  onNewConversation: () => void;
  onSelectConversation: (conversationId: string) => void;
  onArchiveConversation: () => void;
  onOpenSettings: () => void;
};

const suggestions = [
  "返回闲鱼首页",
  "现在页面上有哪些可操作控件？",
  "继续刚才的操作，并用截图确认结果",
];

function statusLabel(status: string, running: boolean) {
  if (running) return "执行中";
  if (status === "completed") return "已完成";
  if (status === "failed") return "失败";
  if (status === "cancelled") return "已停止";
  if (status === "needs_input") return "等待补充";
  return "就绪";
}

export function AgentPanel({
  settings,
  selectedDevice,
  conversations,
  conversationId,
  messages,
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
  onNewConversation,
  onSelectConversation,
  onArchiveConversation,
  onOpenSettings,
}: Props) {
  const [historyOpen, setHistoryOpen] = useState(false);
  const streamRef = useRef<HTMLDivElement>(null);
  const activeConversation = conversations.find(
    (conversation) => conversation.id === conversationId,
  );
  const configured = Boolean(settings?.hasApiKey && settings.model);
  const canStart =
    configured &&
    Boolean(selectedDevice?.ready) &&
    Boolean(instruction.trim()) &&
    !running;

  useEffect(() => {
    const stream = streamRef.current;
    if (stream) stream.scrollTop = stream.scrollHeight;
  }, [messages, events, running]);

  return (
    <aside className="agent-panel">
      <div className="agent-heading">
        <div>
          <span className="eyebrow">CONTINUOUS SESSION</span>
          <h2>Agent 对话</h2>
        </div>
        <div className="conversation-head-actions">
          <button
            type="button"
            className="conversation-history-button"
            onClick={() => setHistoryOpen((value) => !value)}
            aria-expanded={historyOpen}
          >
            历史
            <ChevronDown size={12} />
          </button>
          <button
            type="button"
            className="new-conversation-button"
            onClick={onNewConversation}
            disabled={running}
            aria-label="新建会话"
            title="新建会话"
          >
            <MessageSquarePlus size={15} />
          </button>
          <button
            type="button"
            className="new-conversation-button"
            onClick={onArchiveConversation}
            disabled={!conversationId || running}
            aria-label="归档当前会话"
            title="归档当前会话"
          >
            <Archive size={14} />
          </button>
        </div>
      </div>

      <div className="conversation-statusbar">
        <span className={`run-state ${running ? "is-running" : runStatus}`}>
          <i />
          {statusLabel(runStatus, running)}
        </span>
        <span className="conversation-title">
          {activeConversation?.title || "新会话 · 第一条消息将创建上下文"}
        </span>
      </div>

      {historyOpen ? (
        <div className="conversation-history">
          {conversations.length === 0 ? (
            <p>还没有历史会话。</p>
          ) : (
            conversations.map((conversation) => (
              <button
                type="button"
                key={conversation.id}
                className={
                  conversation.id === conversationId ? "is-active" : ""
                }
                onClick={() => {
                  onSelectConversation(conversation.id);
                  setHistoryOpen(false);
                }}
              >
                <span>
                  <strong>{conversation.title}</strong>
                  <small>{conversation.lastMessage || "暂无消息"}</small>
                </span>
                <time>
                  {new Date(conversation.updatedAt).toLocaleDateString([], {
                    month: "2-digit",
                    day: "2-digit",
                  })}
                </time>
              </button>
            ))
          )}
        </div>
      ) : null}

      <div className="conversation-stream" ref={streamRef}>
        {messages.length === 0 ? (
          <div className="conversation-welcome">
            <span className="welcome-mark">
              <BrainCircuit size={21} />
            </span>
            <h3>下一句话，接着做。</h3>
            <p>
              说“打开闲鱼搜相机”“继续往下滑”“刚才那个页面有什么按钮”。会话会保留上下文，Agent
              也会记住设备上已验证的路线。
            </p>
          </div>
        ) : (
          messages.map((message) => {
            const relatedEvents =
              message.role === "assistant" && message.runId
                ? events
                    .filter((event) => event.runId === message.runId)
                    .slice(-8)
                : [];
            return (
              <article
                key={message.id}
                className={`chat-message ${message.role} ${
                  message.status || ""
                }`}
              >
                <div className="chat-message-meta">
                  <span>
                    {message.role === "user" ? "你" : "APP PILOT"}
                  </span>
                  <time>
                    {new Date(message.createdAt).toLocaleTimeString([], {
                      hour: "2-digit",
                      minute: "2-digit",
                    })}
                  </time>
                </div>
                <p>{message.content}</p>
                {relatedEvents.length > 0 ? (
                  <details className="chat-run-trace">
                    <summary>
                      查看执行轨迹 · {relatedEvents.length} 条事件
                    </summary>
                    <div>
                      {relatedEvents.map((event, index) => (
                        <span key={`${event.timestamp}-${index}`}>
                          <b>{event.title}</b>
                          {event.detail}
                        </span>
                      ))}
                    </div>
                  </details>
                ) : null}
              </article>
            );
          })
        )}

        {running ? (
          <div className="chat-running">
            <span className="typing-dots">
              <i />
              <i />
              <i />
            </span>
            <div>
              <strong>
                {currentAction
                  ? `Step ${currentStep} · ${currentAction}`
                  : "Agent 正在观察页面"}
              </strong>
              <p>
                {currentThought ||
                  "正在融合截图、UI 树、OCR 与历史经验，选择下一步。"}
              </p>
            </div>
          </div>
        ) : null}
      </div>

      {!configured ? (
        <button type="button" className="setup-warning" onClick={onOpenSettings}>
          <Sparkles size={17} />
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

      <div className="conversation-composer">
        <div className="composer-label">
          <span>{running ? "Agent 运行中，可先编辑下一条" : "继续对话"}</span>
          <span>{instruction.length}/600</span>
        </div>
        <textarea
          value={instruction}
          onChange={(event) => onInstructionChange(event.target.value)}
          onKeyDown={(event) => {
            if (event.key === "Enter" && !event.shiftKey && !event.nativeEvent.isComposing) {
              event.preventDefault();
              if (canStart) onStart();
            }
          }}
          placeholder="例如：继续下滑，找到“发布”，先不要提交。"
          maxLength={600}
          rows={3}
        />
        <div className="suggestions">
          {suggestions.map((suggestion) => (
            <button
              type="button"
              key={suggestion}
              onClick={() => onInstructionChange(suggestion)}
            >
              {suggestion}
            </button>
          ))}
        </div>
        <div className="composer-footer">
          <label className="inline-risk-toggle">
            <span className={`check-box ${allowHighRisk ? "checked" : ""}`}>
              {allowHighRisk ? <Check size={12} strokeWidth={3} /> : null}
            </span>
            <input
              type="checkbox"
              checked={allowHighRisk}
              onChange={(event) => onAllowHighRiskChange(event.target.checked)}
              disabled={running}
            />
            <span>允许高风险动作</span>
            <ShieldAlert size={13} />
          </label>
          {running ? (
            <button type="button" className="stop-button" onClick={onCancel}>
              <CircleStop size={16} />
              停止
            </button>
          ) : (
            <button
              type="button"
              className="send-button"
              onClick={onStart}
              disabled={!canStart}
            >
              <Play size={14} fill="currentColor" />
              发送
            </button>
          )}
        </div>
      </div>

      <div className="runtime-card compact">
        <div className="runtime-card-head">
          <span>
            <Zap size={14} />
            混合感知在线
          </span>
          <code>{runId ? runId.slice(0, 8) : "continuous"}</code>
        </div>
        <div className="runtime-metrics">
          <div>
            <Sparkles size={14} />
            <span>VISION</span>
            <strong>{settings?.useVision ? "HYBRID" : "OFF"}</strong>
          </div>
          <div>
            <BrainCircuit size={14} />
            <span>MODEL</span>
            <strong>{settings?.model || "unset"}</strong>
          </div>
          <div>
            <Sparkles size={14} />
            <span>SKILLS</span>
            <strong>{recalledSkills.length}</strong>
          </div>
        </div>
      </div>

      {recalledSkills.length > 0 ? (
        <div className="recall-row compact">
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
    </aside>
  );
}
