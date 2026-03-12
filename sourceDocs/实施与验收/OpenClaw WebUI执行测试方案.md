# OpenClaw WebUI执行测试方案

更新时间：2026-03-11

---

## 1. 文档定位
这份文档定义 Loom 的正式产品验收方法。

固定规则：
1. 官方验收入口只认 `OpenClaw WebUI`。
2. Playwright 只是执行 WebUI 人机交互的工具，不是验收真相源。
3. `vitest` 和其他自动化脚本只承担工程回归，不替代本文件定义的产品验收。

---

## 2. 唯一验收入口
### 2.1 官方入口
正式产品验收固定通过：
1. `OpenClaw WebUI`
2. 真实聊天输入
3. Playwright 驱动交互执行

不被承认为产品验收入口的方式：
1. 直接写 runtime 文件
2. 直接调用 Loom 内部接口
3. 直接调用 gateway websocket 或脚本桩
4. 只跑 `vitest`、集成脚本或模拟 case

### 2.2 当前网关约束
当前 OpenClaw 网关配置来自：
1. [openclaw.json](../../openclaw.json)

当前固定事实：
1. `gateway.port = 18789`
2. `gateway.auth.mode = token`
3. `gateway.auth.token = ${GATEWAY_AUTH_TOKEN}`
4. `controlUi.allowedOrigins = ["*"]`

执行正式验收时：
1. 只读取运行中的 gateway 配置
2. 不在测试证据中记录 raw token
3. 浏览器态必须使用 OpenClaw 官方生成的 dashboard URL 带上有效 token
4. 不得把裸 `http://127.0.0.1:18789/chat?...` 页面当成官方验收入口
5. 不得先看内部路由页面状态，再倒推官方 dashboard 是否可用

### 2.3 标准接入恢复流程
每次正式验收前，先按这条标准流程接入 WebUI：
1. 在 `/Users/codez/.openclaw/loom/adapters/openclaw` 执行 `npm run export:extension`
2. 确认导出产物已覆盖 `/Users/codez/.openclaw/extensions/loom-openclaw/`
3. 确认 `openclaw.json` 中 `plugins.entries.loom-openclaw.config.bridge.runtimeRoot` 已配置为绝对路径
4. 确认 `join(runtimeRoot, "loom/bootstrap/openclaw/bootstrap-ticket.json")` 已存在
5. 启动 bridge，并确认 `curl http://127.0.0.1:6417/v1/health` 返回 `status=ready`
6. 在本机执行 `openclaw dashboard --no-open`
7. 只使用 OpenClaw 打印出的官方 URL 打开 WebUI
8. 如果 gateway 仍拒绝当前浏览器，会生成 pending device pairing request
9. 执行 `openclaw devices list --json` 确认 pending request
10. 执行 `openclaw devices approve --latest --json` 或按 `requestId` 精确批准
11. reload 官方 dashboard URL
12. 每次打开或 reload WebUI 后，至少等待 3 秒，再检查页面 health、输入框、发送按钮、会话选择器和聊天内容
13. 直到页面 health 为 `OK` 且输入框、发送按钮、会话选择器可用

固定规则：
1. 不手工拼接非官方 token URL
2. 不在文档、截图、日志证据中记录 raw token
3. 只有完成这条恢复流程后仍失败，才记为“前置条件失败”
4. 不得用裸 `/chat?...` 页面去判断 gateway health；只有官方 dashboard URL 的页面状态才算验收证据
5. 如果 gateway 或 bridge 刚重启，必须重新打开官方 dashboard URL，不复用旧页面状态
6. `LocalHttpBridge` 的 runtime owner 仍然是 Loom，不是 plugin
7. 截至 `2026-03-11`，当前仓库只实现了 `loom-bridge-http serve` 入口，尚未把自动拉起 bridge 接回默认启动链路
8. 所以当前本地正式验收仍要显式启动 bridge；这是当前实现缺口，不是把 bridge owner 改回 plugin

