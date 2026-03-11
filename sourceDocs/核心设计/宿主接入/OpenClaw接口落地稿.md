# OpenClaw接口落地稿

状态：landing draft  
定位：`loom-openclaw` 把 Loom 协议真正落到 OpenClaw hook 面上的最小接口稿  
更新时间：2026-03-11

---

## 1. 目的
这份文档不再讨论“接口应该有什么对象”，而回答更具体的 4 个问题：
1. OpenClaw 哪些 hook 负责把宿主事实送进 Loom
2. `loom-openclaw` 应该向 Loom 发什么最小结构化请求
3. Loom 回来的结构化结果怎样落回 OpenClaw
4. 哪些对象写进 `runtime/loom/`，哪些只允许写进宿主兼容投影

更深层上，它解决的是：

**把 `CurrentTurnEnvelope`、`SemanticDecisionEnvelope`、`ControlAction`、`ResultSummaryPayload` 这些正式 contract 从“文档上的对象”变成可接线的宿主接口。**

---

## 2. 正式命名
这一稿统一使用这组名字：
1. `Loom`
   - 产品与治理内核总名
2. `Harness`
   - Loom 中承接任务治理主链的核心运行时
   - 它负责读取 `interactionLane`、`managedTaskClass`、`PhasePlan`、`pendingUserDecision`，并推进阶段和 agent 绑定
3. `loom-openclaw`
   - OpenClaw 宿主插件
4. `runtime/loom/`
   - Loom authoritative truth root
5. 宿主兼容投影根
   - compatibility projection root

这里的取舍非常明确：
1. 新系统不再复用旧运行时命名当主轴
2. 旧 TS 插件和旧 runtime 文件只保留参考与兼容角色
3. Loom 自己拥有独立的命名和独立的 runtime 根

---

## 3. 最小落地边界
### 3.1 Loom 持有的真相源
Loom 持有：
1. `managedTaskRef`
   - 单个受管任务 owner
2. `activeManagedTaskRef`
   - 当前唯一 active 任务指针
3. `PhasePlan`
   - 这次任务实际采用的阶段序列
4. `IsolatedTaskRun`
   - 当前执行期运行单元；当前阶段运行态作为 run 的从属状态
5. `ExecutionAuthorization`
   - 当前这轮执行真正拿到的能力租约
6. `TaskEventModel`
   - 统一事件链

### 3.2 `loom-openclaw` 持有的宿主事实
`loom-openclaw` 持有：
1. `host_session_id`
   - OpenClaw 聊天容器 id
2. `host_message_ref`
   - 宿主消息引用
3. `HostCapabilitySnapshot`
   - 宿主真实 agent/model/tool/subagent 能力快照
4. `HostSemanticBundle`
   - 宿主主模型已经给出的综合结构化语义判断包
5. `RenderedTextPayload`
   - adapter-local 的最终渲染结果，不是 Loom 正式出站 contract

`loom-openclaw` 不持有：
1. `SpecBundle`
2. `ProofOfWorkBundle`
3. `IsolatedTaskRun`

这些对象都由 Loom 生成、持有和投影；adapter 只接收其结构化摘要并做文本渲染。

### 3.3 这一层为什么不能再含糊
如果 `loom-openclaw` 继续偷偷持有 task truth，会立刻出现双真相源：
1. Loom 以为自己是 `managedTaskRef` owner
2. OpenClaw 侧又继续在 TS runtime 里把 session 文件当 task truth

这会直接让：
1. `activeManagedTaskRef`
2. `pendingUserDecision`
3. `ExecutionAuthorization`
4. `ResultSummaryPayload`

都出现 owner 冲突。

---

## 4. OpenClaw hook 到 `loom-openclaw` 的映射
### 4.1 Inbound
| OpenClaw hook | `loom-openclaw` 动作 | 发给 Loom 的对象 | 为什么这样切 |
| --- | --- | --- | --- |
| adapter startup | 完成 bridge bootstrap handshake | `BridgeBootstrapRequest / BridgeBootstrapAck` | 先证明 bridge peer 合法，后续 ingress 才能进 Loom |
| adapter startup / host config refresh | 同步宿主能力快照 | `HostCapabilitySnapshot` | 让 Loom 先知道当前 agent、tool、worker control 的现实能力 |
| `message_received` | 组装 `HostInboundTurn` 并归一化 | `CurrentTurnEnvelope` | 宿主原始入站事实先采集，再收成 Loom 正式入站对象 |
| `before_agent_start` | 准备本轮语义入口和 host context | `HostRunLifecycleEvent::AgentRunStarting` | 让 Loom 知道这轮 run 边界 |
| `before_prompt_build` | 可选补充 host context snapshot | `HostRunLifecycleEvent::PromptBuilding` | 不让 prompt 细节直接泄进 Loom 领域模型 |

