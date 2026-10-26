# AI App Test Agent

基于 Tauri 2、React 和 Rust 的 Android AI 自动化工作台。

核心能力：

- ADB 设备发现、截图和 accessibility/UI 树采集。
- 截图 + UI 树 + 多模态模型混合感知。
- Tool Use/ReAct 单步执行循环。
- SQLite 任务轨迹和证据记录。
- 成功轨迹编译为可审核 Skill。
- 已批准 Skill 在相似任务中优先召回。

首版功能范围见 [docs/product-spec.md](docs/product-spec.md)。

## 开发

```bash
npm install
npm run tauri dev
```

Android 设备需要开启 USB 调试并完成 `adb devices` 授权。

## 配置

在应用设置中填写 OpenAI-compatible 多模态模型：

- Base URL，例如 `https://api.example.com/v1`
- API Key
- Model

首版 API Key 保存在应用数据目录的受限权限设置文件中。迁移到 macOS Keychain 列在后续安全项。
