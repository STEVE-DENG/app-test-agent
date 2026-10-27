import {
  Activity,
  Braces,
  Camera,
  Copy,
  Database,
  Download,
  FileCode2,
  Gauge,
  Layers3,
  Play,
  RefreshCw,
  Square,
  TerminalSquare,
  Timer,
} from "lucide-react";
import { useMemo, useState } from "react";
import { api, errorMessage } from "../lib/api";
import type {
  AgentEvent,
  DebugSnapshot,
  DeviceInfo,
  EfficiencyStats,
  LearningLoopReport,
} from "../types";

type Props = {
  device: DeviceInfo | null;
  packageName: string;
  snapshot: DebugSnapshot | null;
  stats: EfficiencyStats | null;
  loading: boolean;
  error: string;
  events: AgentEvent[];
  learning: boolean;
  learningReport: LearningLoopReport | null;
  onCapture: () => void;
  onRefreshStats: () => void;
  onStartLearning: (input: {
    maxCycles: number;
    maxStepsPerCycle: number;
    stopAfterStagnantCycles: number;
  }) => void;
  onStopLearning: () => void;
  onExport: () => void;
};

function formatBytes(bytes: number) {
  if (bytes < 1024) return `${bytes} B`;
  if (bytes < 1024 * 1024) return `${(bytes / 1024).toFixed(1)} KB`;
  return `${(bytes / 1024 / 1024).toFixed(1)} MB`;
}

function formatMs(milliseconds: number) {
  if (milliseconds < 1000) return `${milliseconds} ms`;
  return `${(milliseconds / 1000).toFixed(1)} s`;
}