### 2.4 明确要求“清环境”时的恢复流程
如果测试任务、测试案例或执行指令明确要求“清环境”“崭新环境”“clean session + clean runtime”，必须先执行这条流程，再开始接入 WebUI：
1. 停止 gateway 服务
2. 确认 bridge 已停止；如果还在运行，先停止 bridge
3. 清空 `agents/*/sessions/` 下的全部会话文件
4. 清空 `runtime/mesh/` 下的全部运行态记录
5. 清空 `runtime/loom/` 下的全部 authoritative state、projection、bootstrap、host-bridge 记录
6. 清空当轮 gateway 日志文件
7. 重新启动 gateway
8. 重新启动 bridge，并重新确认 `/v1/health status=ready`
9. 再回到 [2.3 标准接入恢复流程](#23-标准接入恢复流程)

固定规则：
1. 明确要求“清环境”时，不允许只点 WebUI 的 `New session`
2. 明确要求“清环境”时，不允许只清 `runtime/loom/` 而保留 `runtime/mesh/` 或 `agents/*/sessions/`
3. 明确要求“清环境”时，不允许复用清理前的 dashboard 页面状态、旧 start card、旧 control surface 或旧日志证据
4. 清环境的目的，是同时清掉宿主会话索引、宿主运行投影、Loom authoritative truth 和本轮日志，避免出现“两个活跃 window”这类跨层残留

### 2.5 WebUI 稳定等待规则
执行 WebUI 验收时，以下动作之后都必须至少等待 3 秒，再做页面判断、截图或继续下一步：
1. 首次打开官方 dashboard URL
2. reload dashboard URL
3. 点击 `New session`
4. 发送一条普通消息
5. 发送 `/loom ...` slash command

固定规则：
1. 不允许在导航完成、按钮点击或消息发送后立刻判定“页面异常”或“页面正常”
2. 至少等待 3 秒后，才允许依据 health、禁用态、聊天内容和控制面内容做结论
3. 如果 3 秒后页面仍在加载或状态摇摆，继续等待并记录等待事实，不得把瞬时中间态当最终结论

---

## 3. 正式执行前置条件
每次执行正式验收前，必须同时满足：
1. `http://127.0.0.1:18789/` 可访问
2. 已通过 [2.3 标准接入恢复流程](#23-标准接入恢复流程) 打开官方 dashboard URL
3. `OpenClaw WebUI` 能进入 chat 页面
4. 页面不是 `Disconnected from gateway.`
5. health 显示为 `OK`
6. 输入框、发送按钮、会话选择器不是 disabled
7. `http://127.0.0.1:6417/v1/health` 返回 `status=ready`
8. `openclaw.json` 中 `bridge.runtimeRoot` 为绝对路径
9. `join(runtimeRoot, "loom/bootstrap/openclaw/bootstrap-ticket.json")` 可读
10. 宿主当前 `workspace_ref / readable_roots / writable_roots` 不依赖 `cwd`
11. 最新 `HostCapabilitySnapshot` 已能表达当前会话的 `spawn_capabilities / session_scope`
12. `runtime/loom/` 可读
13. gateway 日志可读

固定失败判定：
1. 如果标准接入恢复流程后 WebUI 仍显示 `Disconnected from gateway.`，本次验收不执行，记为“前置条件失败”
2. 如果日志出现 `token_missing`，说明浏览器没有带上有效 dashboard token；完成标准接入恢复流程后仍存在，记为“前置条件失败”
3. 如果日志出现 `pairing required`，说明浏览器设备未完成配对；完成标准接入恢复流程后仍存在，记为“前置条件失败”
4. 如果聊天输入框、发送按钮、会话选择器保持 disabled，本次验收不执行，记为“前置条件失败”
5. 如果 bridge 未 ready，或 `bridge.runtimeRoot` 不是绝对路径，本次验收不执行，记为“前置条件失败”
6. 如果插件还在依赖 `cwd` 推导 bootstrap ticket、workspace root 或 gateway CLI `cwd`，本次验收不执行，记为“前置条件失败”
7. 如果 capability snapshot 仍然只靠 `supports_spawn_agents` 或顶层中断布尔值表达当前会话能力，本次验收不执行，记为“前置条件失败”
8. 如果没有遵守 [2.5 WebUI 稳定等待规则](#25-webui-稳定等待规则) 就直接判页面状态，本次判定无效，必须重做

---

## 4. 正式执行流程
每条正式用例统一按这条流程执行：
1. 打开对应测试用例，确认测试目的、前置条件和失败判定
2. 先完成 [2.3 标准接入恢复流程](#23-标准接入恢复流程)
3. 如果当前任务明确要求清环境，先完成 [2.4 明确要求“清环境”时的恢复流程](#24-明确要求清环境时的恢复流程)
4. 用 Playwright 打开 `OpenClaw WebUI`
5. 按 [2.5 WebUI 稳定等待规则](#25-webui-稳定等待规则) 等待页面稳定
6. 校验前置条件已满足
7. 在 WebUI 中按“输入脚本”驱动真实对话
8. 在交互期间按用例要求采集：
   - 用户可见证据
   - `runtime/loom/` 状态证据
   - gateway / runtime 结构化日志证据
9. 对照用例逐项判定通过或失败
10. 记录执行结果、阻断原因和证据位置

固定要求：
1. 不允许跳过用户可见路径，直接观察后台状态判通过
2. 不允许只凭聊天文本判通过
3. 不允许只凭日志判通过
4. 必须同时满足：用户可见结果、运行时状态、结构化日志
5. WebUI 至少要完成一轮真实可发送、可接收的聊天，才能认定“入口可用”
6. 任何关键交互后都必须遵守 [2.5 WebUI 稳定等待规则](#25-webui-稳定等待规则)

---

## 5. 证据采集标准
### 5.1 用户视角
必须采集：
1. WebUI 页面截图
2. 用户输入内容
3. 助手输出或卡片内容
4. `/loom` slash command 提示、输入框/发送按钮禁用态或审批卡的可见状态
5. 官方 dashboard URL 已注入 token 的打开证据，不采集裸 `/chat?...` 页面代替

### 5.2 运行时视角
必须采集：
1. `runtime/loom/` 中相关正式对象是否生成、更新或 supersede
2. 关键对象引用关系是否成立
3. 是否出现不应存在的平行对象或越权状态

### 5.3 日志视角
必须采集结构化日志证据，不靠自然语言备注。

第一层风险治理至少验证：
1. `risk_assessment.created`
2. `risk_assessment.superseded`
3. `execution_authorization.issued`
4. `execution_authorization.narrowed`
5. `execution_authorization.reissued`

第二层责任协作至少验证：
1. `agent_binding.issued`
2. `agent_binding.superseded`
3. `handoff.proposed`
4. `handoff.accepted`
5. `handoff.cancelled`

第三层注意力冻结至少验证：
1. notice 仍由 `watchdogNoticeState` 承接
2. 没有独立 `AttentionPolicy` 相关主链日志

### 5.4 证据记录禁令
1. 不记录 raw gateway token
2. 不在证据中泄漏 secret 内容
3. 不把内部自由文本 debug log 当作正式验收唯一证据

---

## 6. 正式测试用例字段
每条正式用例必须至少包含：
1. `用例编号`
2. `层级归属`
3. `测试目的`
4. `前置条件`
5. `WebUI 输入脚本`
6. `期间行为`
7. `预期用户可见结果`
8. `预期运行时状态`
9. `预期日志`
10. `证据采集`
11. `失败判定`
12. `当前是否可执行`

---

## 7. 当前三层推进状态
### 7.1 第一层
当前状态：正式推进  
当前 owner：
1. `RiskAssessment`
2. `TaskScopeSnapshot`
3. `ExecutionAuthorization`

### 7.2 第二层
当前状态：最小冻结  
当前 owner：
1. `AgentBinding`
2. `HandoffContract`

固定约束：
1. `coding_pack / COMPLEX` 默认路径不得创建 `HandoffContract`
2. `HUGE / MAX` 的正式交接才允许进入最小 handoff 路径

### 7.3 第三层
当前状态：deferred  
当前 owner：
1. `watchdogNoticeState`
2. `WipPolicy / ReworkPolicy / AcceptancePolicy` 的 `...escalation_policy`

固定约束：
1. 当前不引入独立 `AttentionPolicy`
2. 当前不引入用户级注意力对象
3. 当前不进入生活节律治理实现

---

## 8. 当前第一轮 WebUI 执行清单
当前必须准备并执行：
1. `R-01 baseline-issued-after-approve-start`
2. `R-02 scope-version-change-supersedes-baseline`
3. `R-03 risky-action-creates-action-override`
4. `R-04 critical-risk-stops-silent-execution`
5. `R-05 capability-drift-reissues-authorization`
6. `H-01 complex-path-does-not-create-handoff`
7. `A-01 no-independent-attention-object-in-v1`
8. `D-04 status-notice-async-outbox`

当前保留但不进入本轮 gate：
1. `H-02 huge-explicit-handoff-creates-minimal-contract`

---

## 9. 当前结论
后续所有正式产品验收，都必须先满足本文件定义的 WebUI 单入口标准。  
如果实现只能靠脚本或内部接口证明正确，而不能通过 WebUI 人机交互案例证明，则不能判定为产品验收通过。

当前接入口径已经冻结为：
1. token 通过 `openclaw dashboard --no-open` 输出的官方 URL 注入
2. 浏览器设备通过 `openclaw devices approve` 完成配对
3. 只有在完成 token 注入和设备配对后，WebUI 才可进入正式验收
