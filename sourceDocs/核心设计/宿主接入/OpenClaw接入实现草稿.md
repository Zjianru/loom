# OpenClaw接入实现草稿

状态：implementation draft  
定位：`loom-openclaw` + Loom 的第一条真实闭环施工稿  
更新时间：2026-03-11

---

## 1. 目标
这份文档不是再描述理想架构，而是回答：
1. 第一条 spike 先做哪条链
2. 每一步要落哪些最小接口
3. 哪些地方允许借旧代码的 hook 面
4. 哪些地方绝不能再借旧运行时投影当真相源

这轮固定目标只有一条：

**打通 `COMPLEX` 的最小闭环：用户启动任务 -> 宿主产出结构化语义 -> Loom 建 candidate -> 用户批准 -> Harness 委派单 worker -> 经过最小 review -> recorder 输出结果摘要 -> 文本回到 OpenClaw。**

这条最小闭环现在额外固定依赖：
1. [编码工作模式预设.md](../工作模式与结果/编码工作模式预设.md)
2. [结果样例.md](../工作模式与结果/结果样例.md)
3. [评审结果合同.md](../工作模式与结果/评审结果合同.md)
4. [规格包合同.md](../任务与协作/规格包合同.md)
5. [工作证明合同.md](../工作模式与结果/工作证明合同.md)
6. [隔离任务运行合同.md](../任务与协作/隔离任务运行合同.md)

---

## 2. 这条 spike 为什么先做 `COMPLEX`
`managedTaskClass`
- 它表示 managed lane 内采用哪种协作拓扑和治理强度
- 正式值是：
  - `COMPLEX`
  - `HUGE`
  - `MAX`

我建议先做 `COMPLEX`，原因是：
1. 它已经足够验证“任务真的被扔出去了”
2. 它需要：
   - `managedTaskRef`
   - `PhasePlan`
   - `AgentBinding`
   - `ResultSummaryPayload`
3. 但它还不会立刻把 spike 拖进多阶段并行和大规模 review 组

这条链里的最小角色固定为：
1. `net`
   - 宿主主 assistant，继续保持聊天自由
2. `recorder`
   - 正式记录角色，负责阶段纪要和最终摘要素材
3. `worker`
   - 一个真实专业 agent

这里的取舍是：
1. `review_group` 第一条 spike 可以先是轻量 review，不一定先落完整 reviewer group
2. 但 `review` 阶段必须存在
3. `watchdog` 只做最小 `StatusNotice`
4. review 阶段至少要落一个正式 `ReviewResult`
5. 默认 pack 固定为 `coding_pack`

---

## 3. 第一条闭环的正式范围
### 3.1 In scope
1. 用户输入一条明确的 managed task 请求
2. OpenClaw 主代理产出 `HostSemanticBundle`
3. `loom-openclaw` 归一化出 `SemanticDecisionEnvelope`
4. Loom 创建 candidate `managedTaskRef`
5. Loom 输出 `managed task start card`
6. 用户输入 `/loom approve`
7. Loom 生成 `SpecBundle`
8. Loom 创建 `IsolatedTaskRun`
9. Harness 绑定 `recorder + 1 worker`
10. worker 完成核心工作
11. Loom 进入最小 `review` 阶段
12. recorder 生成阶段纪要和结果摘要素材
13. Loom 编译 `ProofOfWorkBundle`
14. Loom 输出 `ResultSummaryPayload`
15. `loom-openclaw` 文本化并投递给宿主聊天区
16. 默认阶段骨架和结果渲染必须分别受 pack preset 与 result examples 约束

### 3.2 Out of scope
1. `HUGE / MAX`
2. 多阶段串行之外的复杂 phase graph
3. 多 reviewer / validate group
4. 完整 watchdog cadence
5. pack marketplace
6. 自动改 `openclaw.json`
7. `research_pack` 进入第一条代码主闭环

---

## 4. 关键对象在这条 spike 里分别代表什么
### 4.1 `host_session_id`
它是什么：
1. OpenClaw 聊天容器 id

它代表：
1. 用户当前在哪个宿主会话里聊天

它不代表：
1. 任务 owner

### 4.2 `managedTaskRef`
它是什么：
1. Loom 中单个受管任务的正式 owner

它代表：
1. start card
2. 当前 `PhasePlan`
3. 当前 `ExecutionAuthorization`
4. recorder 产物
5. 最终 `ResultSummaryPayload`

### 4.3 `HostSemanticBundle`
它是什么：
1. 宿主主模型已经完成的综合结构化判断包

这一条闭环里，它至少要给出：
1. `interactionLane=managed_task_candidate`
2. `taskActivationReason`
3. `managedTaskClass=complex`
4. `WorkHorizon`

