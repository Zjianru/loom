# OpenClaw适配验证方案

状态：spike plan（first minimum validation completed）  
定位：`loom-openclaw` 作为首个宿主插件时的最小闭环验证方案  
更新时间：2026-03-12

---

## 1. 目的
这份文档回答：
1. OpenClaw v0 最小要打通哪条链路
2. 这条链路要验证哪些 owner 和边界
3. 什么属于 spike 范围
4. 什么明确不属于 spike 范围

更深层上，它解决的是：

**把“Loom 不做语义推断，只吃宿主结构化判断”从设计文档推进到一个真实可接的宿主闭环。**

---

## 2. spike 的最小目标
v0 spike 只验证这条闭环：
1. `loom-openclaw` 先完成 bridge bootstrap
2. `loom-openclaw` 先同步一次 `HostCapabilitySnapshot`
3. 用户输入先收成 `CurrentTurnEnvelope`
4. OpenClaw 主代理产出 `HostSemanticBundle`
5. `loom-openclaw` 归一化成 `SemanticDecisionEnvelope`
6. Loom 生成 `managed task start card`
7. `/loom` slash command 或宿主语义层把用户显式控制回复结构化成 `control_action`
8. Loom 进入 active task
9. Harness 跑最小 `clarify -> execute -> review -> deliver`
10. Loom 回 `ResultSummaryPayload`

这条链路的价值不在于功能多，而在于它能同时证明：
1. 宿主能提供结构化语义判断
2. kernel 不需要猜自然语言
3. `host_session_id` 和 `managedTaskRef` 已分离
4. start / control / result 三类往返都通
5. bridge peer 不是匿名接入
6. pause/cancel 未来建立在真实 host capability 上
7. `SpecBundle / ProofOfWorkBundle / IsolatedTaskRun` 由 Loom 生成，不由 adapter 发明
8. adapter 在 semantic failure path 上保持 fail-closed / fail-open 边界，不回退到文本猜测

---

## 3. 这轮 spike 不做什么
为了守住边界，v0 spike 明确不做：
1. 不做完整 `watchdog` 流动治理
   - 但允许一个最小正式 `StatusNotice`
   - 当前只冻结：
     - `stage_entered`
     - `blocked`
2. 不做完整 `review_group` / `validate_group`
3. 不做 pack marketplace
4. 不自动写 `openclaw.json`
5. 不做自定义卡 UI
6. 不把 TS runtime projection 升成治理真相源

但必须做的失败路径守卫是：
1. 缺 `interaction_lane`
   - `fail-open to chat`
2. candidate 缺 `managed_task_class / work_horizon`
   - 不进入 execute
3. low-confidence managed judgment
   - 先重判或澄清
4. major schema mismatch
   - 直接拒绝归一化
5. free-text “继续”
   - 不得猜成 `approve_start`

我的建议：
1. 先证明桥是对的
2. 再证明周边功能是完整的

---

## 3.5 这条 spike 依赖哪些正式 contract
这条最小闭环显式依赖：
1. [当前轮输入合同.md](../语义与入口/当前轮输入合同.md)
2. [宿主能力快照合同.md](宿主能力快照合同.md)
3. [决策窗口合同.md](../治理策略/决策窗口合同.md)
4. [内核出站载荷合同.md](内核出站载荷合同.md)
5. [出站投递生命周期合同.md](出站投递生命周期合同.md)
6. [能力漂移与重授权合同.md](../治理策略/能力漂移与重授权合同.md)
7. [评审结果合同.md](../工作模式与结果/评审结果合同.md)
8. [编码工作模式预设.md](../工作模式与结果/编码工作模式预设.md)
9. [结果样例.md](../工作模式与结果/结果样例.md)

---

## 4. Spike 的正式前提
### 4.1 宿主语义判断先由主代理产出
这次已锁定：
1. v0 不单独引入第二个 semantic service
2. 先由 OpenClaw 主代理产出结构化语义结果

原因：
1. 这最贴近 OpenClaw 当前现实
2. 也最符合“主代理是唯一自然语言入口”的产品体验

