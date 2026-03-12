# OpenClaw最小适配接口

状态：architecture draft  
定位：`loom-openclaw` 作为首个宿主插件时的最小接口草案  
更新时间：2026-03-12

---

## 1. 目的
这份文档回答：
1. OpenClaw 现在到底给了我们哪些宿主能力
2. v0 adapter 最少应该接哪些输入输出
3. 哪些事情必须留在 adapter
4. 哪些事情绝不能继续留在宿主插件里充当治理真相源

---

## 2. 当前 OpenClaw 已知能力
### 2.1 hook 事件面
从当前宿主 hook 注册代码看，OpenClaw 现在能稳定给插件这些入口：
1. `message_received`
   - 用户或 control-plane 文本进入宿主
2. `before_agent_start`
   - 本轮 agent run 即将启动
3. `before_prompt_build`
   - prompt 即将构造
4. `message_sending`
   - assistant 准备发送消息
5. `before_message_write`
   - assistant 消息即将写入 transcript
6. `before_tool_call`
   - 工具即将调用
7. `tool_result_persist`
   - 工具结果持久化
8. `subagent_spawned`
   - 子 agent 已生成
9. `subagent_ended`
   - 子 agent 已结束

这意味着：
1. OpenClaw 已经能覆盖 inbound、outbound、tool lifecycle、subagent lifecycle
2. 对 v0 adapter 来说，这个接入面已经够厚

### 2.2 宿主配置面
从 [openclaw.json](../../../openclaw.json) 看，OpenClaw 至少暴露了这些事实：
1. agent roster
2. per-agent tool allow/deny
3. subagent allow list
4. model provider 与 fallback
5. workspace / runtime root
6. plugin config 与 runtimeContextRoot

这意味着：
1. OpenClaw adapter 能拿到宿主能力快照
2. `AgentCapabilityProfile` 不需要把这些宿主实现细节硬编码进 kernel

### 2.3 当前 runtime 文件现实
OpenClaw 插件今天已经在写：
1. 宿主兼容任务投影
2. 宿主兼容输入投影
3. 宿主兼容内部 transcript 投影

这说明：
1. 当前 host plugin 已经具备本地 runtime projection 能力
2. v0 adapter 可以继续利用这一层做兼容投影
3. 但不能再把这些 projection 当成治理真相源

### 2.4 新 runtime 根建议
这轮正式建议：
1. Loom authoritative truth 写入 `runtime/loom/`
2. 宿主兼容投影只保留 compatibility projection

### 2.5 路径真相源冻结
这轮把本地路径 owner 固定成两类：
1. `bridge.runtimeRoot`
   - Loom runtime 文件树的绝对根路径
   - bootstrap ticket、probe projection 等 Loom 本地文件都必须从它派生
2. `host workspace root`
   - 宿主当前 agent / runtime context 提供的工作区根
   - `workspace_ref / readable_roots / writable_roots / gateway call cwd` 都必须从它派生

固定禁令：
1. 不再把单个 bootstrap ticket 文件路径当成独立 owner
2. 不再依赖 `api.resolvePath(".")` 或进程 `cwd` 推导 Loom runtime 路径
3. 如果宿主不能稳定给出 repo identity，`repo_ref` 留空，不得硬编码

---

## 3. adapter 的正式边界
### 3.1 adapter 负责什么
`loom-openclaw` 负责：
1. 读取宿主消息与 hook 事件
2. 解析宿主 identity
3. 发现宿主能力
4. 把 kernel 的结构化输出渲染成文本
5. 把宿主工具与 subagent 执行桥接给 kernel
6. 在需要时向 legacy runtime 写兼容投影
7. 把宿主大模型已经给出的结构化语义判断转交给 kernel

### 3.2 adapter 不负责什么
`loom-openclaw` 不负责：
1. 从原始自然语言直接得出 `interactionLane / managedTaskClass / WorkHorizon` 的最终结构化语义值
2. `managedTaskRef` 的 owner
3. `PhasePlan` 的决定
4. `AcceptancePolicy` 的最终解释
5. `ExecutionAuthorization` 的最终决定

边界说明：
1. 第 1 项由宿主语义层生成
2. 第 2-5 项由 kernel 持有

---

## 4. 关键变量在 adapter 中代表什么
### 4.1 `host_session_id`
它是什么：
1. OpenClaw 宿主会话容器标识

它代表：
1. 用户当前正在哪个聊天容器里交流

它不代表：
1. 受管任务 id

### 4.2 `managedTaskRef`
它是什么：
1. kernel 中单个受管任务的正式 owner

