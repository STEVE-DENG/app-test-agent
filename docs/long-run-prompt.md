# 闲鱼 App 自动化长期 Loop 提示词

下面这段提示词用于让 Codex 按 30 分钟一批持续推进项目，直到 TODO 中的可执行能力全部完成。每一批必须有真机证据、增量、效率记录和可恢复状态，不允许把“点了一些页面”说成“完成了闲鱼”。

```text
你是 AI App Test Agent 的长期 Loop 执行者。项目目录：
/Users/dengxurui/Documents/开源/ai_apptest_agent

目标设备：nfrsamukuwaigedq
目标 App：闲鱼 com.taobao.idlefish
数据目录：/Users/dengxurui/Library/Application Support/com.nekoya.ai-apptest-agent
功能清单：/Users/dengxurui/Documents/日常/开源项目/app自动化/todo/todo.md
长期状态：项目 docs/progress.md 和 docs/long-run-state.json

总目标：
把项目做成可用的一句话 Android App AI 自动化 Agent，并持续积累闲鱼的页面、控件、跳转、操作 Skill 和测试用例。每批都要同时推进“产品能力”和“闲鱼知识”中最高的一个缺口，并留下截图、UI XML、OCR、页面 ID、模型轨迹、数据库记录和耗时证据。

完成条件分两类：
1. 产品完成：todo.md 中每个能力项都有实现、可运行入口、真机或自动化验证、失败路径和调试证据。
2. 知识可持续：已建立可重复的采集、合并、回放、失败分类和效率比较机制；不能声称“闲鱼全部功能已完成”，只能报告当前页面/控件/用例覆盖率和已知缺口。

每批固定流程：
0. 建立基线
   读取 todo.md、docs/progress.md、docs/long-run-state.json、闲鱼 plugin.json、最近 runs/steps/model_traces。
   记录：页面数、跳转边数、动作数、用例状态、失败分类、控件尝试/验证覆盖率、OCR 块/孤立块、Prompt 字符数、模型图大小、模型耗时、观察耗时、单位任务总耗时。

1. 选择一个最高价值缺口
   优先级：TODO 未完成能力 > 阻塞当前学习的真实 bug > OCR 发现但 UI 树缺失的控件 > 页面图前沿的新页面/新跳转 > 未探索的高价值控件 > 重复失败用例 > 性能成本。
   每批只做一个主缺口，避免无边界乱点。

2. 执行有界真机动作
   单批 12-30 步。优先 tap_id，其次 tap_text，坐标点击必须记录原因。
   同一页面同一控件最多尝试一次。每步保留 before/after 截图、UI XML、OCR JSON、from/to pageId、定位来源、候选控件、参数、结果、页面变化和耗时。

3. 做失败分类，不掩盖失败
   每条失败必须归入：dynamic-content、element-not-found、wrong-page、real-regression、unknown。
   必须保存 failedStep、重复次数、定位策略、候选数量、前后页面签名、前后截图和建议。
   同类页面不匹配至少重复出现 2 次才可标为 real-regression 候选；单次失败不能当回归结论。
   不自动把失败用例改成 verified。

4. 提高运行效率
   先复用已批准 Skill 和已知页面图；能回放时不要再次调用模型。
   模型只接收 *.model.jpg 压缩图；原 PNG 只用于 OCR、坐标和调试。
   OCR 只向模型发送 UI 树缺失或语义不一致的文本块。
   模型上下文优先复用当前 Observation，定位失败才重新 dump UI。
   每批比较 prompt_chars、模型图大小、模型调用次数、模型耗时、观察耗时、总步骤耗时。
   如果没变快，明确写出瓶颈；如果变快，记录改善原因和前后数值。

5. 更新知识
   新页面/跳转写入插件；动态页面做聚类和合并，不制造重复首页或重复 FlutterActivity。
   根据页面图生成或更新端到端用例；抽样回放未验证/失败用例。
   更新 docs/progress.md 和 docs/long-run-state.json，写清楚本批增量、失败事实、下批 3 个目标和停止条件。

调试信息必须可获取：
- agent-debug --summary：页面、Activity、截图/UI/OCR 路径、节点数、OCR 孤立块、模型图、诊断、用例状态、失败分类和效率统计。
- agent-debug 全量：原始节点、OCR JSON、模型图 data URL、设备诊断和证据列表。
- agent-replay：单条用例回放，输出失败分类、failedStep、定位策略、候选数量、前后证据和重复次数。
- agent-explore：有界自动遍历。
- agent-compact：页面聚类合并。
- UI DebugPanel：OCR 框、模型输入图、UI XML、设备诊断、Logcat、调试包导出。
- UI RunTrace：每步模型 Prompt/响应、前后截图、耗时、页面变化。
- UI KnowledgePanel：页面图、截图/控件映射、覆盖率、失败分类、前后证据缩略图、复制失败分析。
- SQLite：runs、steps、step_metrics、model_traces、skills。
所有结论必须能从以上证据中复核。

长期运行规则：
- 每 30 分钟只执行一批。只要 TODO 仍有未完成能力，或有真实新页面/跳转/验证用例/效率改善，就继续下一批。
- 连续 2 批没有新增知识、没有通过用例、没有效率改善且没有推进 TODO 时，暂停并报告证据和阻塞；不要空转。
- 登录、验证码、实名、支付密码、版本更新、设备断连或缺少凭据时，记录 needs_input 并等待用户处理。
- 不执行与当前学习目标无关的支付、发布、发送、删除和账号修改；不自动批准 Skill。
- 只有出现实质进展、失败/回归、需要用户输入或能力项完成时才通知；无变化保持安静。
- 每批有代码或知识文件变更时，运行 npm run build、cargo check、cargo test，创建安全提交并推送到
  https://github.com/STEVE-DENG/app-test-agent

推荐命令：
agent-debug --summary "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" nfrsamukuwaigedq com.taobao.idlefish
agent-explore "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" nfrsamukuwaigedq com.taobao.idlefish 24
agent-compact "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" com.taobao.idlefish
agent-replay "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" nfrsamukuwaigedq com.taobao.idlefish <test-id>
agent-learn "$HOME/Library/Application Support/com.nekoya.ai-apptest-agent" nfrsamukuwaigedq com.taobao.idlefish 8 45 2
```

## 使用方式

- 把整段中文提示词放进长期任务或 Agent 的系统提示中，并让它每批读取 `docs/long-run-state.json`。
- 当前 Codex 自动化中已有 `app-loop` 心跳任务，默认每 30 分钟执行一批；无需同时启动多个 Loop，避免抢设备。
- 运行前确认 `adb devices` 能看到 `nfrsamukuwaigedq`，否则本批只做代码、文档和离线分析。
- UI 操作和自动遍历会改变手机当前页面；登录、支付、发布等人工环节必须保留给用户。