### 4.2 adapter 先只读宿主配置
这次已锁定：
1. adapter 可读取 [openclaw.json](../../../openclaw.json)
2. 但 v0 不自动写宿主配置

### 4.3 通知先走宿主消息流
这次已锁定：
1. `watchdog` 的最小 notice 通过宿主消息流回到聊天区
2. 不额外引入第二通道

---

## 5. 最小架构选型
### 5.1 语义入口
我的建议：
1. 使用宿主主代理输出 `HostSemanticBundle`
2. adapter 接住它
3. adapter 归一化后交给 kernel

### 5.2 传输边界
总架构仍不把最终 transport 写死成 HTTP、socket 或 in-process。

但代码 spike 这轮建议明确选：
1. `LocalHttpBridge`

原因：
1. 调试最简单
2. 宿主和 Loom 语言无关
3. 比 in-process 更不容易把 Loom 重新塞回插件内部

所以 v0 取舍是：
1. 逻辑上 daemon-ready
2. 工程上 bridge-first
3. 代码 spike 先落 `LocalHttpBridge`
4. 并先把 bridge bootstrap 和 capability sync 做成正式前置步骤

### 5.2.1 当前固定责任切分
这轮固定责任不是“plugin 拉起 bridge”，而是：
1. Loom runtime 负责运行 `LocalHttpBridge`
2. `loom-openclaw` 负责 bridge peer 的 bootstrap、health check、capability sync 与 reconnect
3. Loom launcher / supervisor / dev tooling 可以自动拉起 bridge，但这仍属于 Loom 的部署责任

原因：
1. 这样才能保持 adapter 薄层，不把 runtime transport 塞回 plugin
2. 这样 bootstrap / auth 才仍然成立为“合法 peer 接入 Loom bridge”
3. 这样后续多宿主扩展时，bridge 仍然属于 Loom 边界，而不是某个 plugin 的私有进程

固定不采用：
1. `loom-openclaw` plugin 持有 bridge 进程生命周期
2. 纯外部预启动作为默认产品责任

### 5.3 兼容投影
这轮建议双目录并存，但 owner 切开：
1. `runtime/loom/`
   - Loom authoritative truth
2. 宿主兼容投影文件

边界必须写死：
1. 宿主兼容投影只是 compatibility projection
2. `runtime/loom/` 才是 spike 里的治理真相源

---

## 6. Spike 数据流
### 6.1 Inbound
1. Loom runtime 先确保 `LocalHttpBridge` 已运行
2. adapter 启动完成 bridge bootstrap
3. adapter 先同步 `HostCapabilitySnapshot`
   - 字段 contract 以 [宿主能力快照合同.md](宿主能力快照合同.md)
     为准
4. OpenClaw `message_received`
   - adapter 读取 `HostInboundTurn`
5. adapter 先归一化出 `CurrentTurnEnvelope`
6. 宿主主代理或宿主语义层产出 `HostSemanticBundle`
7. adapter 校验：
   - `schema_version`
   - 必填 decision
   - per-decision `confidence`
   - `DecisionSource`
8. adapter 归一化成一条或多条 `SemanticDecisionEnvelope`
9. Loom 消费并推进治理分支

固定说明：
1. 如果 bridge 不可达或 bootstrap 未完成，adapter 必须 fail-closed
2. adapter 不得因为 bridge 缺失而退回“自己临时持有 bridge 进程”的模式

### 6.2 Candidate
1. 如果 `interaction_lane=managed_task_candidate`
2. 且 `managed_task_class / work_horizon / task_activation_reason` 齐全
3. Loom 创建 `managedTaskRef`
4. Loom 先打开 `PendingDecisionWindow(kind=StartCandidate)`
5. Loom 输出 `StartCard`
6. adapter 渲染成宿主文本

当前 v0 验证还要补一条现实取舍：
1. `StartCard` 的用户可见主路径，当前仍默认走 `chat.inject`
2. 也就是：
   - Loom authoritative 先提交 candidate/window/outbox
   - adapter 再把 `StartCardPayload` 文本化后投进宿主 transcript + WebUI chat
