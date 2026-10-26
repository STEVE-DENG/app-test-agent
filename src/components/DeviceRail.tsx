import {
  ArrowUpRight,
  CircleAlert,
  Layers3,
  MonitorUp,
  RefreshCw,
  Settings2,
  Smartphone,
  Sparkles,
} from "lucide-react";
import type { DeviceInfo } from "../types";

type Props = {
  devices: DeviceInfo[];
  selectedSerial: string | null;
  loading: boolean;
  onSelect: (serial: string) => void;
  onRefresh: () => void;
  onOpenMirror: () => void;
  onOpenSettings: () => void;
};

function statusLabel(state: string) {
  if (state === "device") return "已连接";
  if (state === "unauthorized") return "待授权";
  if (state === "offline") return "离线";
  return state;
}

export function DeviceRail({
  devices,
  selectedSerial,
  loading,
  onSelect,
  onRefresh,
  onOpenMirror,
  onOpenSettings,
}: Props) {
  const selectedDevice = devices.find((device) => device.serial === selectedSerial);

  return (
    <aside className="device-rail">
      <div className="rail-heading">
        <div>
          <span className="eyebrow">DEVICE BAY</span>
          <h2>设备工作台</h2>
        </div>
        <button
          className="icon-button"
          type="button"
          onClick={onRefresh}
          aria-label="刷新设备"
          title="刷新设备"
          disabled={loading}
        >
          <RefreshCw size={16} className={loading ? "spin" : ""} />
        </button>
      </div>

      <div className="device-list">
        {devices.length === 0 && !loading ? (
          <div className="rail-empty">
            <Smartphone size={25} strokeWidth={1.6} />
            <strong>没有检测到设备</strong>
            <p>连接 Android 手机，开启 USB 调试并完成授权。</p>
            <code>adb devices</code>
          </div>
        ) : null}

        {devices.map((device) => {
          const active = device.serial === selectedSerial;
          return (
            <button
              type="button"
              key={device.serial}
              className={`device-card ${active ? "is-active" : ""}`}
              onClick={() => onSelect(device.serial)}
            >
              <span className="device-card-topline">
                <span className={`device-state ${device.ready ? "ready" : "warn"}`}>
                  <i />
                  {statusLabel(device.state)}
                </span>
                <span className="device-serial">{device.serial.slice(-8)}</span>
              </span>
              <span className="device-model">
                {device.manufacturer} {device.model || "Android Device"}
              </span>
              <span className="device-meta">
                Android {device.androidVersion || "?"}
                {device.resolution ? ` · ${device.resolution}` : ""}
              </span>
              {device.foregroundApp ? (
                <span className="device-app">{device.foregroundApp}</span>
              ) : null}
            </button>
          );
        })}
      </div>

      <div className="rail-actions">
        <button
          type="button"
          className="rail-action"
          onClick={onOpenMirror}
          disabled={!selectedDevice?.ready}
        >
          <MonitorUp size={16} />
          打开 scrcpy
          <ArrowUpRight size={14} className="action-tail" />
        </button>
        <button type="button" className="rail-action" onClick={onOpenSettings}>
          <Settings2 size={16} />
          模型与运行设置
          <ArrowUpRight size={14} className="action-tail" />
        </button>
      </div>

      <div className="pipeline-card">
        <div className="pipeline-title">
          <Sparkles size={15} />
          Hybrid loop
        </div>
        <div className="pipeline-row">
          <span className="pipeline-dot vision" />
          <span>Vision + UI tree</span>
          <b>ON</b>
        </div>
        <div className="pipeline-row">
          <span className="pipeline-dot react" />
          <span>ReAct tools</span>
          <b>12</b>
        </div>
        <div className="pipeline-row">
          <span className="pipeline-dot skill" />
          <span>Skill recall</span>
          <b>2 max</b>
        </div>
      </div>

      {selectedDevice && !selectedDevice.ready ? (
        <div className="rail-warning">
          <CircleAlert size={16} />
          <span>请在手机上允许 USB 调试授权后重新刷新。</span>
        </div>
      ) : null}

      <div className="rail-footnote">
        <Layers3 size={13} />
        <span>Trace-first · local evidence</span>
      </div>
    </aside>
  );
}
