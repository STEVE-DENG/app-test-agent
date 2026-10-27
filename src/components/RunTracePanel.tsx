import {
  BrainCircuit,
  Camera,
  ChevronRight,
  Clipboard,
  Code2,
  Diff,
  ExternalLink,
  Filter,
  Timer,
} from "lucide-react";
import { useEffect, useMemo, useState } from "react";
import { api, errorMessage } from "../lib/api";
import type {
  ModelTraceRecord,
  RunRecord,
  StepMetricRecord,
  ToolRecord,
} from "../types";

type Props = {
  run: RunRecord;
  steps: ToolRecord[];
  metrics: StepMetricRecord[];
  modelTraces: ModelTraceRecord[];
  onOpenDebug: () => void;
  onOpenPage: (pageId: string) => void;
};

function formatMs(milliseconds: number) {
  if (milliseconds < 1000) return `${milliseconds} ms`;
  return `${(milliseconds / 1000).toFixed(1)} s`;
}

function shortSignature(value: string) {
  return value ? value.slice(0, 10) : "—";
}

export function RunTracePanel({
  run,
  steps,
  metrics,
  modelTraces,
  onOpenDebug,
  onOpenPage,
}: Props) {
  const [filter, setFilter] = useState("");
  const [selectedStep, setSelectedStep] = useState<number | null>(
    steps[0]?.stepIndex ?? null,
  );
  const [beforeImage, setBeforeImage] = useState("");
  const [afterImage, setAfterImage] = useState("");
  const [imageError, setImageError] = useState("");
  const [copied, setCopied] = useState(false);
  const [showModelResponse, setShowModelResponse] = useState(false);

  const metricByStep = useMemo(
    () => new Map(metrics.map((metric) => [metric.stepIndex, metric])),
    [metrics],
  );
  const modelTraceByStep = useMemo(
    () => new Map(modelTraces.map((trace) => [trace.stepIndex, trace])),
    [modelTraces],
  );
  const selected =
    steps.find((step) => step.stepIndex === selectedStep) || steps[0] || null;
  const selectedModelTrace = selected
    ? modelTraceByStep.get(selected.stepIndex) || null
    : null;
  const visibleSteps = useMemo(() => {
    const query = filter.trim().toLowerCase();
    if (!query) return steps;
    return steps.filter((step) =>
      [step.tool, step.thought, JSON.stringify(step.args), JSON.stringify(step.result)]
        .join(" ")
        .toLowerCase()
        .includes(query),
    );
  }, [filter, steps]);

  useEffect(() => {
    if (selectedStep === null && steps[0]) {
      setSelectedStep(steps[0].stepIndex);
    }
  }, [selectedStep, steps]);

  useEffect(() => {
    let disposed = false;
    setBeforeImage("");
    setAfterImage("");
    setImageError("");
    setShowModelResponse(false);
    if (!selected) return;
    const load = async () => {
      try {
        if (selected.screenshotPath) {
          const value = await api.readArtifact(selected.screenshotPath);
          if (!disposed) setBeforeImage(value);
        }
        if (selected.screenshotAfterPath) {
          const value = await api.readArtifact(selected.screenshotAfterPath);
          if (!disposed) setAfterImage(value);
        }
      } catch (error) {
        if (!disposed) setImageError(errorMessage(error));
      }
    };
    void load();
    return () => {
      disposed = true;
    };
  }, [selected?.stepIndex, selected?.screenshotPath, selected?.screenshotAfterPath]);

  const copyTrace = async () => {
    if (!selected) return;
    try {
      await navigator.clipboard.writeText(
        JSON.stringify(
          {
            run,
            step: selected,
            metric: metricByStep.get(selected.stepIndex) || null,
            modelTrace: modelTraceByStep.get(selected.stepIndex) || null,
          },
          null,
          2,
        ),
      );
      setCopied(true);
      window.setTimeout(() => setCopied(false), 1500);
    } catch {
      setCopied(false);
    }
  };

  return (
    <div className="run-trace-panel">
      <div className="trace-head">
        <div>
          <span className="eyebrow">RUN TRACE · {run.id.slice(0, 8)}</span>
          <h3>{run.instruction}</h3>
          <p>{run.summary || "没有结果摘要"}</p>
        </div>
        <div className="trace-actions">
          <button type="button" onClick={copyTrace} disabled={!selected}>
            <Clipboard size={13} />
            {copied ? "已复制" : "复制步骤"}
          </button>
          <button type="button" onClick={onOpenDebug}>
            <ExternalLink size={13} />
            打开调试台
          </button>
        </div>
      </div>

      <div className="trace-toolbar">
        <label>
          <Filter size={13} />
          <input
            value={filter}
            onChange={(event) => setFilter(event.target.value)}
            placeholder="筛选工具、思考、参数或结果"
          />
        </label>
        <span>
          {visibleSteps.length}/{steps.length} steps · {run.model}
        </span>
      </div>

      <div className="trace-layout">
        <div className="trace-step-list">
          {visibleSteps.length === 0 ? (
            <div className="trace-empty">没有匹配的步骤</div>
          ) : (
            visibleSteps.map((step) => {
              const metric = metricByStep.get(step.stepIndex);
              return (
                <button
                  type="button"
                  key={`${step.stepIndex}-${step.tool}`}
                  className={selected?.stepIndex === step.stepIndex ? "is-active" : ""}
                  onClick={() => setSelectedStep(step.stepIndex)}
                >
                  <span className="trace-step-index">{step.stepIndex}</span>
                  <span className="trace-step-main">
                    <strong>{step.tool}</strong>
                    <small>{step.thought || JSON.stringify(step.args)}</small>
                    <small>
                      {shortSignature(step.pageIdBefore)} <ChevronRight size={10} />{" "}
                      {shortSignature(step.pageIdAfter)}
                      {metric ? ` · ${formatMs(metric.totalMs)}` : ""}
                    </small>
                  </span>
                  {step.pageIdBefore !== step.pageIdAfter ? (
                    <Diff size={13} className="trace-diff" />
                  ) : null}
                </button>
              );
            })
          )}
        </div>

        <div className="trace-detail">
          {selected ? (
            <>
              <div className="trace-detail-head">
                <div>
                  <span className="eyebrow">STEP {selected.stepIndex}</span>
                  <h4>{selected.tool}</h4>
                </div>
                <code>{selected.createdAt}</code>
              </div>
              <div className="trace-facts">
                <span>
                  <Timer size={12} />
                  page {shortSignature(selected.pageIdBefore)} →{" "}
                  {shortSignature(selected.pageIdAfter)}
                </span>
                {metricByStep.get(selected.stepIndex) ? (
                  <span>
                    <Timer size={12} />
                    {metricByStep.get(selected.stepIndex)?.totalMs} ms · model{" "}
                    {metricByStep.get(selected.stepIndex)?.modelMs} ms · tool{" "}
                    {metricByStep.get(selected.stepIndex)?.toolMs} ms
                  </span>
                ) : null}
                {selected.pageIdBefore ? (
                  <button
                    type="button"
                    onClick={() => onOpenPage(selected.pageIdBefore)}
                  >
                    打开 before 页面
                  </button>
                ) : null}
                {selected.pageIdAfter &&
                selected.pageIdAfter !== selected.pageIdBefore ? (
                  <button
                    type="button"
                    onClick={() => onOpenPage(selected.pageIdAfter)}
                  >
                    打开 after 页面
                  </button>
                ) : null}
              </div>
              <div className="trace-json-grid">
                <div>
                  <span>
                    <Code2 size={12} /> ARGS
                  </span>
                  <pre>{JSON.stringify(selected.args, null, 2)}</pre>
                </div>
                <div>
                  <span>
                    <Code2 size={12} /> RESULT
                  </span>
                  <pre>{JSON.stringify(selected.result, null, 2)}</pre>
                </div>
              </div>
              <div className="trace-evidence">
                <div>
                  <span>
                    <Camera size={12} /> BEFORE
                  </span>
                  {beforeImage ? (
                    <img src={beforeImage} alt="step before" />
                  ) : (
                    <div className="trace-image-empty">没有 before 证据</div>
                  )}
                </div>
                <div>
                  <span>
                    <Camera size={12} /> AFTER
                  </span>
                  {afterImage ? (
                    <img src={afterImage} alt="step after" />
                  ) : (
                    <div className="trace-image-empty">没有 after 证据</div>
                  )}
                </div>
              </div>
              {imageError ? <div className="trace-image-error">{imageError}</div> : null}
              <div className="trace-thought">{selected.thought || "没有思考摘要"}</div>
              <div className="model-trace">
                <div className="model-trace-head">
                  <span>
                    <BrainCircuit size={13} /> MODEL TRACE
                  </span>
                  {selectedModelTrace ? (
                    <button
                      type="button"
                      onClick={() => setShowModelResponse((current) => !current)}
                    >
                      {showModelResponse ? "收起响应" : "查看原始响应"}
                    </button>
                  ) : null}
                </div>
                {selectedModelTrace ? (
                  <>
                    <div className="model-trace-facts">
                      <span>{selectedModelTrace.model}</span>
                      <span>{selectedModelTrace.useVision ? "视觉" : "文本"}输入</span>
                      <span>prompt {selectedModelTrace.promptChars} chars</span>
                      <span>{selectedModelTrace.nodeCount} nodes</span>
                      <span>{selectedModelTrace.attemptCount} attempts</span>
                      <span>{formatMs(selectedModelTrace.durationMs)}</span>
                    </div>
                    {selectedModelTrace.usage ? (
                      <code className="model-usage">
                        usage {JSON.stringify(selectedModelTrace.usage)}
                      </code>
                    ) : null}
                    {showModelResponse ? (
                      <pre className="model-response">
                        {selectedModelTrace.rawResponse}
                      </pre>
                    ) : null}
                  </>
                ) : (
                  <span className="model-trace-empty">当前步骤没有模型轨迹</span>
                )}
              </div>
            </>
          ) : (
            <div className="trace-empty">选择一个步骤查看参数、结果和前后截图</div>
          )}
        </div>
      </div>
    </div>
  );
}