它代表：
1. 任务卡、阶段、通知、结果都围绕它

它和 `host_session_id` 必须分离。

### 4.3 `CurrentTurnEnvelope`
它是什么：
1. adapter 归一化后真正交给 Loom 的正式入站事实对象

在 adapter 口径下它应该被理解为：
1. 宿主原始 `HostInboundTurn` 的正式领域投影
2. Loom 识别“当前输入是谁”的唯一 formal inbound owner

### 4.4 `ResolvedHookSessionRuntime.canonical`
它是什么：
1. 当前 TS hook 系统解析出来的 canonical session identity

它代表：
1. 宿主 continuity / explicit window 恢复后的会话 identity

它不应该被提升成：
1. task owner

### 4.5 `ExecutionAuthorization`
它是什么：
1. 当前运行时这次真的拿到的能力租约

在 adapter 里它的意义是：
1. kernel 已决定这轮能做什么
2. adapter 要按这个结果去桥接真实 OpenClaw 工具调用

---

## 5. v0 最小 inbound 接口
OpenClaw adapter 至少要向 kernel 发送这些结构化输入。

### 5.0 `IngressMeta`
```rust
pub struct IngressMeta {
    pub ingress_id: IngressId,
    pub causation_id: Option<CausationId>,
    pub correlation_id: CorrelationId,
    pub dedupe_window: DedupeWindow,
}
```

它代表：
1. 这条 ingress 的幂等键是什么
2. 它由哪条宿主事件或哪次上游决策引起
3. 同一条任务链里的相关请求如何关联

我的建议：
1. 所有 ingress 命令都必须带 `IngressMeta`
2. 否则 OpenClaw hook 重放、adapter 重投递、daemon 恢复都会造成重复消费

### 5.1 `HostInboundTurn`
```rust
pub struct HostInboundTurn {
    pub meta: IngressMeta,
    pub host_session_id: HostSessionId,
    pub host_message_id: HostMessageId,
    pub actor_ref: HostActorRef,
    pub content: String,
    pub message_kind: HostMessageKind,
    pub received_at: Timestamp,
    pub host_context: HostContextSnapshot,
}
```

它回答：
1. 谁在什么会话里发了什么话
2. 当前宿主上下文是什么

固定边界：
1. `HostInboundTurn`
   - 只停留在宿主 hook/bridge transport 层
2. adapter 必须先把它归一化成 `CurrentTurnEnvelope`
3. Loom 不直接把 `HostInboundTurn` 当 formal inbound object 消费

### 5.2 `HostRunLifecycleEvent`
```rust
pub enum HostRunLifecycleEvent {
    AgentRunStarting(AgentRunStartingPayload),
    PromptBuilding(PromptBuildingPayload),
}
```

它回答：
1. 这轮宿主 run 正在进入哪个阶段

### 5.3 `HostToolIntent`
```rust
pub struct HostToolIntent {
    pub meta: IngressMeta,
    pub host_session_id: HostSessionId,
    pub host_run_id: HostRunId,
    pub actor_ref: HostActorRef,
    pub tool_name: String,
    pub tool_input: serde_json::Value,
    pub observed_at: Timestamp,
}
```

它回答：
1. 宿主现在准备做什么工具调用

### 5.4 `HostToolResult`
```rust
pub struct HostToolResult {
    pub meta: IngressMeta,
    pub host_session_id: HostSessionId,
    pub host_run_id: HostRunId,
    pub tool_name: String,
    pub result_payload: serde_json::Value,
    pub persisted_at: Timestamp,
}
```

### 5.5 `HostSubagentLifecycleEvent`
正式字段 contract 统一以：
1. [宿主执行派发合同.md](宿主执行派发合同.md)
   为准。

它回答：
1. 子 agent 何时生成
2. 何时结束
3. 结果如何

### 5.5.1 `HostExecutionCommand` 与 `ack_host_execution`
正式字段 contract 统一以：
1. [宿主执行派发合同.md](宿主执行派发合同.md)
   为准。

它回答：
1. Loom 现在到底要派发哪条执行命令
2. `ack_host_execution` 确认的是“宿主已接受 dispatch”，不是“adapter 已读到命令”
3. dispatch 失败、ack 失败、`Spawned / Ended` 迟到时，状态机应该怎样收口