### 4.2 Semantic ingress
| OpenClaw 载体 | `loom-openclaw` 动作 | 发给 Loom 的对象 | 取舍 |
| --- | --- | --- | --- |
| 专门结构化 tool/result | 捕获 `HostSemanticBundle` 并归一化 | `SemanticDecisionEnvelope` | v0 首选，最不容易回退到文本解析 |
| sidecar payload | 兼容替代并归一化 | `SemanticDecisionEnvelope` | 允许做 bridge，但不建议作为首选 |

这里几个变量的含义必须固定：
1. `HostSemanticBundle`
   - 表示宿主主模型已经完成的综合判断
   - 它可以同时包含：
     - `interactionLane`
     - `taskActivationReason`
     - `managedTaskClass`
     - `WorkHorizon`
     - change/boundary 分类
2. `SemanticDecisionEnvelope`
   - 表示 `loom-openclaw` 归一化后给 Loom 的 bounded decision
   - Loom 只消费它，不再解释自然语言
3. `HostInboundTurn`
   - 只是宿主 transport 载体
   - 进入 Loom 前必须先被 adapter 归一化成 `CurrentTurnEnvelope`
4. `HostCapabilitySnapshot`
   - 表示宿主当前 agent/model/tool/render/worker control 的真实能力快照
   - Loom 用它决定 `AgentBinding` 和 interruption 路径是否可执行

这一层固定失败策略：
1. 缺 `interactionLane`
   - `fail-open to chat`
2. managed judgment 缺 `managedTaskClass / WorkHorizon`
   - 不得静默进入 execute
3. low-confidence 或 major schema mismatch
   - 不得继续归一化成可执行治理输入

### 4.3 Control action ingress
| 用户行为 | `loom-openclaw` 动作 | 发给 Loom 的对象 |
| --- | --- | --- |
| `/loom approve` | 先按 `host_session_id` 查询当前 authoritative control surface，再按 `allowed_actions` 映射 | `ControlAction::ApproveStart` 或 `ControlAction::ApproveRequest` |
| `/loom modify ...` | 查询当前 surface 后，把显式 grammar 产出成 judgment 并映射 | `ControlAction::ModifyCandidate` |
| `/loom cancel` | 查询当前 surface 后映射 | `ControlAction::CancelCandidate` |
| `/loom keep` | 查询当前 surface 后映射 | `ControlAction::KeepCurrentTask` |
| `/loom replace` | 查询当前 surface 后映射 | `ControlAction::ReplaceActive` |
| `/loom reject` | 查询当前 surface 后映射 | `ControlAction::RejectRequest` |

这里的取舍是：
1. v0 仍然可以文本展示
2. 但传回 Loom 的必须是结构化控制动作
3. `/loom` 是正式 control surface carrier，不是按钮回调占位
4. command handler 必须先读 authoritative `CurrentControlSurfaceProjection`
   - 至少包含 `surface_type / managed_task_ref / decision_token / allowed_actions`
5. `/loom` parser 只负责把显式 grammar 产出成 `control_action` judgment
   - 它不是第二套 `ControlAction` owner
6. `loom-openclaw` 不得从自由文本直接猜动作类型
7. 例如用户只回复“继续”
   - 如果宿主没给显式 `control_action` judgment
   - adapter 必须请求宿主补判或保守退化
8. active task 动作如 `request_task_change / request_horizon_reconsideration`
   - 留到后续版本
   - 不属于当前 WebUI 第一版 grammar

### 4.4 Tool and subagent bridge
| OpenClaw hook | `loom-openclaw` 动作 | 发给 Loom 的对象 |
| --- | --- | --- |
| `before_tool_call` | 把宿主真实工具意图转成结构化观察 | `HostToolIntent` |
| `tool_result_persist` | 把宿主真实工具结果转成结构化结果 | `HostToolResult` |
| `subagent_spawned` | 报告子 agent 已生成 | `HostSubagentLifecycleEvent::Spawned` |
| `subagent_ended` | 报告子 agent 已结束 | `HostSubagentLifecycleEvent::Ended` |

