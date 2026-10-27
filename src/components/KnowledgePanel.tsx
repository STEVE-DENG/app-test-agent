import {
  ArrowDownToLine,
  Camera,
  CircleStop,
  Combine,
  FileJson,
  GitBranch,
  Layers3,
  Play,
  RefreshCw,
  Route,
  ScanSearch,
  Sparkles,
  Upload,
} from "lucide-react";
import { useEffect, useMemo, useRef, useState } from "react";
import { api, errorMessage } from "../lib/api";
import type {
  ActionTrace,
  AppKnowledgePlugin,
  AppPluginSummary,
  KnowledgeElement,
  KnowledgePage,
  PageTransition,
} from "../types";

type Props = {
  plugins: AppPluginSummary[];
  plugin: AppKnowledgePlugin | null;
  selectedPackage: string;
  selectedPageId: string | null;
  exploring: boolean;
  error: string;
  onSelectPackage: (packageName: string) => void;
  onSelectPage: (pageId: string) => void;
  onRefresh: () => void;
  onStartExploration: (maxSteps: number) => void;
  onStopExploration: () => void;
  onGenerateTests: () => void;
  onCompact: () => void;
  onRunTest: (testId: string) => void;
  onExport: () => void;
  onImport: (file: File) => void;
};

type GraphNode = {
  page: KnowledgePage;
  x: number;
  y: number;
  depth: number;
};

const NODE_WIDTH = 168;
const NODE_HEIGHT = 52;
const COLUMN_GAP = 218;
const ROW_GAP = 70;

function graphLayout(plugin: AppKnowledgePlugin | null) {
  if (!plugin || plugin.pages.length === 0) {
    return {
      nodes: [] as GraphNode[],
      edges: [] as Array<{
        transition: PageTransition;
        from: GraphNode;
        to: GraphNode;
      }>,
      width: 720,
      height: 230,
    };
  }
  const pageById = new Map(plugin.pages.map((page) => [page.id, page]));
  const start =
    plugin.pages.find((page) => page.activity.endsWith("MainActivity")) ||
    plugin.pages[0];
  const adjacency = new Map<string, string[]>();
  for (const transition of plugin.transitions) {
    if (!adjacency.has(transition.fromPageId)) {
      adjacency.set(transition.fromPageId, []);
    }
    adjacency.get(transition.fromPageId)?.push(transition.toPageId);
  }
  const depth = new Map<string, number>([[start.id, 0]]);
  const queue = [start.id];
  while (queue.length > 0) {
    const current = queue.shift() as string;
    for (const next of adjacency.get(current) || []) {
      if (!depth.has(next)) {
        depth.set(next, (depth.get(current) || 0) + 1);
        queue.push(next);
      }
    }
  }
  const maxDepth = Math.max(0, ...depth.values());
  for (const page of plugin.pages) {
    if (!depth.has(page.id)) depth.set(page.id, maxDepth + 1);
  }
  const groups = new Map<number, KnowledgePage[]>();
  for (const page of plugin.pages) {
    const level = depth.get(page.id) || 0;
    if (!groups.has(level)) groups.set(level, []);
    groups.get(level)?.push(page);
  }
  const nodes: GraphNode[] = [];
  for (const [level, pages] of [...groups.entries()].sort(
    (left, right) => left[0] - right[0],
  )) {
    pages.forEach((page, row) => {
      nodes.push({
        page,
        depth: level,
        x: 26 + level * COLUMN_GAP,
        y: 28 + row * ROW_GAP,
      });
    });
  }
  const nodeById = new Map(nodes.map((node) => [node.page.id, node]));
  const edges = plugin.transitions
    .filter(
      (transition) =>
        pageById.has(transition.fromPageId) &&
        pageById.has(transition.toPageId),
    )
    .map((transition) => ({
      transition,
      from: nodeById.get(transition.fromPageId) as GraphNode,
      to: nodeById.get(transition.toPageId) as GraphNode,
    }))
    .filter((edge) => Boolean(edge.from && edge.to));
  const columns = Math.max(...nodes.map((node) => node.depth), 0) + 1;
  const maxRows = Math.max(...[...groups.values()].map((pages) => pages.length), 1);
  return {
    nodes,
    edges,
    width: Math.max(720, columns * COLUMN_GAP + 40),
    height: Math.max(230, maxRows * ROW_GAP + 28),
  };
}

