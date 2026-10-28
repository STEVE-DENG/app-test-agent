import {
  Eye,
  EyeOff,
  KeyRound,
  PlugZap,
  Save,
  SlidersHorizontal,
  X,
} from "lucide-react";
import { useEffect, useState } from "react";
import type { SaveSettingsInput, SettingsView } from "../types";

type Props = {
  open: boolean;
  settings: SettingsView | null;
  saving: boolean;
  testing: boolean;
  testResult: string;
  error: string;
  onClose: () => void;
  onSave: (input: SaveSettingsInput) => void;
  onTest: (input: SaveSettingsInput) => void;
};

export function SettingsDialog({
  open,
  settings,
  saving,
  testing,
  testResult,
  error,
  onClose,
  onSave,
  onTest,
}: Props) {
  const [baseUrl, setBaseUrl] = useState(settings?.baseUrl || "");
  const [model, setModel] = useState(settings?.model || "");
  const [apiKey, setApiKey] = useState("");
  const [showKey, setShowKey] = useState(false);
  const [useVision, setUseVision] = useState(settings?.useVision ?? true);
  const [temperature, setTemperature] = useState(settings?.temperature ?? 0.2);
  const [maxSteps, setMaxSteps] = useState(settings?.maxSteps ?? 24);

  useEffect(() => {
    if (!settings || !open) return;
    setBaseUrl(settings.baseUrl);
    setModel(settings.model);
    setApiKey("");
    setUseVision(settings.useVision);
    setTemperature(settings.temperature);
    setMaxSteps(settings.maxSteps);
  }, [settings, open]);

  useEffect(() => {
    if (!open) return;
    const handleKey = (event: KeyboardEvent) => {
      if (event.key === "Escape") onClose();
    };
    window.addEventListener("keydown", handleKey);
    return () => window.removeEventListener("keydown", handleKey);
  }, [open, onClose]);

  if (!open) return null;

  const input: SaveSettingsInput = {
    baseUrl,
    apiKey: apiKey.trim() || undefined,
    model,
    useVision,
    temperature,
    maxSteps,
  };
  const canSubmit = Boolean(baseUrl.trim() && model.trim());
  const canTest = canSubmit && Boolean(apiKey.trim() || settings?.hasApiKey);

  return (
    <div className="dialog-backdrop" role="presentation" onMouseDown={onClose}>
      <section
        className="settings-dialog"
        role="dialog"
        aria-modal="true"
        aria-labelledby="settings-title"
        onMouseDown={(event) => event.stopPropagation()}
      >
        <header>
          <div>
            <span className="eyebrow">MODEL ROUTER</span>
            <h2 id="settings-title">模型与运行设置</h2>
          </div>
          <button
            type="button"
            className="icon-button"
            onClick={onClose}
            aria-label="关闭设置"
          >
            <X size={17} />
          </button>
        </header>

        <div className="settings-body">
          <div className="settings-callout">
            <SlidersHorizontal size={17} />
            <span>
              Agent 会同时发送压缩后的 UI 树与截图。支持任意 OpenAI-compatible
              多模态 Chat Completions 服务，包括 DeepSeek V4.1 视觉端点。
            </span>
          </div>

          <label className="field">
            <span>Base URL</span>
            <input
              type="url"
              value={baseUrl}
              onChange={(event) => setBaseUrl(event.target.value)}
              placeholder="https://api.example.com/v1"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
            />
            <small>应用会自动拼接 `/chat/completions`。</small>
          </label>

          <label className="field">
            <span>Model</span>
            <input
              type="text"
              value={model}
              onChange={(event) => setModel(event.target.value)}
              placeholder="opencode-go/deepseek-v4.1-flash"
              autoCapitalize="off"
              autoCorrect="off"
              spellCheck={false}
            />
          </label>

          <label className="field">
            <span>
              API Key
              {settings?.hasApiKey ? (
                <em>
                  <KeyRound size={12} />
                  已配置 {settings.apiKeyPreview}
                </em>
              ) : null}
            </span>
            <div className="secret-input">
              <input
                type={showKey ? "text" : "password"}
                value={apiKey}
                onChange={(event) => setApiKey(event.target.value)}
                placeholder={
                  settings?.hasApiKey ? "留空则保留已有 Key" : "输入模型服务 API Key"
                }
                autoCapitalize="off"
                autoCorrect="off"
                spellCheck={false}
              />
              <button
                type="button"
                onClick={() => setShowKey((value) => !value)}
                aria-label={showKey ? "隐藏 Key" : "显示 Key"}
              >
                {showKey ? <EyeOff size={15} /> : <Eye size={15} />}
              </button>
            </div>
          </label>

          <div className="settings-grid">
            <label className="field">
              <span>Temperature</span>
              <div className="range-field">
                <input
                  type="range"
                  min="0"
                  max="1"
                  step="0.05"
                  value={temperature}
                  onChange={(event) => setTemperature(Number(event.target.value))}
                />
                <code>{temperature.toFixed(2)}</code>
              </div>
            </label>
            <label className="field">
              <span>Max steps</span>
              <input
                type="number"
                min="4"
                max="60"
                value={maxSteps}
                onChange={(event) => setMaxSteps(Number(event.target.value))}
              />
            </label>
          </div>

          <label className="switch-row">
            <span>
              <strong>多模态视觉输入</strong>
              <small>
                多模态模型建议保持开启；关闭后只使用 UI 树、OCR 和文本上下文。
              </small>
            </span>
            <input
              type="checkbox"
              checked={useVision}
              onChange={(event) => setUseVision(event.target.checked)}
            />
            <i className={useVision ? "on" : ""} />
          </label>

          {error ? <div className="settings-error">{error}</div> : null}
          {testResult ? <div className="settings-success">{testResult}</div> : null}
        </div>

        <footer>
          <span>API Key 保存在应用数据目录，文件权限为 0600。</span>
          <div>
            <button
              type="button"
              className="secondary-button"
              disabled={testing || !canTest}
              onClick={() => onTest(input)}
            >
              <PlugZap size={15} />
              {testing ? "检查中..." : "测试连接"}
            </button>
            <button type="button" className="secondary-button" onClick={onClose}>
              取消
            </button>
            <button
              type="button"
              className="primary-button"
              disabled={saving || !canSubmit}
              onClick={() => onSave(input)}
            >
              <Save size={15} />
              {saving ? "保存中..." : "保存配置"}
            </button>
          </div>
        </footer>
      </section>
    </div>
  );
}