### 4.5 Bridge bootstrap and capability sync
这两条不是普通用户输入，但属于 v0 必须存在的系统 ingress：
1. bridge bootstrap
   - 先完成 `BridgeBootstrapTicket -> BridgeSessionCredential` 握手
   - 没过这一步，任何语义归一化 ingress、控制动作 ingress、tool intent ingress 都不得进入 Loom
2. capability sync
   - adapter 启动时至少同步一次 `HostCapabilitySnapshot`
   - OpenClaw agent/tool 配置变化时必须重新同步
   - 否则 `ExecutionAuthorization` 和 `WorkerInterruptionRequest` 会建立在过期能力上
   - `HostCapabilitySnapshot` 的正式字段 contract 以 [宿主能力快照合同.md](宿主能力快照合同.md)
     为准
   - 能力变化后的重授权语义，统一以 [能力漂移与重授权合同.md](../治理策略/能力漂移与重授权合同.md)
     为准

---

## 5. Loom 到 `loom-openclaw` 的最小 outbound
Loom 至少要能回这些结构化结果：
1. `StartCardPayload`
   - 表示 candidate 的正式用户入口卡
2. `BoundaryCardPayload`
   - 表示 active task 与第二项重任务的边界选择卡
3. `ResultSummaryPayload`
   - 表示最终结果包的用户可见摘要
4. `SuppressHostMessagePayload`
   - 表示某条宿主原始消息不应透给用户
5. `ToolDecisionPayload`
   - 表示 Loom 对某次真实工具调用的治理结果

这些 payload 的正式字段，以：
1. [内核出站载荷合同.md](内核出站载荷合同.md)

为准。

补一条收口边界：
1. `StatusNotice`
   - 在当前 landing 中仍只作为可选最小 watchdog 扩展
   - 未进入本轮最小正式 outbound payload 集

这里最关键的两个对象是：
1. `SuppressHostMessagePayload`
   - 它解决主聊天区再次泄露内部治理旁白的问题
2. `ToolDecisionPayload`
   - 它解决 `approval-gate` 和宿主真实工具调用之间的桥接问题

补一条执行桥接边界：
1. `HostExecutionCommand`
   - 属于 Loom -> adapter 的执行派发 aggregate
   - 不属于用户可见 `KernelOutboundPayload`
2. 它的正式状态机、`ack_host_execution` 语义、以及 `HostSubagentLifecycleEnvelope`
   - 统一以 [宿主执行派发合同.md](宿主执行派发合同.md)
     为准

### 5.1 当前 v0 用户可见投递主路径
当前这版 `loom-openclaw` 对用户可见治理消息的主路径，固定为：
1. `next_outbound(host_session_id)`
2. adapter 本地把结构化 payload 渲染成宿主文本
3. 调用 `chat.inject(sessionKey=host_session_id, message=rendered_text)`
4. 宿主真正完成可见投递后，再 `ack_outbound(delivery_id)`

更具体的插件侧缓解设计，见：
1. [chat.inject影响最小化设计.md](chat.inject影响最小化设计.md)

这里几个变量分别代表：
1. `host_session_id`
   - OpenClaw 宿主聊天容器 id
2. `delivery_id`
   - Loom authoritative durable outbox 中这一次正式投递单元的稳定 id
3. `ack_outbound`
   - 宿主真正完成用户可见投递后的回写
   - 不是 adapter “读到了就 ack”

当前仍把 `chat.inject` 作为 v0 主路径，原因是：
1. 它是现有宿主里唯一同时满足“写 transcript + 广播 WebUI chat 事件”的正式接口
2. 它比 compatibility projection、日志投影或 adapter-local 假消息更接近正式用户可见主链
3. 它最符合 durable outbox 的 `delivery_id -> visible delivery -> ack_outbound` 闭环

这里也要明确当前代价：
1. start card / result summary 等可见治理消息，仍受宿主 transcript materialize 时序影响
2. 如果宿主 session entry 已存在但 transcript 文件尚未 materialize，`chat.inject` 可能失败
3. 这类失败不表示 Loom candidate/window 真相错误，而表示宿主可见投递窗口尚未就绪