OpenClaw 当前映射里几个变量分别代表：
1. `helperSessionKey`
   - adapter-local 的内部 dispatch 句柄
   - 只用于把内部执行 prompt 发进宿主
   - 不得拿它充当 authoritative command truth
   - 也不得拿它倒推出 `session_scope`
   - 如果 capability builder 只能利用这类本地 dispatch 线索推断 role
     必须同步成 `session_scope.source=Derived`
2. `childSessionKey`
   - 宿主真实回传的 child execution handle
   - 正式映射到 `host_child_execution_ref`
   - 如果当前 TS/HTTP transport 还保留 `child_session_key`
     只能把它当 compat alias
3. `command_id`
   - Loom authoritative 派发命令 id
   - adapter 应把它映射成宿主原生幂等键

### 5.6 `HostCapabilitySnapshot`
正式字段 contract 统一以：
1. [宿主能力快照合同.md](宿主能力快照合同.md)
   为准。

它回答：
1. OpenClaw 当前真的能给什么
2. 运行中的 worker 到底能不能被 pause / resume / cancel

### 5.6.1 `HostWorkerControlCapabilities`
正式字段 owner 也统一以：
1. [宿主能力快照合同.md](宿主能力快照合同.md)
   为准。

它回答：
1. 当前宿主是否真的支持 pause
2. 是否支持恢复
3. 是否支持 cooperative interrupt
4. 是否支持 hard interrupt

### 5.7 `HostSemanticBundleIngress`
```rust
pub struct HostSemanticBundleIngress {
    pub meta: IngressMeta,
    pub host_session_id: HostSessionId,
    pub host_message_ref: Option<HostMessageRef>,
    pub managed_task_ref: Option<ManagedTaskRef>,
    pub bundle: HostSemanticBundle,
}
```

它回答：
1. OpenClaw 主 assistant 或宿主语义层已经对这条输入做了综合结构化判断
2. adapter 接下来应如何把它拆成一个或多个 `SemanticDecisionEnvelope`

字段细节应回看：
1. [宿主语义协议.md](../语义与入口/宿主语义协议.md)

这条能力非常关键，因为你已经明确：
1. kernel 不负责语义推断
2. kernel 只吃结构化语义结果
3. OpenClaw 侧更自然的是一次产出整条输入的综合判断包，而不是零散 decision
4. `HostSemanticBundleIngress`
   - 只代表宿主 transport 层送来的综合判断包
   - 不是 Loom 最终消费的 formal judgment owner

一致性约束：
1. 如果 `HostSemanticBundleIngress.managed_task_ref` 和 `bundle.managed_task_ref` 同时存在
2. 它们必须一致
3. 不一致时 adapter 必须 fail closed

### 5.8 `ControlActionIngress`
```rust
pub struct ControlActionIngress {
    pub meta: IngressMeta,
    pub host_session_id: HostSessionId,
    pub host_message_ref: Option<HostMessageRef>,
    pub managed_task_ref: Option<ManagedTaskRef>,
    pub action: ControlAction,
}
```

它回答：
1. 宿主显式结构化入口已经把这次用户回复分类成正式控制动作
2. `loom-openclaw` 只负责把这条结构化动作送进 Loom

这里要特别强调：
1. v0 不允许 adapter 从纯文本自己猜 `approve_start / request_task_change / replace_active`
2. 这些都必须先由宿主显式结构化入口写成 `control_action` judgment
   - 可以来自宿主语义层
   - 也可以来自 `/loom` slash command parser
3. `/loom` parser 的作用是把显式 grammar 变成结构化 judgment
   - 这不是 free-text inference
   - 也不是第二套 `ControlAction` owner
4. 所有消费 pending decision window 的控制动作都必须带 `decision_token`
4. 缺 token 时，adapter 必须 fail closed，而不是继续把旧回复映射成有效动作
5. 如果 ingress 层和 `action.managed_task_ref` 同时存在，它们也必须一致
6. `ControlActionIngress`
   - 只属于 adapter transport carrier
   - 进入 Loom 主链时最终消费的仍是 `action: ControlAction`

### 5.8.1 `/loom` 作为最小正式控制面
OpenClaw WebUI 现状没有稳定“点击回调” transport，所以 v0 正式控制面固定为：
1. card 继续只做 control surface projection
2. `/loom ...`
   - 作为用户正式行使治理动作的最小 action carrier

第一版 grammar 先只收 window-consuming 动作：
1. `/loom`
   - 读取当前 authoritative control surface 并展示可执行命令
2. `/loom approve`
   - 命中当前 `start_card` 时映射成 `approve_start`
   - 命中当前 `approval_request` 时映射成 `approve_request`
3. `/loom cancel`
   - 映射成 `cancel_candidate`