### 4.4 `SemanticDecisionEnvelope`
它是什么：
1. `loom-openclaw` 归一化给 Loom 的 bounded decision

它代表：
1. Loom 已经可以直接消费的治理输入

### 4.5 `PhasePlan`
它是什么：
1. 这次任务实际采用的阶段方案

这条 spike 里建议最小固定为：
1. `clarify`
2. `execute`
3. `review`
4. `deliver`

原因：
1. `COMPLEX` 先验证委派闭环
2. 不在第一条 spike 里引入多阶段复杂治理
3. 具体默认骨架以 `编码工作模式预设` 为准，不允许实现层另写一套“方便版本”
4. 这里的 `clarify / execute / review / deliver`
   - 都是 `PhasePlan` 中的细粒度 `StagePackageId`
   - 不是顶层 `workflowStage` 扩展枚举

### 4.6 `SpecBundle`
它是什么：
1. 这次 `coding_pack / COMPLEX` spike 在批准启动后必须生成的正式执行锚点文档组

它代表：
1. `scope_doc`
2. `plan_doc`
3. `verification_doc`

它为什么必须出现在第一条 spike：
1. 否则 pack preset 里的默认 spec 形状会重新被实现层内嵌

### 4.7 `IsolatedTaskRun`
它是什么：
1. 当前 `managedTaskRef` 在执行期绑定的一次独立受管 run

它代表：
1. run id
2. run artifacts
3. run evidence
4. run completion / failure

它为什么必须出现在第一条 spike：
1. 这样 `managedTaskRef`、宿主 session、执行租约、最终结果包才能分层

### 4.8 `ProofOfWorkBundle`
它是什么：
1. 这条最小闭环在 `review -> deliver` 之间必须编译出的正式证据包

它代表：
1. `run_summary`
2. `evidence_refs`
3. `review_summary`
4. `artifact_manifest`
5. `acceptance_basis`

它为什么必须出现在第一条 spike：
1. 否则结果链会退化成只回一段 summary，而不是正式交付

---

## 5. 建议的落地切片
### Slice 0: 新 runtime 根与命名落位
做什么：
1. 在设计稿和实现草图里正式使用：
   - `Loom`
   - `Harness`
   - `runtime/loom/`
   - `loom-openclaw`
2. 宿主兼容投影继续只做 compatibility projection

为什么先做这个：
1. 不先切命名和 runtime 根，后面所有代码都会继续沿用旧运行时 owner 幻觉

完成标志：
1. 新实现草图和新接口稿都不再把宿主兼容投影当 authoritative truth

### Slice 0.5: bridge bootstrap flow
做什么：
1. Loom 启动 `LocalHttpBridge`
2. 生成 `bridge_instance_id`
3. 生成一次性 `BridgeBootstrapTicket`
4. `loom-openclaw` 先读取绝对路径 `bridge.runtimeRoot`
5. bootstrap ticket 路径固定由 `join(bridge.runtimeRoot, "loom/bootstrap/openclaw/bootstrap-ticket.json")` 派生
6. adapter 通过受限本地 bootstrap 渠道拿到 ticket
7. adapter 完成 bootstrap handshake，换取 `BridgeSessionCredential`
8. 后续 ingress 全部改用 session secret + `rotation_epoch`

取舍：
1. v0 不先做复杂证书体系
2. 但也绝不接受匿名本地请求
3. 不把 bootstrap ticket 单文件路径做成独立长期配置
4. Loom runtime 本地文件统一从 `bridge.runtimeRoot` 派生

完成标志：
1. 未 bootstrap 的 adapter 不能调用任何 Loom ingress
2. bridge 重启后能触发重新 bootstrap，而不是继续沿用旧 secret
3. 安装态不依赖 `cwd` 就能稳定找到 bootstrap ticket

### Slice 1: `HostSemanticBundle` ingress
做什么：
1. 在 OpenClaw 主代理侧约定一个显式结构化语义输出通道
2. `LocalHttpBridge` 只绑定 loopback，并要求 shared bridge secret
3. `loom-openclaw` 能捕获：
   - `ingress_id`
   - `causation_id`
   - `correlation_id`
   - `dedupe_window`
   - `schema_version`
   - `DecisionSource`
   - per-decision `confidence`
   - decision payload
4. 归一化出 `SemanticDecisionEnvelope[]`

取舍：
1. 只接受显式结构化载体
2. 不从自然语言反解析

完成标志：
1. 一条 `COMPLEX` 启动请求能稳定产出可消费的 bundle
2. 未带 bridge secret 的本地请求不会进入 Loom