### 5.2 `structured replacement` 的当前边界
`structured replacement`
1. 指的是利用宿主 `before_message_write` 等 hook，把原本要落盘的普通 assistant 消息改写成结构化治理文本

它当前能做到：
1. transcript/history 层的持久化替换
2. 普通 assistant 泄漏后的持久化修正
3. 作为 `SuppressHostMessagePayload` 后续收口的研究方向

它当前做不到：
1. 保证用户第一眼看到的第一条实时 assistant 气泡就是 start card

原因是：
1. `before_message_write`
   - 作用点在 transcript 同步写入前
2. WebUI 当前实时 assistant 可见链路
   - 来自 agent stream -> Gateway `chat` 事件 -> 前端内存状态
   - 不直接来自 transcript 回读
3. 因此 `structured replacement` 目前最多保证“历史真相正确”
   - 还不能作为 `L-02` 所要求的“第一条实时用户可见 managed 消息”主路径

当前取舍固定为：
1. 继续把 `chat.inject` 作为 v0 主路径
2. `structured replacement` 只保留为研究方向与补强路径
3. 在宿主没有新的正式实时替换接口前，不把它写成已落地主链

### 5.3 `SuppressHostMessagePayload` 的当前收口状态
`SuppressHostMessagePayload`
1. 仍然是正式 outbound contract
2. 它解决的是“宿主原始消息不应继续透给用户”的正式治理语义

但当前实现仍要如实说明：
1. adapter 里还存在 `pendingSemanticSessions / suppressAssistantSessions` 这类 adapter-local 运行态闸门
2. 它们当前承担的是宿主 hook 层的即时压制责任
3. 它们不是最终形态的正式 suppression protocol owner

因此当前必须区分两层：
1. 正式语义 owner
   - 仍是 Loom 的 `SuppressHostMessagePayload`
2. 当前宿主 hook 层运行态闸门
   - 只是 v0 现实接线中的本地补强，不应反写成 contract 已完整落地

后续仍需收口的点：
1. `host_message_ref`
   - 当前代码层尚未像合同那样稳定要求精确宿主消息引用
2. `replacement_outbound_ref`
   - 当前代码层尚未形成“被谁替换”的完整正式闭环
3. adapter 不应长期依赖本地 suppression set 充当最终协议层

---

## 6. `runtime/loom/` 的最小布局
建议 v0 先落这 5 组目录：
1. `runtime/loom/tasks/`
   - `managedTaskRef` 聚合态只读投影
2. `runtime/loom/events/`
   - 从 authoritative event store 导出的 debug/export projection
3. `runtime/loom/projections/`
   - 面向 UI/调试读取的聚合态投影
4. `runtime/loom/notices/`
   - watchdog notice projection
5. `runtime/loom/host-bridges/openclaw/`
   - `loom-openclaw` 的 host mapping、durable outbox、delivery ack、compat shadow

取舍：
1. v0 不要求一次把所有 runtime 都产品化
2. 但 authoritative truth 必须一开始就从宿主兼容投影迁出

---

## 7. 最小接口草图
### 7.1 `loom-openclaw` -> Loom
```rust
pub struct IngressMeta {
    pub ingress_id: IngressId,
    pub causation_id: Option<CausationId>,
    pub correlation_id: CorrelationId,
    pub dedupe_window: DedupeWindow,
}

pub trait LoomIngress {
    fn ingest_current_turn(&self, turn: CurrentTurnEnvelope) -> LoomResult<()>;
    fn ingest_semantic_decision(&self, decision: SemanticDecisionEnvelope) -> LoomResult<()>;
    fn ingest_control_action(&self, action: ControlAction) -> LoomResult<()>;
    fn observe_tool_intent(&self, intent: HostToolIntent) -> LoomResult<ToolDecisionPayload>;
    fn observe_tool_result(&self, result: HostToolResult) -> LoomResult<()>;
    fn ingest_subagent_lifecycle(&self, event: HostSubagentLifecycleEnvelope) -> LoomResult<()>;
    fn sync_capabilities(&self, snapshot: HostCapabilitySnapshot) -> LoomResult<()>;
}
```