3. `before_message_write` / `message_sending`
   - 当前只算 suppression 与未来结构化替换研究入口
   - 不应在本轮验证里被写成首显主路径

因此验收判定要区分两层：
1. 如果 candidate、`PendingDecisionWindow(kind=StartCandidate)`、`decision_token` 都已 authoritative 成立
2. 但宿主聊天区仍未看到 start card
3. 这表示：
   - Loom candidate 主链成立
   - 宿主可见投递主路径失败
4. 不得把这类问题误判成“没有创建 candidate”

### 6.3 Control Action
1. start card / boundary card / approval request 继续只作为 control surface projection
2. 用户通过 `/loom approve|cancel|modify|keep|replace|reject`
   - 正式行使第一版窗口消费动作
3. command handler 先调用
   - `read_current_control_surface(host_session_id)`
4. authoritative query 至少返回：
   - `surface_type`
   - `managed_task_ref`
   - `decision_token`
   - `allowed_actions`
5. `/loom` parser 先把显式 grammar 产出成 `control_action` judgment
6. adapter 复用现有 mapping 逻辑，把该 judgment 归一化成 `ControlAction`
7. Loom 消费 authoritative window 并推进状态

固定边界：
1. `/loom approve`
   - 只允许根据当前 `allowed_actions` 归一化成 `approve_start` 或 `approve_request`
2. 不允许 adapter 直接信最近一次 outbound cache
3. query 返回 `0` 个或 `>1` 个 open window 时，必须 fail closed

### 6.4 Result
1. Loom 输出结构化 `ResultSummaryPayload`
2. adapter 渲染成宿主文本
3. 兼容投影写回 legacy runtime

---

## 7. Spike 里的关键对象
### 7.1 `host_session_id`
它是什么：
1. OpenClaw 聊天容器 id

它代表：
1. 当前用户在哪个宿主会话里聊天

它不代表：
1. 任务 owner

### 7.2 `managedTaskRef`
它是什么：
1. kernel 中单个受管任务的正式 owner

它代表：
1. 任务卡
2. 阶段
3. 通知
4. 结果

这个变量存在的意义，就是把聊天容器和治理容器拆开。

### 7.3 `HostSemanticBundle`
它是什么：
1. OpenClaw 主代理给 adapter 的综合结构化判断包

它代表：
1. 当前这条输入已经被宿主理解成什么

### 7.4 `SemanticDecisionEnvelope`
它是什么：
1. adapter 给 kernel 的归一化语义判断单元

它代表：
1. kernel 已经可以直接消费的治理输入

---

## 8. Spike 的最小输入输出
### 8.1 最小 inbound set
spike 至少需要这 4 类输入：
1. `HostInboundTurn`
2. `HostCapabilitySnapshot`
3. `HostSemanticBundle`
4. `ControlAction`

所有输入都必须带：
1. `ingress_id`
2. `causation_id`
3. `correlation_id`
4. `dedupe_window`

### 8.2 最小 outbound set
当前 `status-notice` 分支的最小 outbound set 至少需要这 4 类正式输出：
1. `StartCardPayload`
2. `ResultSummaryPayload`
3. `SuppressHostMessagePayload`
4. `StatusNoticePayload`

固定边界：
1. 最终用户可见文本仍由 adapter 基于结构化 payload 本地渲染
2. 它不再作为独立 `KernelOutboundPayload` 进入最小正式 outbound 集
3. `StatusNoticePayload`
   - 当前属于最小正式 outbound 集
   - 但只允许：
     - `stage_entered`
     - `blocked`
4. 它必须走现有 durable outbox 主链
5. adapter 必须把它归类为 `async_notice`
6. 它不占用 `current control surface`
7. `stage_ref`
   - 固定指向 `PhasePlanEntryId`
   - 不允许退回自由文本阶段名
8. `headline`
   - 对两类 notice 都是必填
