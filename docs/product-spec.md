# AI App Test Agent - 首版功能规格

状态：已批准开发
日期：2026-10-25
范围：Android App 自动化桌面端 MVP

## 1. 产品目标

用户用一句自然语言描述 Android App 任务，Agent 通过混合感知和工具调用完成操作，并在任务结束后把成功路线沉淀为可审核的 Skill，让重复任务逐步变快、变稳、变便宜。

首版必须打通：

`一句话任务 -> 混合观察 -> ReAct 执行 -> 独立结果 -> 轨迹记录 -> Skill 候选 -> 人工批准 -> 后续复用提示`

## 2. 首版功能

### 2.1 设备工作台

| 状态 | 功能 | 验收方式 |
|---|---|---|
| MVP | ADB 设备发现 | 能列出 USB/无线 ADB 设备及连接状态 |
| MVP | 设备健康信息 | 展示型号、Android 版本、分辨率、前台 App |
| MVP | 一键刷新 | 设备连接或断开后可刷新列表 |
| MVP | scrcpy 镜像 | 从工作台打开当前设备镜像 |
| MVP | 空状态引导 | 无设备时展示 USB 调试与授权提示 |
| 后续 | Mobilerun/Appium 适配器 | 通过统一 DeviceDriver 接口切换 |
| 后续 | 多设备会话 | 设备级任务、证据和上下文隔离 |

### 2.2 混合感知

| 状态 | 功能 | 验收方式 |
|---|---|---|
| MVP | 截图采集 | 获取当前屏幕截图并保存为本地证据 |
| MVP | accessibility/UI 树采集 | 解析 uiautomator XML 中的文本、描述、ID、bounds |
| MVP | 页面签名 | 根据前台 App、关键节点和截图生成页面签名 |
| MVP | 视觉输入 | 支持将截图作为多模态模型输入 |
| MVP | 结构化输入 | 将压缩后的 UI 树和前台 Activity 一起发送给模型 |
| MVP | 点击区域可视化 | 截图与元素列表可联动查看 |
| 后续 | OCR 兜底 | 无可用 UI 树时提取文字区域 |
| 后续 | 视觉 grounding 兜底 | 使用视觉模型定位图标和自绘控件 |

### 2.3 Agent 执行

| 状态 | 功能 | 验收方式 |
|---|---|---|
| MVP | 一句话任务 | 输入自然语言目标后启动任务 |
| MVP | ReAct 单步循环 | 每一步“观察 → 决策 → 工具 → 再观察” |
| MVP | 多模态提示 | 模型同时获得截图、UI 树摘要和任务状态 |
| MVP | 元素优先点击 | 支持按文本、content-desc、resource-id 定位 |
| MVP | 坐标点击兜底 | 结构化定位失败时允许坐标点击并记录原因 |
| MVP | 输入与清空 | 支持 Unicode 文本输入和输入后验证 |
| MVP | 滑动与系统键 | 支持上下左右滑动、返回、Home、Enter |
| MVP | App 启动 | 通过包名启动目标 App |
| MVP | 等待与超时 | 支持固定等待、最大步数和任务取消 |
| MVP | 独立结果说明 | Agent 必须明确说明完成、失败或需要用户输入 |
| MVP | 高风险起始门禁 | 支付、发送、删除、安装等任务需显式允许 |
| 后续 | 逐步审批 | 每个高风险工具调用前暂停等待用户确认 |
| 后续 | 人工接管与交还 | Agent 暂停后允许用户操作并重新同步 |

### 2.4 轨迹与可观察性

| 状态 | 功能 | 验收方式 |
|---|---|---|
| MVP | Run 记录 | SQLite 保存任务、设备、模型、状态和时间 |
| MVP | Step 记录 | 保存工具、参数、结果、页面签名和截图路径 |
| MVP | 实时事件 | UI 实时展示思考摘要、动作和工具结果 |
| MVP | 证据回看 | 轨迹步骤可查看对应截图引用 |
| MVP | 最终摘要 | 保存成功/失败原因和任务摘要 |
| 后续 | logcat 关联 | 按时间窗过滤设备日志并关联失败步骤 |
| 后续 | 报告导出 | 导出 Markdown/HTML 和附件目录 |