### Slice 1.5: worker control capability sync
做什么：
1. `loom-openclaw` 启动时读取宿主 agent/tool/runtime 现实能力
2. 明确同步 `HostCapabilitySnapshot.worker_control_capabilities`
3. Loom 持久化当前 capability snapshot
4. 后续 capability 变化时通过 `sync_capabilities` 重发最新快照
5. `workspace_ref / readable_roots / writable_roots / gateway call cwd` 全部从宿主 `host workspace root` 派生

这里几个变量分别代表：
1. `HostCapabilitySnapshot`
   - 宿主总体能力快照
2. `HostWorkerControlCapabilities`
   - 宿主对正在运行 worker 的 pause/resume/cancel/interrupt 真实支持情况
3. `ExecutionAuthorization`
   - Loom 根据这些事实能力发放的真实执行租约
4. `host workspace root`
   - 宿主当前 agent / runtime context 提供的工作区绝对根路径
   - 不得从 `cwd` 猜

完成标志：
1. Harness 在做 `AgentBinding` 前能拿到最新 capability snapshot
2. pause/cancel 路径不再建立在“宿主应该支持”这种假设上

### Slice 2: candidate task creation
做什么：
1. Loom 收到 `interactionLane=managed_task_candidate`
2. 校验 `managedTaskClass / WorkHorizon / taskActivationReason`
3. 创建 candidate `managedTaskRef`
4. 在 Loom authoritative store 持久化第一批 task/event truth
5. 再导出：
   - `runtime/loom/tasks/` projection
   - `runtime/loom/events/` debug/export projection

更深层要守住的边界：
1. `host_session_id`
   - 只存宿主容器映射
2. `managedTaskRef`
   - 才是任务 owner

完成标志：
1. start card 不依赖宿主兼容投影任务文件
2. projection 文件缺失时，authoritative task truth 仍然成立
3. candidate 出站 payload 形状以 [内核出站载荷合同.md](内核出站载荷合同.md) 为准
4. Loom 不会在 candidate 阶段提前创建 `IsolatedTaskRun`

### Slice 3: `/loom approve` -> `approve_start`
做什么：
1. 用户在宿主聊天区输入 `/loom approve`
2. command handler 先按 `host_session_id` 读取当前 authoritative control surface
3. `/loom` parser 把显式 grammar 结构化成 `control_action` judgment
4. `loom-openclaw` 把它映射成 `ControlAction::ApproveStart`
5. Loom 更新：
   - `activeManagedTaskRef`
   - `pendingUserDecision`
   - `workflowStage`
   - 并校验 `decision_token`
6. Loom 按 `编码工作模式预设` 的稳定模板 id `coding.spec.full.v0` 生成 `SpecBundle`
7. Loom 创建 `IsolatedTaskRun`

取舍：
1. v0 仍然文本展示
2. 回传动作必须结构化
3. token 缺失或过期时，必须 fail closed

完成标志：
1. task 从 candidate 稳定进入 active
2. 迟到或重复的 `/loom approve` 会被 `decision_token + ingress_id` 挡住
3. `approve_start` 路径不会反向修改 preset 里的默认阶段骨架
4. `SpecBundle` 与 `IsolatedTaskRun` 在这一切片后正式存在

### Slice 4: Harness binding
做什么：
1. Harness 读取：
   - `managedTaskClass=complex`
   - 当前 `PhasePlan`
   - 当前 `HostCapabilitySnapshot`
2. 固定绑定：
   - 1 recorder
   - 1 worker
3. 写入 `AgentBinding`

这里的变量各自代表：
1. `AgentCapabilityProfile`
   - 单个 agent 会什么、成本和风险怎样
2. `AgentBinding`
   - 这次真正绑了谁
3. `ExecutionAuthorization`
   - 这次执行真正允许用哪些能力

完成标志：
1. Harness 不再沿用旧会话投影 store 当 owner
2. `HostWorkerControlCapabilities` 已进入后续 pause/cancel 决策面
3. capability 漂移后的收紧或重发 authorization，按 [能力漂移与重授权合同.md](../治理策略/能力漂移与重授权合同.md) 执行

### Slice 4.5: host execution dispatch lifecycle
做什么：
1. Loom 为 worker / recorder 创建正式 `HostExecutionCommand`
2. adapter 只读取 `pending` command
3. 宿主真正接受 dispatch 后，adapter 再回 `ack_host_execution`
4. `subagent_spawned / subagent_ended` 回到 Loom 时，统一走 `HostSubagentLifecycleEnvelope`

这里几个变量分别代表：
1. `HostExecutionCommand`
   - Loom authoritative 派发命令
2. `ack_host_execution`
   - 宿主已接受 dispatch 的正式确认动作
3. `helperSessionKey`
   - OpenClaw 当前实现里的 adapter-local dispatch 句柄
   - 不是正式对象