9. `blocked`
   - 必须带 `stage_ref`
   - 因为当前 `watchdog` 升级语义下应能拿到对应阶段条目

---

## 9. FailurePolicy 在 spike 中怎么落
### 9.1 缺 `interaction_lane`
规则：
1. adapter 不激活 managed lane
2. 保守退回 `chat`
3. 必要时渲染澄清文本

### 9.2 lane 已进入 managed，但缺 `managed_task_class / work_horizon`
规则：
1. adapter 允许发起一次自动重判
2. 第二次仍缺：
   - 不进入 execution
   - 不生成看似完整 start card
   - 返回补判/澄清

### 9.3 低置信度
规则：
1. 每个 decision 各自判断
2. 允许一次自动重判
3. 仍不够则保守退化

### 9.4 major schema mismatch
规则：
1. 直接 fail closed
2. 不推进任何治理分支

---

## 10. OpenClaw 中的建议接线
### 10.1 建议保留的宿主事实面
建议继续利用：
1. `message_received`
2. `before_agent_start`
3. `message_sending`
4. `before_message_write`
5. `before_tool_call`
6. `tool_result_persist`
7. `subagent_spawned`
8. `subagent_ended`

原因：
1. 这些 hook 已经覆盖 inbound / outbound / tool / subagent 生命周期
2. 对 spike 来说足够了

### 10.2 建议的宿主语义输出方式
这轮我的建议是：
1. 先用主代理产出的结构化语义对象
2. adapter 捕获这个结构化对象
3. 不解析可见自然语言文本

拒绝的做法：
1. 从普通聊天文本反解析语义
2. 在 adapter 里再做 heuristics 分类
3. 让 TS runtime projection 文件充当判断真相源

---

## 11. Spike 成功标准
spike 至少满足以下 6 条才算通过：
1. OpenClaw 能产出并传递 `HostSemanticBundle`
2. adapter 能把它归一化成 `SemanticDecisionEnvelope`
3. kernel 不需要读取原始自然语言就能完成最小治理闭环
4. `host_session_id` 与 `managedTaskRef` 已分离
5. `/loom approve` 能 authoritative 地消费当前 start card，并归一化成 `approve_start` 回到 kernel
6. 最终 `ResultSummaryPayload` 能回到宿主文本层

---

## 12. 第一轮最小验证结果（2026-03-12）
### 12.1 这轮验证已经完成什么
第一轮 clean-room 已经完成，且结论足够稳定。

如果把“最小验证”定义为：
1. 用户输入进入 Loom inbound
2. OpenClaw 主代理给出 `HostSemanticBundle`
3. adapter 归一化出 candidate 所需的结构化治理输入
4. Loom 创建 `managed_task_ref`
5. Loom 打开 `PendingDecisionWindow(kind=StartCandidate)`
6. start card 进入 authoritative outbox
7. `/loom` 控制面仍能 authoritative 消费当前 open window

那么这轮已经完成。

### 12.2 这轮没有完成什么
但如果把“最小验证”理解成产品验收版：
1. `managed_task_candidate` 的第一条用户可见消息就是 start card
2. WebUI 不先泄漏普通 assistant

那么这轮没有完成。

这里几个变量要明确：
1. `host_session_id`
   - 它表示宿主聊天容器
   - 本轮是 `agent:main:main`
2. `managed_task_ref`
   - 它表示 Loom 里的正式任务 owner
   - 本轮已创建成功
3. `current_pending_window_ref`
   - 它表示当前 start card 对应的 open decision window
   - 本轮存在，说明 start window 正常
4. `delivery_status`
   - 它表示 start card 这条 authoritative delivery 当前处于什么生命周期
   - 本轮先是 `retry_scheduled`，后被 activity wake 推到 `acked`

### 12.3 这轮真实观察到的链路
clean-room 中真实发生的是：
1. 用户输入后，WebUI 第一条可见消息先变成普通 assistant 文本
2. 约 8 秒后，authoritative side 才出现：
   - `managed_task_ref`
   - `current_pending_window_ref`
   - `start_card` delivery