function timeLabel(timestamp: string) {
  if (!timestamp) return "—";
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function DebugPanel({
  device,
  packageName,
  snapshot,
  stats,
  loading,
  error,
  events,
  learning,
  learningReport,
  onCapture,
  onRefreshStats,
  onStartLearning,
  onStopLearning,
  onExport,
}: Props) {
  const [showRawXml, setShowRawXml] = useState(false);
  const [maxCycles, setMaxCycles] = useState(8);
  const [maxSteps, setMaxSteps] = useState(45);
  const [stagnant, setStagnant] = useState(2);
  const [selectedArtifact, setSelectedArtifact] = useState("");
  const [artifactContent, setArtifactContent] = useState("");
  const [artifactError, setArtifactError] = useState("");
  const [copied, setCopied] = useState(false);
  const [copiedKind, setCopiedKind] = useState("");

  const debugEvents = useMemo(
    () =>
      events
        .filter(
          (event) =>
            event.eventType.includes("learning") ||
            event.eventType.includes("exploration") ||
            event.eventType.includes("debug") ||
            event.eventType.includes("knowledge") ||
            event.eventType.startsWith("tool_") ||
            event.eventType === "thought",
        )
        .slice(-30)
        .reverse(),
    [events],
  );

  const copyValue = async (value: string, kind: string) => {
    try {
      await navigator.clipboard.writeText(value);
      setCopied(true);
      setCopiedKind(kind);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  const openArtifact = async (path: string, kind: string) => {
    setSelectedArtifact(path);
    setArtifactError("");
    setArtifactContent("");
    try {
      setArtifactContent(
        kind === "image"
          ? `图片预览：${await api.readArtifact(path)}`
          : await api.readTextArtifact(path),
      );
    } catch (nextError) {
      setArtifactError(errorMessage(nextError));
    }
  };

  return (
    <div className="debug-panel">
      <div className="debug-toolbar">
        <div>
          <span className="eyebrow">OBSERVABILITY CONSOLE</span>
          <strong>{device?.model || "未选择设备"}</strong>
          <code>{packageName || "尚未确定包名"}</code>
        </div>
        <div className="debug-actions">
          <button type="button" onClick={onCapture} disabled={!device?.ready || loading}>
            <Camera size={14} />
            {loading ? "采集中..." : "采集调试快照"}
          </button>
          <button type="button" onClick={onRefreshStats}>
            <RefreshCw size={14} />
            刷新指标
          </button>
          <button
            type="button"
            onClick={() =>
              snapshot &&
              void copyValue(JSON.stringify(snapshot, null, 2), "快照")
            }
            disabled={!snapshot}
          >
            <Copy size={14} />
            {copied && copiedKind === "快照" ? "已复制" : "复制快照 JSON"}
          </button>
          <button
            type="button"
            onClick={() => snapshot && void copyValue(snapshot.rawUiXml, "XML")}
            disabled={!snapshot}
          >
            <Copy size={14} />
            {copied && copiedKind === "XML" ? "已复制" : "复制 UI XML"}
          </button>
          <button
            type="button"
            onClick={() =>
              snapshot &&
              void copyValue(JSON.stringify(snapshot.nodes, null, 2), "节点")
            }
            disabled={!snapshot}
          >
            <Copy size={14} />
            {copied && copiedKind === "节点" ? "已复制" : "复制节点 JSON"}
          </button>
          <button type="button" onClick={onExport} disabled={!device?.ready}>
            <Download size={14} />
            导出调试包
          </button>
        </div>
      </div>

      {error ? <div className="debug-error">{error}</div> : null}

      <div className="debug-metrics">
        <div>
          <Gauge size={15} />
          <span>任务</span>
          <strong>{stats?.totalRuns || 0}</strong>
          <small>
            {stats?.completedRuns || 0} 完成 / {stats?.failedRuns || 0} 失败
          </small>
        </div>
        <div>
          <Timer size={15} />
          <span>平均每步</span>
          <strong>{formatMs(stats?.avgStepMs || 0)}</strong>
          <small>模型 {formatMs(stats?.totalModelMs || 0)} · 工具 {formatMs(stats?.totalToolMs || 0)}</small>
        </div>
        <div>
          <Layers3 size={15} />
          <span>页面 / 跳转</span>
          <strong>
            {stats?.pages || 0} / {stats?.transitions || 0}
          </strong>
          <small>{stats?.testCases || 0} 条生成用例</small>
        </div>
        <div>
          <Database size={15} />
          <span>经验 / 证据</span>
          <strong>
            {stats?.approvedSkills || 0} / {stats?.artifacts || 0}
          </strong>
          <small>{stats?.pendingSkills || 0} 个 Skill 待审核</small>
        </div>
      </div>

      <div className="debug-grid">
        <section className="debug-card learning-card">
          <div className="debug-card-head">
            <div>
              <span className="eyebrow">CONTINUOUS LEARNING</span>
              <h3>持续遍历闲鱼</h3>
            </div>
            <span className={`debug-live ${learning ? "is-live" : ""}`}>
              <i />
              {learning ? "学习中" : "待启动"}
            </span>
          </div>
          <p>
            每轮自动启动 App、探索未知控件、记录截图/UI 树/跳转边，并在多轮无增长后停止。
          </p>
          <div className="learning-controls">
            <label>
              <span>轮数</span>
              <input
                type="number"
                min={1}
                max={100}
                value={maxCycles}
                onChange={(event) => setMaxCycles(Number(event.target.value) || 1)}
                disabled={learning}
              />
            </label>
            <label>
              <span>每轮步数</span>
              <input
                type="number"
                min={1}
                max={300}
                value={maxSteps}
                onChange={(event) => setMaxSteps(Number(event.target.value) || 1)}
                disabled={learning}
              />
            </label>
            <label>
              <span>无增长停止</span>
              <input
                type="number"
                min={1}
                max={20}
                value={stagnant}
                onChange={(event) => setStagnant(Number(event.target.value) || 1)}
                disabled={learning}
              />
            </label>
          </div>
          <div className="learning-buttons">
            {learning ? (
              <button type="button" className="debug-stop" onClick={onStopLearning}>
                <Square size={13} fill="currentColor" />
                停止学习
              </button>
            ) : (
              <button
                type="button"
                className="debug-primary"
                disabled={!device?.ready || !packageName}
                onClick={() =>
                  onStartLearning({
                    maxCycles,
                    maxStepsPerCycle: maxSteps,
                    stopAfterStagnantCycles: stagnant,
                  })
                }
              >
                <Play size={13} fill="currentColor" />
                启动长期 Loop
              </button>
            )}
            <span>
              目标：<code>{packageName || "先采集一帧确定 App"}</code>
            </span>
          </div>
          {learningReport ? (
            <div className="learning-report">
              <div>
                <strong>{learningReport.cyclesCompleted}</strong>
                <span>轮完成</span>
              </div>
              <div>
                <strong>{learningReport.totalSteps}</strong>
                <span>步</span>
              </div>
              <div>
                <strong>{learningReport.pages}</strong>
                <span>页面</span>
              </div>
              <div>
                <strong>{learningReport.transitions}</strong>
                <span>跳转</span>
              </div>
              <p>{learningReport.summary}</p>
            </div>
          ) : (
            <div className="debug-muted">
              启动后每轮会推送 learning_round_completed 事件，可在执行轨迹中查看。
            </div>
          )}
        </section>

        <section className="debug-card snapshot-card">
          <div className="debug-card-head">
            <div>
              <span className="eyebrow">CURRENT SNAPSHOT</span>
              <h3>混合观察事实</h3>
            </div>
            <Activity size={17} />
          </div>
          {snapshot ? (
            <>
              <div className="snapshot-meta">
                <div>
                  <span>PAGE ID</span>
                  <code>{snapshot.pageId}</code>
                </div>
                <div>
                  <span>SIGNATURE</span>
                  <code>{snapshot.pageSignature.slice(0, 20)}</code>
                </div>
                <div>
                  <span>CAPTURED</span>
                  <code>{timeLabel(snapshot.capturedAt)}</code>
                </div>
                <div>
                  <span>NODES</span>
                  <code>
                    {snapshot.nodeCount} / actionable {snapshot.actionableNodeCount}
                  </code>
                </div>
              </div>
              <div className="snapshot-body">
                <img src={snapshot.screenshotDataUrl} alt="debug snapshot" />
                <div className="snapshot-facts">
                  <span>
                    <TerminalSquare size={13} /> {snapshot.activity || "Unknown activity"}
                  </span>
                  <span>
                    <FileCode2 size={13} /> {snapshot.visibleNodeCount} visible ·{" "}
                    {snapshot.scrollableNodeCount} scrollable
                  </span>
                  <code>{snapshot.package}</code>
                  <code>{snapshot.uiPath}</code>
                  <code>{snapshot.screenshotPath}</code>
                </div>
              </div>
              <div className="raw-toggle-row">
                <button type="button" onClick={() => setShowRawXml((value) => !value)}>
                  <Braces size={13} />
                  {showRawXml ? "隐藏原始 UI XML" : "查看原始 UI XML"}
                </button>
                <span>{snapshot.dataDir}</span>
              </div>
              {showRawXml ? (
                <pre className="raw-xml">{snapshot.rawUiXml || "没有保存 UI XML"}</pre>
              ) : null}
            </>
          ) : (
            <div className="debug-empty">
              <Camera size={22} />
              <span>点击「采集调试快照」，一次拿到截图、UI XML、页面签名和耗时。</span>
            </div>
          )}
        </section>

        <section className="debug-card artifacts-card">
          <div className="debug-card-head">
            <div>
              <span className="eyebrow">EVIDENCE FILES</span>
              <h3>证据与原始数据</h3>
            </div>
            <span>{snapshot?.artifacts.length || 0} files</span>
          </div>
          <div className="artifact-list">
            {snapshot?.artifacts.length ? (
              snapshot.artifacts.slice(0, 30).map((artifact) => (
                <button
                  type="button"
                  key={artifact.path}
                  className={selectedArtifact === artifact.path ? "is-active" : ""}
                  onClick={() => void openArtifact(artifact.path, artifact.kind)}
                >
                  <span className={`artifact-kind ${artifact.kind}`}>{artifact.kind}</span>
                  <span>
                    <strong>{artifact.name}</strong>
                    <small>
                      {formatBytes(artifact.size)} · {timeLabel(artifact.modifiedAt)}
                    </small>
                  </span>
                </button>
              ))
            ) : (
              <div className="debug-muted">还没有证据文件。采集或跑一次任务后会生成。</div>
            )}
          </div>
          {selectedArtifact ? (
            <div className="artifact-view">
              <div>
                <span>SELECTED ARTIFACT</span>
                <code>{selectedArtifact}</code>
              </div>
              {artifactError ? <span className="artifact-error">{artifactError}</span> : null}
              {artifactContent ? (
                artifactContent.startsWith("图片预览：") ? (
                  <img
                    src={artifactContent.slice("图片预览：".length)}
                    alt="selected artifact"
                  />
                ) : (
                  <pre>{artifactContent.slice(0, 24_000)}</pre>
                )
              ) : null}
            </div>
          ) : null}
        </section>

        <section className="debug-card events-card">
          <div className="debug-card-head">
            <div>
              <span className="eyebrow">LIVE EVENT STREAM</span>
              <h3>循环与知识事件</h3>
            </div>
            <span>{debugEvents.length} recent</span>
          </div>
          <div className="debug-event-list">
            {debugEvents.length ? (
              debugEvents.map((event, index) => (
                <div key={`${event.timestamp}-${index}`}>
                  <span>{timeLabel(event.timestamp)}</span>
                  <strong>{event.eventType}</strong>
                  <p>{event.detail}</p>
                </div>
              ))
            ) : (
              <div className="debug-muted">
                启动持续学习或自动遍历后，这里会显示每轮发现、跳转和知识写入事件。
              </div>
            )}
          </div>
        </section>
      </div>
    </div>
  );
}

