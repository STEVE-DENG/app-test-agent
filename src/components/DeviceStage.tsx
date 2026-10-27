import {
  Camera,
  Crosshair,
  MonitorUp,
  MoveUpRight,
  ScanSearch,
  Smartphone,
  TerminalSquare,
} from "lucide-react";
import type { DeviceInfo, Observation, UiNode } from "../types";

type Props = {
  device: DeviceInfo | null;
  observation: Observation | null;
  selectedNode: UiNode | null;
  loading: boolean;
  error: string;
  onCapture: () => void;
  onOpenMirror: () => void;
  onSelectNode: (node: UiNode) => void;
};

function shortComponent(component: string) {
  const pieces = component.split(".");
  return pieces.length > 2 ? pieces.slice(-2).join(".") : component;
}

export function DeviceStage({
  device,
  observation,
  selectedNode,
  loading,
  error,
  onCapture,
  onOpenMirror,
  onSelectNode,
}: Props) {
  const overlayNodes = observation
    ? observation.nodes
        .filter(
          (node) =>
            node.visible &&
            node.enabled &&
            (node.clickable || node.text || node.contentDescription),
        )
        .slice(0, 110)
    : [];
  const viewportWidth =
    observation?.width ||
    Math.max(1, ...overlayNodes.map((node) => node.bounds.right));
  const viewportHeight =
    observation?.height ||
    Math.max(1, ...overlayNodes.map((node) => node.bounds.bottom));

  return (
    <section className="device-stage">
      <div className="stage-heading">
        <div>
          <span className="eyebrow">LIVE SURFACE</span>
          <h2>混合观察画布</h2>
        </div>
        <div className="stage-actions">
          <div className="signal-cluster">
            <span className={observation ? "signal live" : "signal"}>
              <i />
              {observation ? "VISION" : "NO FRAME"}
            </span>
            <span className={observation ? "signal live teal" : "signal"}>
              <i />
              {observation ? `${observation.nodes.length} NODES` : "UI TREE"}
            </span>
            <span className={observation?.ocr ? "signal live teal" : "signal"}>
              <i />
              {observation?.ocr
                ? `${observation.ocr.blocks.length} OCR`
                : observation?.ocrError
                  ? "OCR OFF"
                  : "OCR"}
            </span>
          </div>
          <button
            className="secondary-button compact"
            type="button"
            onClick={onCapture}
            disabled={!device?.ready || loading}
          >
            <Camera size={15} />
            {loading ? "采集..." : "采集一帧"}
          </button>
          <button
            className="icon-button"
            type="button"
            onClick={onOpenMirror}
            disabled={!device?.ready}
            title="打开 scrcpy"
            aria-label="打开 scrcpy"
          >
            <MonitorUp size={16} />
          </button>
        </div>
      </div>

      {error ? (
        <div className="stage-error">
          <TerminalSquare size={16} />
          <span>{error}</span>
        </div>
      ) : null}

      <div className="stage-canvas">
        {!device ? (
          <div className="stage-empty">
            <div className="empty-orbit">
              <span />
              <Smartphone size={34} strokeWidth={1.35} />
            </div>
            <span className="eyebrow">WAITING FOR DEVICE</span>
            <h3>连接一台 Android 设备</h3>
            <p>
              开启开发者选项和 USB 调试。设备出现在左栏后，这里会同时呈现截图与
              accessibility 节点。
            </p>
            <div className="command-card">
              <code>adb devices</code>
              <span>USB / Wireless ADB</span>
            </div>
          </div>
        ) : observation ? (
          <div className="phone-workbench">
            <div
              className="phone-shell"
              style={{ aspectRatio: `${viewportWidth} / ${viewportHeight}` }}
            >
              <div className="phone-camera" />
              <img
                className="device-screen"
                src={observation.screenshotDataUrl}
                alt={`Android screen from ${device.serial}`}
                draggable={false}
              />
              <div className="node-overlay">
                {overlayNodes.map((node) => {
                  const left = (node.bounds.left / viewportWidth) * 100;
                  const top = (node.bounds.top / viewportHeight) * 100;
                  const width =
                    ((node.bounds.right - node.bounds.left) / viewportWidth) * 100;
                  const height =
                    ((node.bounds.bottom - node.bounds.top) / viewportHeight) * 100;
                  const active = selectedNode?.index === node.index;
                  return (
                    <button
                      key={`${node.index}-${node.bounds.left}-${node.bounds.top}`}
                      type="button"
                      className={`node-box ${active ? "is-selected" : ""} ${
                        node.clickable ? "is-clickable" : ""
                      }`}
                      style={{
                        left: `${left}%`,
                        top: `${top}%`,
                        width: `${width}%`,
                        height: `${height}%`,
                      }}
                      onClick={() => onSelectNode(node)}
                      aria-label={node.text || node.contentDescription || "UI node"}
                    >
                      <span className="node-index">{node.index}</span>
                    </button>
                  );
                })}
              </div>
              {observation.ocr?.blocks.length ? (
                <div className="ocr-overlay stage-ocr-overlay">
                  {observation.ocr.blocks.slice(0, 120).map((block, index) => {
                    const left = (block.bounds.left / viewportWidth) * 100;
                    const top = (block.bounds.top / viewportHeight) * 100;
                    const width =
                      ((block.bounds.right - block.bounds.left) / viewportWidth) * 100;
                    const height =
                      ((block.bounds.bottom - block.bounds.top) / viewportHeight) * 100;
                    const matched = observation.nodes.find(
                      (node) => node.index === block.matchedNodeIndex,
                    );
                    return (
                      <button
                        key={`${block.text}-${index}`}
                        type="button"
                        className={matched ? "is-linked" : "is-orphan"}
                        style={{
                          left: `${left}%`,
                          top: `${top}%`,
                          width: `${width}%`,
                          height: `${height}%`,
                        }}
                        onClick={() => matched && onSelectNode(matched)}
                        title={`OCR: ${block.text} · ${(block.confidence * 100).toFixed(0)}%`}
                        aria-label={`OCR ${block.text}`}
                      />
                    );
                  })}
                </div>
              ) : null}
              <div className="scan-wash" />
            </div>

            <div className="stage-floating-meta">
              <div>
                <Crosshair size={14} />
                <span>{shortComponent(observation.package) || "Unknown app"}</span>
              </div>
              <div>
                <ScanSearch size={14} />
                <span>{observation.pageSignature.slice(0, 12)}</span>
              </div>
            </div>
          </div>
        ) : (
          <div className="stage-empty ready">
            <div className="empty-orbit teal">
              <span />
              <MoveUpRight size={31} strokeWidth={1.4} />
            </div>
            <span className="eyebrow">DEVICE READY</span>
            <h3>{device.model || "Android Device"}</h3>
            <p>
              先采集当前屏幕，获取截图和 UI 树。你可以在画布中点击节点检查
              locator，也可以直接启动一句话任务。
            </p>
            <button
              className="primary-button"
              type="button"
              onClick={onCapture}
              disabled={loading}
            >
              <Camera size={16} />
              采集当前屏幕
            </button>
          </div>
        )}
      </div>

      <div className="node-inspector-strip">
        {selectedNode ? (
          <>
            <div className="inspector-primary">
              <span className="eyebrow">SELECTED NODE</span>
              <strong>
                {selectedNode.text ||
                  selectedNode.contentDescription ||
                  selectedNode.className.split(".").slice(-1)}
              </strong>
            </div>
            <div className="inspector-field">
              <span>RESOURCE ID</span>
              <code>{selectedNode.resourceId || "—"}</code>
            </div>
            <div className="inspector-field">
              <span>BOUNDS</span>
              <code>
                [{selectedNode.bounds.left},{selectedNode.bounds.top}][
                {selectedNode.bounds.right},{selectedNode.bounds.bottom}]
              </code>
            </div>
            <div className="inspector-field compact-field">
              <span>STATE</span>
              <code>
                {selectedNode.clickable ? "clickable" : "static"} ·{" "}
                {selectedNode.enabled ? "enabled" : "disabled"}
              </code>
            </div>
          </>
        ) : (
          <div className="inspector-placeholder">
            <Crosshair size={15} />
            <span>点击截图中的节点，查看文本、resource-id 与 bounds。</span>
          </div>
        )}
      </div>
    </section>
  );
}