4. `/loom modify <summary 或 JSON>`
   - 映射成 `modify_candidate`
5. `/loom keep`
   - 映射成 `keep_current_task`
6. `/loom replace`
   - 映射成 `replace_active`
7. `/loom reject`
   - 映射成 `reject_request`
8. `/loom probe`
   - 只用于 transport 诊断，不属于正式治理动作

这轮明确不放进第一版 grammar：
1. `pause / resume / cancel_task / request_review / request_task_change / request_horizon_reconsideration`
2. 原因不是这些动作不存在
3. 而是当前 WebUI 最小正式控制面先只收“消费 open window 的动作”
4. 目标协议层面：
   - `request_task_change` 已有正式入口合同
   - `request_horizon_reconsideration` 已收成正式动作，但 paired bundle 形状仍待设计冻结

### 5.8.2 执行前必须先查 authoritative current control surface
`/loom` command handler 在真正发控制动作前，必须先向 bridge 查询：
```rust
pub struct CurrentControlSurfaceProjection {
    pub host_session_id: HostSessionId,
    pub surface_type: ControlSurfaceType,
    pub managed_task_ref: ManagedTaskRef,
    pub decision_token: DecisionToken,
    pub allowed_actions: Vec<ControlActionKind>,
}
```

这几个字段在控制面里的含义必须写死：
1. `action_kind`
   - 这次到底在请求哪条正式控制动作
2. `managed_task_ref`
   - 当前治理动作真正命中的任务 owner
   - 不是聊天容器 id
3. `decision_token`
   - 当前 open window 的 authoritative 消费令牌
   - 只要消费窗口，就必须带它
4. `allowed_actions`
   - 这次 surface 当前允许哪些正式动作
   - `/loom approve`
     之类的短命令必须靠它做 fail-closed 归一化

固定规则：
1. query 输入先只用 `host_session_id`
2. query 返回 `0` 个 open window 时，command handler 必须拒绝提交正式动作
3. query 返回 `>1` 个 open window 时，bridge 必须 fail closed，不能偷偷挑一个
4. adapter cache、WebUI 文案、最近一次 outbound 文本
   - 都不能充当最终控制依据

### 5.8.3 `/loom` 仍然复用同一条归一化 owner
这轮固定采用：
1. `/loom` parser 先产出显式 `control_action` judgment
2. 再复用现有 mapping 逻辑归一化成 `ControlAction`
3. 进入 Loom 主链时最终消费的仍然是正式 `ControlAction`

不采用：
1. `/loom` 直接绕过 mapping 手写第二套 `ControlAction` ingress 归一化
2. adapter 直接相信最近一次 start card cache

---

## 6. v0 最小 outbound 接口
kernel 至少要能向 OpenClaw adapter 发这些结构化输出。

### 6.1 `KernelOutboundPayload`
```rust
pub enum KernelOutboundPayload {
    StartCard(StartCardPayload),
    BoundaryCard(BoundaryCardPayload),
    ApprovalRequest(ApprovalRequestPayload),
    ResultSummary(ResultSummaryPayload),
    SuppressHostMessage(SuppressHostMessagePayload),
    ToolDecision(ToolDecisionPayload),
    StatusNotice(StatusNoticePayload),
}
```

这些 payload 的正式字段形状，不在本文件重复展开，而统一引用：
1. [内核出站载荷合同.md](内核出站载荷合同.md)

这份最小接口稿只定义：
1. adapter 必须能接这些 payload
2. 它们怎样进入宿主投递链
3. `StatusNotice`
   - 当前已纳入最小正式 outbound payload 集
   - 只允许：
     - `StageEntered`
     - `Blocked`
   - `stage_ref` 必须绑定到 `PhasePlanEntryId`
   - `headline` 对两类 notice 都必填
   - adapter 侧固定归类为 `async_notice`
4. 文本化结果
   - 由 adapter 基于这些结构化 payload 本地渲染
   - 不再作为独立 `KernelOutboundPayload` 对象
5. `HostExecutionCommand`
   - 属于执行桥接 aggregate
   - 不是用户可见 `KernelOutboundPayload`

这里要再写死一条：
1. `StartCardPayload`
   - 必须携带 `decision_token`
2. `BoundaryCardPayload`
   - 必须携带 `decision_token`
3. adapter 渲染给用户时可以只展示 `/loom` 可执行命令，而不必直露 token
4. 但后续控制回复仍必须把 token 原样带回 Loom

再补一条 outbox 边界：
1. `KernelOutboundPayload`
   - 不是 delivery lifecycle 本身