function shortActivity(activity: string) {
  return activity.split(".").slice(-2).join(".");
}

function clip(text: string, limit: number) {
  const chars = [...text];
  return chars.length > limit ? `${chars.slice(0, limit).join("")}…` : text;
}

function testStatus(status: string) {
  if (status === "verified") return "已验证";
  if (status === "failed") return "失败";
  if (status === "running") return "执行中";
  return "已生成";
}

type ElementCoverage = {
  element: KnowledgeElement;
  attempts: number;
  successes: number;
  transitions: number;
};

type PageCoverage = {
  page: KnowledgePage;
  elements: ElementCoverage[];
  actionable: number;
  attempted: number;
  verified: number;
  coveragePercent: number;
  verifiedPercent: number;
  uncovered: KnowledgeElement[];
};

type PluginCoverage = {
  pages: Map<string, PageCoverage>;
  actionable: number;
  attempted: number;
  verified: number;
  coveragePercent: number;
};

function coverageElementLabel(element: KnowledgeElement) {
  return (
    element.text ||
    element.contentDescription ||
    element.resourceId ||
    element.className.split(".").slice(-1)[0] ||
    ""
  ).trim();
}

function isCoverageElement(element: KnowledgeElement) {
  return (
    (element.clickable || element.scrollable || element.focusable) &&
    coverageElementLabel(element).length > 0
  );
}

function actionMatchesElement(
  action: ActionTrace,
  pageId: string,
  element: KnowledgeElement,
) {
  if (action.fromPageId !== pageId) return false;
  const elementKeyValue = action.args.elementKey;
  if (typeof elementKeyValue === "string" && elementKeyValue === element.key) {
    return true;
  }
  const resourceId = action.args.resource_id;
  if (
    typeof resourceId === "string" &&
    resourceId &&
    element.resourceId &&
    (element.resourceId === resourceId ||
      element.resourceId.includes(resourceId) ||
      resourceId.includes(element.resourceId))
  ) {
    return true;
  }
  const text = action.args.text;
  if (typeof text === "string" && text.trim()) {
    const needle = text.trim().toLowerCase();
    const candidates = [
      element.text,
      element.contentDescription,
      element.resourceId,
    ]
      .map((value) => value.trim().toLowerCase())
      .filter(Boolean);
    if (
      candidates.some(
        (candidate) =>
          candidate === needle ||
          candidate.includes(needle) ||
          needle.includes(candidate),
      )
    ) {
      return true;
    }
  }
  const x = action.args.x;
  const y = action.args.y;
  return (
    typeof x === "number" &&
    typeof y === "number" &&
    x >= element.bounds.left &&
    x <= element.bounds.right &&
    y >= element.bounds.top &&
    y <= element.bounds.bottom
  );
}

function buildPluginCoverage(plugin: AppKnowledgePlugin | null): PluginCoverage {
  const pages = new Map<string, PageCoverage>();
  if (!plugin) {
    return { pages, actionable: 0, attempted: 0, verified: 0, coveragePercent: 0 };
  }
  const actionsByPage = new Map<string, ActionTrace[]>();
  for (const action of plugin.actions) {
    const actions = actionsByPage.get(action.fromPageId) || [];
    actions.push(action);
    actionsByPage.set(action.fromPageId, actions);
  }
  const transitionsByPage = new Map<string, PageTransition[]>();
  for (const transition of plugin.transitions) {
    const transitions = transitionsByPage.get(transition.fromPageId) || [];
    transitions.push(transition);
    transitionsByPage.set(transition.fromPageId, transitions);
  }

  let actionable = 0;
  let attempted = 0;
  let verified = 0;
  for (const page of plugin.pages) {
    const actions = actionsByPage.get(page.id) || [];
    const transitions = transitionsByPage.get(page.id) || [];
    const elements = page.elements.filter(isCoverageElement).map((element) => {
      const matchingActions = actions.filter((action) =>
        actionMatchesElement(action, page.id, element),
      );
      const matchingTransitions = transitions.filter(
        (transition) => transition.triggerKey === element.key,
      );
      const successes = matchingActions.filter((action) => action.success).length;
      return {
        element,
        attempts: matchingActions.length + matchingTransitions.length,
        successes: successes + matchingTransitions.length,
        transitions: matchingTransitions.length,
      };
    });
    const pageAttempted = elements.filter((element) => element.attempts > 0).length;
    const pageVerified = elements.filter((element) => element.successes > 0).length;
    const coverage: PageCoverage = {
      page,
      elements,
      actionable: elements.length,
      attempted: pageAttempted,
      verified: pageVerified,
      coveragePercent: elements.length
        ? Math.round((pageAttempted / elements.length) * 100)
        : 0,
      verifiedPercent: elements.length
        ? Math.round((pageVerified / elements.length) * 100)
        : 0,
      uncovered: elements
        .filter((element) => element.attempts === 0)
        .map((element) => element.element),
    };
    pages.set(page.id, coverage);
    actionable += coverage.actionable;
    attempted += coverage.attempted;
    verified += coverage.verified;
  }
  return {
    pages,
    actionable,
    attempted,
    verified,
    coveragePercent: actionable ? Math.round((attempted / actionable) * 100) : 0,
  };
}