### 7.2 Loom -> `loom-openclaw`
```rust
pub trait HostDelivery {
    fn next_outbound(&self, host_session_id: HostSessionId) -> LoomResult<Vec<OutboundDelivery>>;
    fn ack_outbound(&self, delivery_id: OutboundDeliveryId) -> LoomResult<()>;
    fn next_host_execution(&self, host_session_id: HostSessionId) -> LoomResult<Option<HostExecutionCommand>>;
    fn ack_host_execution(&self, command_id: HostExecutionCommandId) -> LoomResult<()>;
    fn read_current_control_surface(&self, host_session_id: HostSessionId) -> LoomResult<Option<CurrentControlSurfaceProjection>>;
    fn read_runtime_projection(&self, managed_task_ref: ManagedTaskRef) -> LoomResult<CompatibilityProjection>;
}
```

这里的逻辑是：
1. ingress 负责“把 adapter 已归一化的正式对象送进 Loom”
2. delivery 负责“把 Loom 的结构化 payload 连同 `delivery_id` 一起送回宿主”
3. 两边都不重新做语义判断
4. 所有宿主 transport ingress 都必须先带 `IngressMeta` 进入 adapter 采集层，再归一化成正式对象
5. outbound 必须先落 authoritative store/outbox，再允许 adapter 读取与投递
6. `ack_outbound` 只在宿主真正完成投递后写回
7. `next_host_execution` 只返回 Loom authoritative command queue 中仍待派发的命令
8. `read_current_control_surface`
   - 只按 `host_session_id` 读取 Loom authoritative open window
   - 返回 `surface_type / managed_task_ref / decision_token / allowed_actions`
9. query 结果为 `0` 个或 `>1` 个 open window 时
   - command ingress 必须 fail closed
8. `ack_host_execution` 只在宿主真正接受 dispatch 后写回
9. `LocalHttpBridge` 形态下，请求还必须通过 loopback-only + shared bridge secret 校验
10. 具体 delivery 状态机、retry、expired、terminal failure 以
   [出站投递生命周期合同.md](出站投递生命周期合同.md)
   为准
11. 具体 host execution 状态机、迟到 lifecycle、dispatch 不确定重放以
    [宿主执行派发合同.md](宿主执行派发合同.md)
    为准

固定分层：
1. `HostInboundTurn / HostSemanticBundleIngress`
   - 仍属于宿主 transport 采集层
2. `CurrentTurnEnvelope / SemanticDecisionEnvelope / ControlAction`
   - 才是 Loom 主链正式消费对象

### 7.3 outbox 顺序约束
这轮再补一条运行时边界：
1. Loom 必须先提交：
   - task/store 变更
   - 对应 `TaskEvent`
   - 对应 outbound item
2. 然后 `loom-openclaw` 再读取未 ack 的 outbound 并投递宿主
3. 宿主投递成功后，adapter 再调用 `ack_outbound`

原因：
1. 否则会出现“start card 已发出，但 authoritative state 还没提交”
2. 一旦宿主重启或 bridge 重试，就会看到重复卡片或丢状态

---

## 8. FailurePolicy 在 landing 中怎么落
### 8.1 缺 `interactionLane`
规则：
1. `loom-openclaw` 不激活 Loom managed lane
2. 保守退回 chat
3. 不创建 `managedTaskRef`

### 8.2 已进入 managed 但缺 `managedTaskClass / WorkHorizon`
规则：
1. 允许一次自动重判
2. 第二次仍缺：
   - 不进入 execute
   - 不生成看似完整 start card
   - 返回补判/澄清

### 8.3 major schema mismatch
规则：
1. 直接 fail closed
2. 不推进 Loom 治理分支
3. 记录一条 adapter-local semantic ingress rejection

---

## 9. 我的建议
### 9.1 先把 `loom-openclaw` 做成薄桥
我建议第一版只做 5 件事：
1. 宿主事实 ingress
2. 宿主语义 ingress -> `SemanticDecisionEnvelope`
3. `/loom` 显式 control surface ingress -> `ControlAction`
4. Loom outbound 文本化渲染
5. 真实工具/子 agent 执行桥接

### 9.2 不要在 `loom-openclaw` 里重新长出治理引擎
不要把它做成：
1. 第二个 lane classifier
2. 第二个 task owner
3. 第二个 approval owner
4. 第二个 watchdog owner

否则后面再好的 Loom 设计都会被宿主 glue 层重新污染。