4. `childSessionKey`
   - 宿主真实 child execution handle
   - 正式映射到 `host_child_execution_ref`
   - 是后续 lifecycle 的宿主事实锚点

取舍：
1. v0 先采用 `at-least-once + command_id` 去重
2. 不先引入 claim / release
3. dispatch 失败前，命令不得脱离 `pending`

完成标志：
1. `chat.send` 或等价宿主 dispatch 失败时，不会提前丢命令
2. dispatch 成功但 ack 迟到时，`Spawned / Ended` 仍能把状态推进到正确阶段
3. 具体状态机与失败矩阵遵循 [宿主执行派发合同.md](宿主执行派发合同.md)

### Slice 5: worker execution + minimal review + recorder summary
做什么：
1. worker 在当前阶段执行工作
2. Loom 通过 `HostSubagentLifecycleEnvelope::Ended` 收到 worker 终态
3. Loom 进入最小 `review` 阶段
4. review 至少检查：
   - 当前输出是否满足 candidate 目标
   - 是否存在明显未闭环风险
5. review 必须至少产出一个正式 `ReviewResult`
6. recorder 记录：
   - 关键输入
   - 关键产出
   - 阶段结论
7. Loom 生成结构化 `ResultSummaryPayload`

取舍：
1. recorder 作为真实 agent 上场
2. review 阶段先存在，review_group 可以后补成真实 agent 组

完成标志：
1. 最终结果不是 raw logs，而是正式摘要
2. `review` 不再只是流程名词，而是有正式 `ReviewResult`，且其中内嵌 `ReviewSummary`
3. `ResultSummaryPayload` 至少覆盖 `结果样例` 里的 `coding_pack / COMPLEX / completed` 形状

### Slice 6: minimal watchdog notice
做什么：
1. 只发两类 `StatusNotice`
   - stage entered
   - blocked

取舍：
1. 先证明 notice 可桥接
2. 不在第一条 spike 里引入完整 cadence

完成标志：
1. 至少能看到一次最小进展通知，不污染主聊天区

### Slice 7: durable outbox and replay
做什么：
1. Loom 为 outbound 建最小 durable outbox
2. `loom-openclaw` 只读取未 ack 的 outbound
3. 宿主投递成功后回写 `ack_outbound`
4. 重启后可按 `delivery_id + correlation_id` 恢复未完成投递

完成标志：
1. bridge 重启不会导致 start card 或 result summary 静默丢失
2. 同一 delivery 不会因为重放被当成新治理动作再次执行
3. retry、expired、terminal failure 均遵循 [出站投递生命周期合同.md](出站投递生命周期合同.md)

---

## 6. OpenClaw 侧建议落点
### 6.1 宿主插件
建议把第一版 bridge 收在：
1. `loom-openclaw`

它负责：
1. 监听 OpenClaw hook
2. 接住 `HostSemanticBundle`
3. 发 ingress 给 Loom
4. 把 Loom outbound 渲染成宿主文本

### 6.2 Legacy 代码的借鉴边界
可以借：
1. hook 注册方式
2. host capability 读取方式
3. tool/subagent lifecycle 观察方式

不能借成真相源的：
1. 宿主兼容投影任务文件
2. `sessionContextStore`
3. `taskRuntimeStore`
4. 旧会话型 task owner 逻辑

---

## 7. Spike 成功标准
这条 spike 至少满足 7 条才算通过：
1. OpenClaw 能产出并传递 `HostSemanticBundle`
2. `loom-openclaw` 能归一化出 `SemanticDecisionEnvelope`
3. Loom 不读取原始自然语言就能生成 start card
4. `host_session_id` 与 `managedTaskRef` 已明确分离
5. `/loom approve` 能 authoritative 地消费当前 start card，并归一化成 `approve_start` 回到 Loom
6. Harness 能完成 `recorder + 1 worker` 的最小绑定
7. 最终 `ResultSummaryPayload` 能文本化回到主聊天区

---

## 8. 我的建议
### 8.1 先做对 owner，再做大功能
第一条 spike 的价值不在于看上去“很强”，而在于：
1. Loom 真成了 task truth
2. `loom-openclaw` 真成了宿主薄桥
3. 旧运行时投影真降成了 compatibility projection

### 8.2 如果第一条 spike 失败，优先查这 4 处
1. `HostSemanticBundle` 载体是否稳定
2. `loom-openclaw` 是否偷偷重新做了语义判断
3. Loom 是否还在依赖宿主兼容投影文件
4. `host_session_id` 与 `managedTaskRef` 是否又被混回去了

不要先怀疑：
1. pack 不够丰富
2. worker 不够多
3. `HUGE / MAX` 还没上

因为第一条 spike 要证明的不是“能力上限”，而是“边界终于是对的”。