2. delivery 的创建、retry、ack、expired、terminal failure
   - 统一由 [出站投递生命周期合同.md](出站投递生命周期合同.md)
     定义
3. host execution command 的排队、dispatch、ack、running、terminal state
   - 统一由 [宿主执行派发合同.md](宿主执行派发合同.md)
     定义

### 6.2 文本渲染结果
它是什么：
1. adapter 把 Loom 结构化 payload 落成的宿主文本结果

固定边界：
1. 你已经明确 v0 用户层必须文本化
2. 但文本化是 adapter 的本地渲染结果，不是新的内核协议 payload
3. kernel 继续只输出正式结构化对象，不重新长出一条自由文本真相源

### 6.3 `SuppressHostMessagePayload`
它是什么：
1. 告诉 adapter 哪些宿主原始消息不应该继续透给用户

它的价值在于：
1. 防止 host prelude、内部治理旁白、工具噪音直接进主聊天区

### 6.4 `ToolDecisionPayload`
它是什么：
1. kernel 对某次 OpenClaw 工具意图的治理结果

建议值：
1. `allow`
2. `deny`
3. `requires_user_approval`

### 6.5 capability drift 边界
adapter 除了接 outbound，还必须接住宿主能力变化带来的重授权后果。

正式定义以：
1. [能力漂移与重授权合同.md](../治理策略/能力漂移与重授权合同.md)

为准。

这里的最小接口要求是：
1. adapter 必须能同步新的 `HostCapabilitySnapshot`
2. Loom 可能因此收紧、撤销或重发 `ExecutionAuthorization`
3. adapter 不得继续按旧 host capability 假设执行

---

## 7. OpenClaw v0 的 host mapping 建议
### 7.1 agent 映射
建议 adapter 从 [openclaw.json](../../../openclaw.json) 读取：
1. `agents.list`
2. 每个 agent 的 `subagents.allowAgents`
3. `tools.allow / tools.deny`

然后把它们收成：
1. `HostAgentCapability`
2. `HostMappingRegistry`

### 7.2 model 映射
建议 adapter 读取：
1. `agents.defaults.model`
2. per-agent `model.primary`
3. provider/model 清单

这里的取舍是：
1. 内核只表达 `desired_model_policy`
2. OpenClaw adapter 再把它映射成真实可用模型

### 7.3 tool 映射
建议 adapter 读取：
1. 全局工具可用性
2. per-agent allow/deny
3. 关键 tool 名称与宿主真实工具名的对应表

### 7.4 subagent 映射
建议 adapter 读取：
1. `subagents.maxConcurrent`
2. `maxSpawnDepth`
3. `maxChildrenPerAgent`
4. agent allow list

原因：
1. `COMPLEX / HUGE / MAX` 的真实执行上限，在 OpenClaw 里首先受宿主 subagent 机制约束

---

## 8. v0 的 adapter 约束
### 8.1 只读宿主配置，不自动写
这条建议已经基本锁住：
1. v0 adapter 先只读 OpenClaw 配置
2. 不自动写 `openclaw.json`

原因：
1. 你的产品一直强调边界和治理
2. adapter 不应该在 v0 就代替用户改宿主配置

### 8.2 v0 不做自定义卡片样式
原因：
1. 当前只有 OpenClaw 一个宿主
2. 而且它不支持这类宿主自定义卡 UI

所以：
1. 保持协议结构化
2. 显示层输出文本

### 8.3 v0 只走宿主消息流通知
也就是：
1. `watchdog` 的主动通知
2. recorder 的阶段汇报
3. approval 提示

都通过宿主消息流回到聊天区。

---

## 9. 我的建议
### 9.1 OpenClaw v0 已经足够做首个 adapter
从当前 hook 与配置现实看，我的判断是：
1. OpenClaw 已经给了足够厚的适配面
2. v0 不缺“能不能接”
3. 缺的是“谁才是真治理 owner”

### 9.2 不要让 adapter 继续偷拿治理真相
最危险的错误不是能力不够，而是：
1. adapter 一边做宿主映射
2. 一边继续在 TS runtime 文件里私自做治理判断

这会让整个架构在第一天就双真相源。

### 9.3 我的建议
1. 把 OpenClaw adapter 定位成：
   - host ingress
   - host egress
   - host capability discovery
   - host execution bridge
2. 不让它再继续充当：
   - task owner
   - acceptance owner
   - WIP owner
   - approval owner

否则 Rust kernel 永远只能是“旁边的第二套系统”。
