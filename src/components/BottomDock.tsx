import {
  Check,
  Clock3,
  BugPlay,
  History,
  LibraryBig,
  Network,
  ScanSearch,
  Sparkles,
  X,
} from "lucide-react";
import { DebugPanel } from "./DebugPanel";
import { KnowledgePanel } from "./KnowledgePanel";
import { RunTracePanel } from "./RunTracePanel";
import type {
  AgentEvent,
  AppKnowledgePlugin,
  AppPluginSummary,
  DebugSnapshot,
  DeviceInfo,
  EfficiencyStats,
  LearningLoopReport,
  ModelTraceRecord,
  Observation,
  RunRecord,
  SkillRecord,
  StepMetricRecord,
  ToolRecord,
  UiNode,
} from "../types";

export type DockTab =
  | "timeline"
  | "knowledge"
  | "skills"
  | "runs"
  | "inspector"
  | "debug";

type Props = {
  activeTab: DockTab;
  onTabChange: (tab: DockTab) => void;
  events: AgentEvent[];
  appPlugins: AppPluginSummary[];
  appPlugin: AppKnowledgePlugin | null;
  selectedPackage: string;
  selectedDevice: DeviceInfo | null;
  selectedPageId: string | null;
  exploring: boolean;
  knowledgeError: string;
  skills: SkillRecord[];
  runs: RunRecord[];
  runSteps: ToolRecord[];
  runMetrics: StepMetricRecord[];
  runModelTraces: ModelTraceRecord[];
  selectedRunId: string | null;
  observation: Observation | null;
  selectedNode: UiNode | null;
  debugSnapshot: DebugSnapshot | null;
  efficiency: EfficiencyStats | null;
  debugLoading: boolean;
  debugError: string;
  learning: boolean;
  learningReport: LearningLoopReport | null;
  onDebugCapture: () => void;
  onRefreshStats: () => void;
  onStartLearning: (input: {
    maxCycles: number;
    maxStepsPerCycle: number;
    stopAfterStagnantCycles: number;
  }) => void;
  onStopLearning: () => void;
  onExportDebug: () => void;
  onSelectNode: (node: UiNode) => void;
  onSelectRun: (runId: string) => void;
  onOpenDebug: () => void;
  onOpenPage: (pageId: string) => void;
  onSkillStatus: (skillId: string, status: string) => void;
  onSelectPackage: (packageName: string) => void;
  onSelectPage: (pageId: string) => void;
  onRefreshKnowledge: () => void;
  onStartExploration: (maxSteps: number) => void;
  onStopExploration: () => void;
  onGenerateTests: () => void;
  onCompactPlugin: () => void;
  onRunTest: (testId: string) => void;
  onExportPlugin: () => void;
  onImportPlugin: (file: File) => void;
};

const tabs: Array<{ id: DockTab; label: string; icon: typeof Clock3 }> = [
  { id: "timeline", label: "执行轨迹", icon: Clock3 },
  { id: "knowledge", label: "页面图谱", icon: Network },
  { id: "skills", label: "经验库", icon: LibraryBig },
  { id: "runs", label: "任务历史", icon: History },
  { id: "inspector", label: "UI 检查器", icon: ScanSearch },
  { id: "debug", label: "调试台", icon: BugPlay },
];

function statusLabel(status: string) {
  if (status === "completed") return "完成";
  if (status === "failed") return "失败";
  if (status === "cancelled") return "停止";
  if (status === "needs_input") return "待补充";
  if (status === "running") return "运行中";
  return status;
}

function timeLabel(timestamp: string) {
  return new Date(timestamp).toLocaleTimeString([], {
    hour: "2-digit",
    minute: "2-digit",
    second: "2-digit",
  });
}