3. 该 delivery 命中：
   - `chat.inject -> transcript file not found`
4. 插件侧一期缓解接管为：
   - `host_not_ready`
   - 前置快重试
   - `quiescent`
   - `late_delivery_risk`
5. 随后 `/loom probe` 触发 `outbound_activity_wakeup`
6. 同一条 delivery 最终 `acked`

### 12.4 这轮应该怎样判定
这轮必须分成两层判定：
1. **最小技术验证：通过**
   - candidate 主链成立
   - `/loom probe` 唤醒链成立
   - authoritative outbox 真相未破坏
2. **最小产品验收：未通过**
   - `L-02 explicit-managed-candidate` 仍失败
   - 因为 start card 不是第一条用户可见消息

因此后续文档和口径必须避免两种误判：
1. 不能因为 `managed_task_ref` 已创建，就说 `L-02` 通过
2. 不能因为 `delivery_status=acked`，就说 start card 首显通过

### 12.5 这轮对 spike 边界的启发
这轮已经说明：
1. 当前 spike 的最小宿主接入闭环，技术上基本成立
2. 当前唯一还卡住产品验收的主阻断，是宿主 transcript materialize 时序窗
3. 因此下一步不应继续重复“这条链是否能跑通”的验证
4. 下一步应转向：
   - 继续做插件侧体验压缩，还是
   - 把问题明确收口为宿主能力缺口

---

## 13. 这轮 Spike 之后还要做什么
如果这条闭环跑通，下一步建议顺序是：
1. 先完成 phase 2 决策
   - 明确插件侧是否继续压缩 `chat.inject` 晚到风险
   - 还是把它收口成宿主能力缺口
2. 在 `status-notice` 分支先冻结并补最小 `StatusNotice`
3. 接入 `request_task_change`
4. 接入 `request_horizon_reconsideration`
5. 再把 `research_pack` 作为第二个真实对照样本接进 spike

原因：
1. 当前最小接入链已经验证过，不需要继续重复证明“有没有 candidate”
2. `StatusNotice / request_task_change / request_horizon_reconsideration`
   - 都建立在当前主接入闭环已稳定的前提上
3. 如果不先把 `chat.inject` 时序问题的策略边界定清，后面所有扩展都会重复碰到同类判断冲突

### 13.1 `request_task_change` 接入时的最小验证点
这里先冻结验证口径，避免后续实现又退回旧入口：
1. active task 变更的正向入口，必须是同一条 `HostSemanticBundle` 中显式成对出现：
   - `task_change`
   - `control_action(action_kind=request_task_change)`
2. `task_change`
   - 只承载治理判断
   - 至少包含：
     - `classification`
     - `execution_surface`
     - `boundary_recommendation`
3. 具体 patch 内容
   - 固定留在 `control_action.payload`
   - 不回填到 `task_change`
4. adapter 不得只看到 `task_change` 就自动脑补 `request_task_change`
5. 如果显式 grammar 或宿主 parser 只能产出 patch，不能产出 `task_change`
   - 必须请求补判或 fail closed
   - 不得直接提交正式 `request_task_change`
6. 验收时至少要覆盖三条 path：
   - paired bundle happy path
   - unpaired `request_task_change` fail closed
   - `task_change` 缺治理字段 fail closed

---

## 14. 我的建议
### 14.1 v0 spike 最重要的不是“做很多”
而是证明两件事：
1. OpenClaw 能作为宿主稳定给出结构化语义判断
2. kernel 真能只吃结构化判断，不再自行解释自然语言

### 14.2 如果 spike 失败，优先看什么
我的建议排查顺序：
1. 是宿主没能产出足够稳定的 `HostSemanticBundle`
2. 还是 adapter 没把它归一化好
3. 还是 kernel 对 failure policy 太激进/太宽松

不要先怀疑：
1. 阶段编排不够复杂
2. agent 数量不够多

因为 spike 的目标不是证明系统很强，而是证明边界是对的。
