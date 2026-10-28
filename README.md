# AI App Test Agent

基于 Tauri 2、React 和 Rust 的 Android AI 自动化工作台。

核心能力：

- Codex 式持续对话流：同一会话可连续追加指令，刷新或重启后从 SQLite 恢复上下文。
- 会话上下文自动带入模型，支持“继续”“刚才那个页面”等指代。
- ADB 设备发现、截图和 accessibility/UI 树采集。
- 截图 + UI 树 + 模型视觉 + Apple Vision OCR 混合感知。
- OCR 文本与控件节点关联；UI 树定位失败时自动使用 OCR 坐标兜底。
- 原图用于调试，900px JPEG 用于模型输入，减少视觉 token 和上传延迟。
- Tool Use/ReAct 单步执行循环。
- SQLite 任务轨迹和证据记录。
- 成功轨迹编译为可审核 Skill。
- 已批准 Skill 在相似任务中优先召回。
- 页面图、控件截图和跳转关系沉淀为 App 插件。
- 动态页面聚类与重复页面合并，避免页面图被同一首页/Flutter 容器撑爆。
- 自动遍历与生成导航测试用例。
- 有界持续学习 Loop：有增量继续，连续无增长自动停止。
- 调试台：原始 UI XML、截图、OCR 框、模型输入图、节点、页面签名、设备诊断、
  Logcat、模型 Prompt/响应、步骤前后证据、耗时统计和完整调试包导出。

首版功能范围见 [docs/product-spec.md](docs/product-spec.md)。
长期 Loop 提示词见 [docs/long-run-prompt.md](docs/long-run-prompt.md)。

## 开发

```bash
npm install
npm run tauri dev
```

Android 设备需要开启 USB 调试并完成 `adb devices` 授权。

## 无头持续学习

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin agent-learn -- \
  "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" \
  <device-serial> com.taobao.idlefish 8 45 2
```

参数依次是数据目录、设备 serial、包名、最大轮数、每轮最大步数、连续无增长停止轮数。

单次调试快照：

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin agent-debug -- \
  "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" \
  <device-serial> com.taobao.idlefish
```

终端快速摘要（不输出 2-3MB base64 截图）：

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin agent-debug -- \
  --summary \
  "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" \
  <device-serial> com.taobao.idlefish
```

合并插件中的重复页面：

```bash
cargo run --manifest-path src-tauri/Cargo.toml --bin agent-compact -- \
  "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" \
  com.taobao.idlefish
```

## 调试入口

- 底部 `调试台`：采集截图/UI XML/OCR，查看 OCR 文本块与坐标、模型实际输入图、页面签名、可操作节点、原始 XML、证据文件和耗时。
- `读取 Logcat`：获取最近 400 行设备日志；完整调试包也会附带 logcat 尾部。
- `导出调试包`：导出当前快照、设备诊断、App 插件、Skill、最近任务、步骤耗时和模型轨迹。
- `页面图谱 -> 合并重复页面`：按 Activity + 稳定元素相似度聚类动态页面。
- `页面图谱 -> 自动遍历`：有界探索并更新闲鱼插件。
- `调试台 -> 启动长期 Loop`：多轮持续学习，可用于积累闲鱼全部可见操作能力。

## 配置

在应用设置中填写 OpenAI-compatible 多模态模型：

- Base URL，例如 `https://api.example.com/v1`
- API Key
- Model

默认配置面向 `opencode-go/deepseek-v4.1-flash`，按多模态视觉模型使用。

## 连续会话 CLI

```bash
# 第一轮会创建会话，并在输出的 conversationId 字段返回会话 ID
cargo run --manifest-path src-tauri/Cargo.toml --bin agent-task -- \
  "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" \
  <device-serial> -- "返回闲鱼首页"

# 后续轮次复用同一 conversation ID
cargo run --manifest-path src-tauri/Cargo.toml --bin agent-task -- \
  "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" \
  <device-serial> <conversation-id> -- "现在页面上有哪些可操作控件？"
```

首版 API Key 保存在应用数据目录的受限权限设置文件中。迁移到 macOS Keychain 列在后续安全项。