export function BottomDock({
  activeTab,
  onTabChange,
  events,
  appPlugins,
  appPlugin,
  selectedPackage,
  selectedDevice,
  selectedPageId,
  exploring,
  knowledgeError,
  skills,
  runs,
  runSteps,
  runMetrics,
  runModelTraces,
  selectedRunId,
  observation,
  selectedNode,
  debugSnapshot,
  efficiency,
  debugLoading,
  debugError,
  learning,
  learningReport,
  onDebugCapture,
  onRefreshStats,
  onStartLearning,
  onStopLearning,
  onExportDebug,
  onSelectNode,
  onSelectRun,
  onOpenDebug,
  onOpenPage,
  onSkillStatus,
  onSelectPackage,
  onSelectPage,
  onRefreshKnowledge,
  onStartExploration,
  onStopExploration,
  onGenerateTests,
  onCompactPlugin,
  onRunTest,
  onExportPlugin,
  onImportPlugin,
}: Props) {
  const pendingSkills = skills.filter((skill) => skill.status === "pending").length;
  const selectedRun = runs.find((run) => run.id === selectedRunId);

  return (
    <section className="bottom-dock">
      <div className="dock-head">
        <div className="dock-tabs">
          {tabs.map((tab) => {
            const Icon = tab.icon;
            const count =
              tab.id === "timeline"
                ? events.length
                : tab.id === "knowledge"
                  ? appPlugin?.pages.length || 0
                : tab.id === "skills"
                  ? skills.length
                  : tab.id === "runs"
                    ? runs.length
                    : tab.id === "debug"
                      ? debugSnapshot?.artifacts.length || 0
                      : observation?.nodes.length || 0;
            return (
              <button
                key={tab.id}
                type="button"
                className={activeTab === tab.id ? "is-active" : ""}
                onClick={() => onTabChange(tab.id)}
              >
                <Icon size={14} />
                {tab.label}
                <span>{count}</span>
              </button>
            );
          })}
        </div>
        <div className="dock-summary">
          {pendingSkills > 0 ? (
            <span className="pending-chip">
              <Sparkles size={12} />
              {pendingSkills} 个技能待审核
            </span>
          ) : (
            <span>SQLite trace · local first</span>
          )}
        </div>
      </div>

      <div className="dock-content">
        {activeTab === "timeline" ? (
          events.length === 0 ? (
            <div className="dock-empty">
              <Clock3 size={22} />
              <span>执行一次任务，观察、判断、工具和证据会出现在这里。</span>
            </div>
          ) : (
            <div className="timeline-list">
              {events.map((event, index) => (
                <article
                  className={`timeline-item ${event.eventType}`}
                  key={`${event.timestamp}-${index}`}
                >
                  <div className="timeline-rail">
                    <span>{event.stepIndex || "•"}</span>
                  </div>
                  <div className="timeline-body">
                    <div className="timeline-title">
                      <strong>{event.title}</strong>
                      <time>{timeLabel(event.timestamp)}</time>
                    </div>
                    <p>{event.detail}</p>
                  </div>
                </article>
              ))}
            </div>
          )
        ) : null}

        {activeTab === "knowledge" ? (
          <KnowledgePanel
            plugins={appPlugins}
            plugin={appPlugin}
            selectedPackage={selectedPackage}
            selectedPageId={selectedPageId}
            exploring={exploring}
            error={knowledgeError}
            onSelectPackage={onSelectPackage}
            onSelectPage={onSelectPage}
            onRefresh={onRefreshKnowledge}
            onStartExploration={onStartExploration}
            onStopExploration={onStopExploration}
            onGenerateTests={onGenerateTests}
            onCompact={onCompactPlugin}
            onRunTest={onRunTest}
            onExport={onExportPlugin}
            onImport={onImportPlugin}
          />
        ) : null}

        {activeTab === "skills" ? (
          skills.length === 0 ? (
            <div className="dock-empty">
              <LibraryBig size={22} />
              <span>成功任务会生成候选 Skill。批准后，相似任务会优先召回。</span>
            </div>
          ) : (
            <div className="skill-grid">
              {skills.map((skill) => (
                <article className={`skill-card ${skill.status}`} key={skill.id}>
                  <div className="skill-card-head">
                    <div>
                      <span className={`skill-status ${skill.status}`}>
                        {skill.status === "pending"
                          ? "待审核"
                          : skill.status === "approved"
                            ? "已批准"
                            : "已拒绝"}
                      </span>
                      <h3>{skill.name}</h3>
                    </div>
                    <code>{skill.plan.length} steps</code>
                  </div>
                  <p>{skill.description}</p>
                  <div className="skill-keywords">
                    {skill.keywords.slice(0, 5).map((keyword) => (
                      <span key={keyword}>{keyword}</span>
                    ))}
                  </div>
                  <div className="skill-stats">
                    <span>复用 {skill.reuseCount}</span>
                    <span>成功 {skill.successCount}</span>
                    <span>失败 {skill.failureCount}</span>
                    <span>{skill.package || "any app"}</span>
                  </div>
                  {skill.status === "pending" ? (
                    <div className="skill-actions">
                      <button
                        type="button"
                        className="approve"
                        onClick={() => onSkillStatus(skill.id, "approved")}
                      >
                        <Check size={13} />
                        批准
                      </button>
                      <button
                        type="button"
                        onClick={() => onSkillStatus(skill.id, "rejected")}
                      >
                        <X size={13} />
                        拒绝
                      </button>
                    </div>
                  ) : null}
                </article>
              ))}
            </div>
          )
        ) : null}

        {activeTab === "runs" ? (
          <div className="runs-layout">
            <div className="run-list">
              {runs.length === 0 ? (
                <div className="dock-empty compact-empty">
                  <History size={20} />
                  <span>还没有任务历史</span>
                </div>
              ) : (
                runs.map((run) => (
                  <button
                    type="button"
                    key={run.id}
                    className={selectedRunId === run.id ? "is-active" : ""}
                    onClick={() => onSelectRun(run.id)}
                  >
                    <span className={`run-status-dot ${run.status}`} />
                    <span className="run-list-main">
                      <strong>{run.instruction}</strong>
                      <small>
                        {new Date(run.startedAt).toLocaleString()} · {run.steps} steps
                      </small>
                    </span>
                    <span className={`run-badge ${run.status}`}>
                      {statusLabel(run.status)}
                    </span>
                  </button>
                ))
              )}
            </div>
            <div className="run-detail">
              {selectedRun ? (
                <RunTracePanel
                  key={selectedRun.id}
                  run={selectedRun}
                  steps={runSteps}
                  metrics={runMetrics}
                  modelTraces={runModelTraces}
                  onOpenDebug={onOpenDebug}
                  onOpenPage={onOpenPage}
                />
              ) : (
                <div className="dock-empty compact-empty">
                  <ScanSearch size={20} />
                  <span>选择一个历史任务查看步骤与证据</span>
                </div>
              )}
            </div>
          </div>
        ) : null}

        {activeTab === "inspector" ? (
          observation ? (
            <div className="tree-layout">
              <div className="tree-list">
                {observation.nodes.map((node) => (
                  <button
                    type="button"
                    key={node.index}
                    className={selectedNode?.index === node.index ? "is-active" : ""}
                    onClick={() => onSelectNode(node)}
                    style={{ paddingLeft: `${10 + node.depth * 9}px` }}
                  >
                    <span className="tree-index">{node.index}</span>
                    <span className="tree-class">
                      {node.className.split(".").slice(-1)}
                    </span>
                    <span className="tree-text">
                      {node.text || node.contentDescription || node.resourceId || "—"}
                    </span>
                    <span className="tree-flags">
                      {node.clickable ? "C" : ""}
                      {node.scrollable ? "S" : ""}
                      {node.focusable ? "F" : ""}
                    </span>
                  </button>
                ))}
              </div>
              <div className="tree-detail">
                {selectedNode ? (
                  <>
                    <span className="eyebrow">NODE DETAIL</span>
                    <h3>
                      {selectedNode.text ||
                        selectedNode.contentDescription ||
                        `Node ${selectedNode.index}`}
                    </h3>
                    <dl>
                      <dt>resource-id</dt>
                      <dd>{selectedNode.resourceId || "—"}</dd>
                      <dt>class</dt>
                      <dd>{selectedNode.className}</dd>
                      <dt>package</dt>
                      <dd>{selectedNode.package}</dd>
                      <dt>bounds</dt>
                      <dd>
                        [{selectedNode.bounds.left},{selectedNode.bounds.top}][
                        {selectedNode.bounds.right},{selectedNode.bounds.bottom}]
                      </dd>
                      <dt>state</dt>
                      <dd>
                        {selectedNode.clickable ? "clickable" : "static"} ·{" "}
                        {selectedNode.enabled ? "enabled" : "disabled"} ·{" "}
                        {selectedNode.visible ? "visible" : "hidden"}
                      </dd>
                    </dl>
                  </>
                ) : (
                  <div className="dock-empty compact-empty">
                    <ScanSearch size={20} />
                    <span>从左侧 UI 树选择一个节点</span>
                  </div>
                )}
              </div>
            </div>
          ) : (
            <div className="dock-empty">
              <ScanSearch size={22} />
              <span>采集当前屏幕后，可在这里浏览完整 accessibility 树。</span>
            </div>
          )
        ) : null}

        {activeTab === "debug" ? (
          <DebugPanel
            device={selectedDevice}
            packageName={selectedPackage || observation?.package || ""}
            snapshot={debugSnapshot}
            stats={efficiency}
            loading={debugLoading}
            error={debugError}
            events={events}
            learning={learning}
            learningReport={learningReport}
            onCapture={onDebugCapture}
            onRefreshStats={onRefreshStats}
            onStartLearning={onStartLearning}
            onStopLearning={onStopLearning}
            onExport={onExportDebug}
          />
        ) : null}
      </div>
    </section>
  );
}