export function KnowledgePanel({
  plugins,
  plugin,
  selectedPackage,
  selectedPageId,
  exploring,
  error,
  onSelectPackage,
  onSelectPage,
  onRefresh,
  onStartExploration,
  onStopExploration,
  onGenerateTests,
  onCompact,
  onRunTest,
  onExport,
  onImport,
}: Props) {
  const [maxSteps, setMaxSteps] = useState(30);
  const [screenshotDataUrl, setScreenshotDataUrl] = useState("");
  const [screenshotError, setScreenshotError] = useState("");
  const [selectedElement, setSelectedElement] =
    useState<KnowledgeElement | null>(null);
  const fileInput = useRef<HTMLInputElement | null>(null);
  const layout = useMemo(() => graphLayout(plugin), [plugin]);
  const selectedPage =
    plugin?.pages.find((page) => page.id === selectedPageId) || null;
  const relatedTransitions =
    plugin?.transitions.filter(
      (transition) =>
        transition.fromPageId === selectedPage?.id ||
        transition.toPageId === selectedPage?.id,
    ) || [];
  const tests = plugin?.testCases || [];
  const coverage = useMemo(() => buildPluginCoverage(plugin), [plugin]);
  const selectedCoverage = selectedPage
    ? coverage.pages.get(selectedPage.id) || null
    : null;

  useEffect(() => {
    setSelectedElement(null);
    if (!selectedPage?.screenshotPath) {
      setScreenshotDataUrl("");
      setScreenshotError("");
      return;
    }
    let disposed = false;
    api
      .readArtifact(selectedPage.screenshotPath)
      .then((dataUrl) => {
        if (!disposed) {
          setScreenshotDataUrl(dataUrl);
          setScreenshotError("");
        }
      })
      .catch((nextError) => {
        if (!disposed) {
          setScreenshotDataUrl("");
          setScreenshotError(errorMessage(nextError));
        }
      });
    return () => {
      disposed = true;
    };
  }, [selectedPage?.screenshotPath]);

  const overlayElements = selectedPage
    ? selectedPage.elements
        .filter(
          (element) =>
            element.clickable ||
            element.scrollable ||
            element.text ||
            element.contentDescription,
        )
        .slice(0, 120)
    : [];

  return (
    <div className="knowledge-panel">
      <div className="knowledge-toolbar">
        <div className="knowledge-app-select">
          <Layers3 size={15} />
          <select
            value={selectedPackage}
            onChange={(event) => onSelectPackage(event.target.value)}
            aria-label="选择 App 插件"
          >
            <option value="">选择 App 插件</option>
            {plugins.map((item) => (
              <option value={item.package} key={item.package}>
                {item.name} · {item.pages} 页面 · {item.transitions} 跳转
              </option>
            ))}
          </select>
        </div>
        <div className="knowledge-stats">
          <span title="已尝试操作 / 可操作元素">
            <ScanSearch size={13} /> {coverage.coveragePercent}% 覆盖
          </span>
          <span>
            <GitBranch size={13} /> {plugin?.transitions.length || 0} 跳转
          </span>
          <span>
            <Camera size={13} /> {plugin?.actions.length || 0} 动作
          </span>
          <span>
            <Route size={13} /> {tests.length} 用例
          </span>
        </div>
        <div className="knowledge-actions">
          <label className="explore-limit">
            <span>最多</span>
            <input
              type="number"
              min={1}
              max={300}
              value={maxSteps}
              onChange={(event) =>
                setMaxSteps(
                  Math.max(1, Math.min(300, Number(event.target.value) || 1)),
                )
              }
              disabled={exploring}
            />
            <span>步</span>
          </label>
          {exploring ? (
            <button type="button" className="knowledge-stop" onClick={onStopExploration}>
              <CircleStop size={14} />
              停止遍历
            </button>
          ) : (
            <button
              type="button"
              className="knowledge-primary"
              onClick={() => onStartExploration(maxSteps)}
              disabled={!selectedPackage}
            >
              <Play size={14} fill="currentColor" />
              自动遍历
            </button>
          )}
          <button
            type="button"
            onClick={onGenerateTests}
            disabled={!selectedPackage || !plugin}
          >
            <Sparkles size={14} />
            生成用例
          </button>
          <button type="button" onClick={onCompact} disabled={!selectedPackage || !plugin}>
            <Combine size={14} />
            合并重复页面
          </button>
          <button type="button" onClick={onRefresh} disabled={!selectedPackage}>
            <RefreshCw size={14} />
          </button>
          <button type="button" onClick={onExport} disabled={!selectedPackage}>
            <ArrowDownToLine size={14} />
            导出
          </button>
          <button type="button" onClick={() => fileInput.current?.click()}>
            <Upload size={14} />
            导入
          </button>
          <input
            ref={fileInput}
            type="file"
            accept="application/json,.json"
            hidden
            onChange={(event) => {
              const file = event.target.files?.[0];
              if (file) onImport(file);
              event.currentTarget.value = "";
            }}
          />
        </div>
      </div>

      {error ? <div className="knowledge-error">{error}</div> : null}

      {!plugin ? (
        <div className="knowledge-empty">
          <Layers3 size={28} />
          <strong>还没有 App 插件</strong>
          <span>采集一帧当前屏幕，页面、控件和跳转关系会开始沉淀到这里。</span>
        </div>
      ) : (
        <div className="knowledge-content">
          <section className="graph-column">
            <div className="knowledge-section-head">
              <div>
                <span className="eyebrow">PAGE TRANSITION MAP</span>
                <strong>{plugin.name}</strong>
              </div>
              <code>{plugin.package}</code>
            </div>
            <div className="graph-scroll">
              <svg
                className="page-graph"
                viewBox={`0 0 ${layout.width} ${layout.height}`}
                style={{ width: layout.width, height: layout.height }}
              >
                <defs>
                  <marker
                    id="arrow"
                    markerWidth="7"
                    markerHeight="7"
                    refX="6"
                    refY="3.5"
                    orient="auto"
                  >
                    <path d="M0,0 L7,3.5 L0,7 Z" fill="currentColor" />
                  </marker>
                </defs>
                {layout.edges.map(({ transition, from, to }) => {
                  const highlighted =
                    !selectedPage ||
                    transition.fromPageId === selectedPage.id ||
                    transition.toPageId === selectedPage.id;
                  const x1 = from.x + NODE_WIDTH;
                  const y1 = from.y + NODE_HEIGHT / 2;
                  const x2 = to.x;
                  const y2 = to.y + NODE_HEIGHT / 2;
                  const bend = Math.max(30, Math.abs(x2 - x1) / 2);
                  return (
                    <g
                      className={highlighted ? "graph-edge active" : "graph-edge"}
                      key={transition.id}
                    >
                      <path
                        d={`M ${x1} ${y1} C ${x1 + bend} ${y1}, ${x2 - bend} ${y2}, ${x2} ${y2}`}
                        markerEnd="url(#arrow)"
                      />
                      {highlighted && layout.edges.length <= 24 ? (
                        <text x={(x1 + x2) / 2} y={(y1 + y2) / 2 - 5}>
                          {clip(transition.triggerLabel || transition.tool, 12)}
                        </text>
                      ) : null}
                    </g>
                  );
                })}
                {layout.nodes.map(({ page, x, y }) => (
                  <g
                    key={page.id}
                    className={`graph-node ${
                      selectedPage?.id === page.id ? "is-selected" : ""
                    }`}
                    role="button"
                    tabIndex={0}
                    onClick={() => onSelectPage(page.id)}
                    onKeyDown={(event) => {
                      if (event.key === "Enter" || event.key === " ") {
                        onSelectPage(page.id);
                      }
                    }}
                  >
                    <rect x={x} y={y} width={NODE_WIDTH} height={NODE_HEIGHT} rx={10} />
                    <text className="graph-page-name" x={x + 12} y={y + 20}>
                      {clip(page.name, 17)}
                    </text>
                    <text className="graph-page-meta" x={x + 12} y={y + 38}>
                      {clip(shortActivity(page.activity), 23)}
                    </text>
                    <text className="graph-visits" x={x + NODE_WIDTH - 12} y={y + 20}>
                      ×{page.visitCount}
                    </text>
                    <text
                      className="graph-coverage"
                      x={x + NODE_WIDTH - 12}
                      y={y + 38}
                      textAnchor="end"
                    >
                      {coverage.pages.get(page.id)?.coveragePercent || 0}%
                    </text>
                  </g>
                ))}
              </svg>
            </div>
          </section>

          <section className="page-inspector">
            {selectedPage ? (
              <>
                <div className="knowledge-section-head compact">
                  <div>
                    <span className="eyebrow">SCREEN ↔ COMPONENT</span>
                    <strong>{selectedPage.name}</strong>
                  </div>
                  <span>
                    {selectedCoverage?.attempted || 0}/{selectedCoverage?.actionable || 0}{" "}
                    covered
                  </span>
                </div>
                <div className="page-preview-and-elements">
                  <div
                    className="knowledge-screen"
                    style={{
                      aspectRatio: `${selectedPage.width || 9} / ${selectedPage.height || 19.5}`,
                    }}
                  >
                    {screenshotDataUrl ? (
                      <img src={screenshotDataUrl} alt={selectedPage.name} />
                    ) : (
                      <div className="knowledge-screen-empty">
                        <ScanSearch size={18} />
                        <span>{screenshotError || "无截图证据"}</span>
                      </div>
                    )}
                    {screenshotDataUrl
                      ? overlayElements.map((element) => {
                          const width = selectedPage.width || 1;
                          const height = selectedPage.height || 1;
                          return (
                            <button
                              type="button"
                              key={element.key}
                              className={`knowledge-element-box ${
                                selectedElement?.key === element.key
                                  ? "is-selected"
                                  : ""
                              }`}
                              style={{
                                left: `${(element.bounds.left / width) * 100}%`,
                                top: `${(element.bounds.top / height) * 100}%`,
                                width: `${((element.bounds.right - element.bounds.left) / width) * 100}%`,
                                height: `${((element.bounds.bottom - element.bounds.top) / height) * 100}%`,
                              }}
                              onClick={() => setSelectedElement(element)}
                              aria-label={
                                element.text ||
                                element.contentDescription ||
                                element.resourceId
                              }
                            />
                          );
                        })
                      : null}
                  </div>
                  <div className="element-debug-list">
                    {overlayElements.slice(0, 80).map((element) => (
                      <button
                        type="button"
                        key={element.key}
                        className={
                          selectedElement?.key === element.key ? "is-active" : ""
                        }
                        onClick={() => setSelectedElement(element)}
                      >
                        <code>{element.key.slice(0, 7)}</code>
                        <span>
                          {element.text ||
                            element.contentDescription ||
                            element.resourceId ||
                            element.className.split(".").slice(-1)}
                        </span>
                        <small>{element.className.split(".").slice(-1)}</small>
                      </button>
                    ))}
                  </div>
                </div>
                <div className="page-coverage">
                  <div className="page-coverage-head">
                    <div>
                      <span className="eyebrow">ACTION COVERAGE</span>
                      <strong>{selectedCoverage?.coveragePercent || 0}%</strong>
                    </div>
                    <span>
                      已尝试 {selectedCoverage?.attempted || 0} · 已验证{" "}
                      {selectedCoverage?.verified || 0} · 共{" "}
                      {selectedCoverage?.actionable || 0}
                    </span>
                  </div>
                  <div className="coverage-bar">
                    <i
                      style={{
                        width: `${selectedCoverage?.coveragePercent || 0}%`,
                      }}
                    />
                  </div>
                  <div className="uncovered-actions">
                    <span>
                      UNEXPLORED · {selectedCoverage?.uncovered.length || 0}
                    </span>
                    <div>
                      {(selectedCoverage?.uncovered || []).slice(0, 12).map((element) => (
                        <button
                          type="button"
                          key={element.key}
                          onClick={() => setSelectedElement(element)}
                          title={element.key}
                        >
                          {clip(coverageElementLabel(element), 14)}
                        </button>
                      ))}
                      {(selectedCoverage?.uncovered.length || 0) === 0 ? (
                        <small>当前页可操作元素均已尝试</small>
                      ) : null}
                    </div>
                  </div>
                </div>
                {selectedElement && screenshotDataUrl ? (
                  <div className="component-crop-row">
                    <span className="eyebrow">COMPONENT CROP</span>
                    <div
                      className="component-crop"
                      style={componentCropStyle(
                        selectedPage,
                        selectedElement,
                        screenshotDataUrl,
                      )}
                      title={`${selectedElement.className} ${JSON.stringify(
                        selectedElement.bounds,
                      )}`}
                    />
                    <span>
                      {selectedElement.bounds.right - selectedElement.bounds.left}×
                      {selectedElement.bounds.bottom - selectedElement.bounds.top} px
                    </span>
                  </div>
                ) : null}
                <div className="page-debug-meta">
                  {selectedElement ? (
                    <>
                      <code>{selectedElement.resourceId || "no resource-id"}</code>
                      <span>
                        [{selectedElement.bounds.left},{selectedElement.bounds.top}][
                        {selectedElement.bounds.right},{selectedElement.bounds.bottom}]
                      </span>
                    </>
                  ) : (
                    <>
                      <code>{selectedPage.id}</code>
                      <span>{relatedTransitions.length} related transitions</span>
                    </>
                  )}
                </div>
              </>
            ) : (
              <div className="knowledge-empty inline">
                <GitBranch size={22} />
                <span>点击左侧页面节点查看截图、控件与跳转证据。</span>
              </div>
            )}
          </section>

          <section className="test-column">
            <div className="knowledge-section-head compact">
              <div>
                <span className="eyebrow">GENERATED JOURNEYS</span>
                <strong>页面导航用例</strong>
              </div>
              <span>{tests.length}</span>
            </div>
            <div className="generated-test-list">
              {tests.length === 0 ? (
                <div className="knowledge-empty inline">
                  <Route size={22} />
                  <span>积累跳转关系后，可自动生成从首页到各页面的测试路径。</span>
                </div>
              ) : (
                tests.map((testCase) => (
                  <article
                    key={testCase.id}
                    className="generated-test"
                    title={JSON.stringify(testCase.steps, null, 2)}
                  >
                    <div className="generated-test-head">
                      <span className={`test-state ${testCase.status}`}>
                        {testStatus(testCase.status)}
                      </span>
                      <button
                        type="button"
                        onClick={() => onRunTest(testCase.id)}
                        disabled={testCase.status === "running"}
                      >
                        <Play size={10} fill="currentColor" />
                        运行
                      </button>
                    </div>
                    <button
                      type="button"
                      className="generated-test-body"
                      onClick={() => onSelectPage(testCase.targetPageId)}
                    >
                      <strong>{testCase.name}</strong>
                      <small>
                        {testCase.steps.length} steps ·{" "}
                        {testCase.steps.map((step) => step.label).join(" / ")}
                      </small>
                    </button>
                    {testCase.lastResult ? (
                      <small className="test-result">{testCase.lastResult}</small>
                    ) : null}
                  </article>
                ))
              )}
            </div>
            <div className="plugin-file-meta">
              <FileJson size={13} />
              <span>{plugin.testCases.length} tests in plugin</span>
              <span>{plugin.pages.length} pages</span>
            </div>
          </section>
        </div>
      )}
    </div>
  );
}

function componentCropStyle(
  page: KnowledgePage,
  element: KnowledgeElement,
  screenshotDataUrl: string,
) {
  const pageWidth = page.width || 1;
  const pageHeight = page.height || 1;
  const elementWidth = Math.max(1, element.bounds.right - element.bounds.left);
  const elementHeight = Math.max(1, element.bounds.bottom - element.bounds.top);
  const scale = Math.min(150 / elementWidth, 100 / elementHeight, 4);
  return {
    width: `${Math.max(24, elementWidth * scale)}px`,
    height: `${Math.max(20, elementHeight * scale)}px`,
    backgroundImage: `url(${screenshotDataUrl})`,
    backgroundSize: `${pageWidth * scale}px ${pageHeight * scale}px`,
    backgroundPosition: `-${element.bounds.left * scale}px -${
      element.bounds.top * scale
    }px`,
    backgroundRepeat: "no-repeat",
  };
}