### 2.5 Skill 与持续进化

| 状态 | 功能 | 验收方式 |
|---|---|---|
| MVP | 成功轨迹清洗 | 从成功步骤中移除观察类和失败步骤 |
| MVP | Skill 候选生成 | 从工具轨迹生成名称、关键词、说明和动作计划 |
| MVP | 候选审核 | 用户在 Skill 页面批准或拒绝候选 |
| MVP | Skill 检索 | 根据任务关键词和当前 App 召回已批准 Skill |
| MVP | Skill 提示注入 | 相似任务把已验证路线作为 Agent 提示 |
| MVP | 复用统计 | 记录 Skill 使用次数、成功/失败次数 |
| 后续 | 参数化 Skill | 自动抽取搜索词、金额、地址等变量 |
| 后续 | 隔离回放验证 | 在不同输入下自动验证 Skill |
| 后续 | 版本晋升与回滚 | 候选通过门槛后自动/人工晋升 |
| 后续 | Maestro YAML 导出 | 导出人类可读、可 CI 执行的流程 |

### 2.6 模型与设置

| 状态 | 功能 | 验收方式 |
|---|---|---|
| MVP | OpenAI-compatible API | 配置 Base URL、API Key、Model |
| MVP | 视觉开关 | 可关闭截图输入进行纯结构感知 |
| MVP | 采样参数 | 支持 temperature、max steps |
| MVP | 本地设置持久化 | 设置写入应用数据目录，API Key 不再回显明文 |
| MVP | 模型连通性检查 | 保存前可检查配置是否存在 |
| 后续 | 多模型路由 | Planner、Grounding、Verifier 分别配置 |
| 后续 | 本地 GUI 模型 | 接入 GUI-Owl/UI-TARS 的 vLLM endpoint |

### 2.7 安全与边界

| 状态 | 功能 | 验收方式 |
|---|---|---|
| MVP | 全局停止 | 取消按钮请求停止 Agent 循环 |
| MVP | 高风险确认 | 未确认时拒绝执行有副作用的任务 |
| MVP | 不自动处理密码 | 遇到登录、验证码、支付密码时要求人工处理 |
| MVP | 工具白名单 | Agent 只能调用设备、观察、输入和任务工具 |
| MVP | 本地数据目录 | 截图、轨迹和 Skill 均保存在应用数据目录 |
| 后续 | 敏感字段脱敏 | 对日志、UI 树和导出内容做字段掩码 |
| 后续 | Keychain | API Key 迁移到 macOS Keychain |

## 3. 当前架构

```text
React/Tauri 工作台
        |
        v
Rust Agent Harness
  - Planner/Executor prompt
  - JSON tool protocol
  - step budget / cancellation / risk gate
  - trace and skill compiler
        |
        +---- OpenAI-compatible multimodal model
        |
        v
ADB Device Driver (MVP)
  - device discovery
  - screenshot
  - uiautomator XML
  - tap / swipe / text / key / app launch
        |
        v
Android device
```

`DeviceDriver` 是稳定边界。首版使用 ADB，后续增加 Mobilerun 与 Appium 适配器时不改变 Agent、UI 和 Skill 数据模型。

## 4. 非目标

- 首版不支持 iOS。
- 首版不做多设备并发。
- 首版不训练 GUI 模型。
- 首版不做完整录制回放和自动 Skill 晋升。
- 首版不把支付、验证码或密码交给模型。
- 首版不承诺云设备和远程执行。

## 5. 验收标准

1. 空设备状态可正常启动应用并展示连接引导。
2. 连接 Android 设备后可刷新、查看截图和 UI 树。
3. 配置多模态模型后，可用一句话启动任务并看到实时步骤。
4. 工具调用在设备上可执行，失败会回写 Agent 而不是终止 UI。
5. 任务轨迹写入 SQLite，刷新应用后仍可查看。
6. 成功任务生成 pending Skill，批准后能被相似任务召回。
7. 无模型配置、无设备、API 错误和用户取消都有明确状态。
8. `npm run build`、`cargo check` 和 Tauri 开发构建通过。
