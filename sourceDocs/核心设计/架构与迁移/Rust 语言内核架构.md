# Rust 语言内核架构

状态：architecture draft  
定位：Loom 的 Rust 分层与 owner 边界草案  
更新时间：2026-03-10

---

## 1. 目的
这份文档回答 3 个问题：
1. Rust 内核应该怎么拆 crate
2. 哪些对象必须由内核持有真相源
3. 在 OpenClaw 仍是唯一宿主的 v0 阶段，如何避免 Rust kernel 和现有 TypeScript 插件同时成为治理 owner

更深层上，它解决的是：

**先把 owner 边界钉死，再决定部署形态。**

---

## 2. 当前现实为什么不能直接开写
### 2.1 OpenClaw 今天已经给了很多宿主能力
从当前代码看，OpenClaw 已经提供了较厚的宿主接入面：
1. hook 事件
   - `message_received`
   - `before_agent_start`
   - `before_prompt_build`
   - `message_sending`
   - `before_message_write`
   - `before_tool_call`
   - `tool_result_persist`
   - `subagent_spawned`
   - `subagent_ended`
2. 宿主 agent / model / tool / subagent 配置
   - 当前主要来自 [openclaw.json](../../../openclaw.json)
3. runtime 文件根
   - `~/.openclaw/runtime`
4. 主动子 agent 能力
   - `sessions_spawn`
   - `sessions_send`

这意味着：
1. OpenClaw 不是“能力不够”
2. 真问题是“能力已经很多，但治理 owner 还没切开”

### 2.2 现有三插件已经各自持有部分治理事实
今天的 TypeScript reality 里：
1. 旧治理插件
   - 通过 `currentTurnStore / sessionContextStore / bindingStore / taskRuntimeStore` 持有任务与会话治理状态
2. `watchdog`
   - 通过读取宿主兼容任务投影跟随任务阶段
3. `approval-gate`
   - 通过读取 runtime session context 和 `executionAuthorization` 跟随授权状态

这里几个变量非常关键：
1. `CurrentTurnEnvelope`
   - 表示最近一条正式入站 turn 的结构化包
   - 它解决“当前用户输入是谁”
2. `PendingUserDecision`
   - 表示当前等待消费的结构化用户决策
   - 它解决“用户已经回复了什么，系统应该按哪类决策消费”
3. `PendingBoundaryConfirmation`
   - 表示 active task 存在时，又来了第二项重任务候选的边界窗口
4. `ExecutionAuthorization`
   - 表示当前运行时真实获得了哪些能力
   - 它不是抽象授权语义，而是本次执行真的能用什么

### 2.3 如果不先做 cutline，会出现双真相源
更广层面的问题不是“crate 怎么拆”，而是：
1. Rust kernel 如果直接开始持有 `managedTaskRef / PhasePlan / AcceptancePolicy`
2. TypeScript 插件又继续写 `sessionContextStore /` 宿主兼容任务投影

那么系统就会同时存在两套 owner：
1. Rust 以为自己是任务真相源
2. TS runtime 仍在以 session 文件当真相源

这就是最危险的冲突。

---

## 3. v0 总体建议
### 3.1 先协议优先，再决定部署方式
你已经明确：
1. daemon / 嵌入式的最终形态要看 adapter 和 OpenClaw 现实能力
2. 不能先凭空假设

所以 v0 最稳的架构取舍是：
1. **先把 kernel 当成独立 owner 设计**
2. **先把 adapter 当成宿主边界层设计**
3. **部署方式保持 adapter-contingent**

换句话说：
1. 逻辑上按 daemon-ready 设计
2. 物理上允许 v0 先用最容易落地的 bridge 方式接 OpenClaw

### 3.2 v0 采用“单治理真相源 + `loom-openclaw` 薄适配层”
我的建议：
1. Rust kernel 是 Loom 的治理真相源
2. `loom-openclaw` 是宿主映射层
3. 现有 TS 插件先降级成 adapter glue / projection bridge

这不是说 v0 一天内要把所有 TS 代码删掉，而是说 owner 必须先切：
1. 任务真相归 kernel
2. 宿主映射归 adapter
3. 兼容投影才允许留在 TS

### 3.3 kernel 不做语义理解，只消费结构化判断
这是这轮新增的重要边界。

规则是：
1. 宿主大模型负责语义判断
2. adapter 负责把判断结果转成 `SemanticDecisionEnvelope`
3. kernel 只做 schema 校验、状态变更和治理执行

这意味着：
1. kernel 不是第二个语义分类器
2. kernel 不是 prompt router
3. kernel 是治理执行器

---

## 4. 内核必须持有的正式对象
这些对象必须由 Rust kernel 持有 authoritative truth：
1. `interactionLane`
   - 表示当前输入留在 chat，还是进入 managed task 路径
2. `managedTaskRef`
   - 单个受管任务的一等对象 id
3. `activeManagedTaskRef`
   - 当前 session 下唯一活跃任务的轻量指针
4. `managedTaskClass`
   - `COMPLEX / HUGE / MAX`
5. `PhasePlan`
   - 这次任务真正采用的阶段方案
6. `IsolatedTaskRun`
   - 当前执行期运行单元；当前阶段执行状态作为它的从属运行态
7. `WorkHorizon`
   - 当前任务/阶段的投资意图
8. `WipPolicy`
   - 活跃数量、老化、升级规则
9. `DelegationLevel`
   - 抽象授权语义
10. `ExecutionAuthorization`
    - 本次真实授权结果
11. `AcceptancePolicy`
    - 完成定义、review、返工
12. `ReworkPolicy`
    - 打回后的回退和返工规则
13. `BudgetPolicy`
    - token / 时间 / agent / 模型等预算约束
14. `TaskEventModel`
    - 全部运行线共享的正式事件链
15. `ControlAction`
   - 运行中用户能施加哪些正式控制

原因：
1. 这些对象一旦分散在多个插件里，后面 recorder、watchdog、approval-gate 都会各自读不同事实
2. 这些对象共同组成了“治理”本身

---

## 5. adapter 必须持有的正式对象
这些对象更适合由 OpenClaw adapter 持有：
1. `HostSessionRef`
   - 宿主聊天容器身份
2. `HostMessageRef`
   - 宿主消息身份
3. `HostCapabilitySnapshot`
   - 当前宿主有哪些 agent / model / tool / memory 能力
4. `SemanticDecisionEnvelope`
   - 宿主大模型已经完成的结构化语义判断
5. `HostMappingRegistry`
   - 内核抽象对象与 OpenClaw 真实 agent / model / tool 的映射
6. `RenderedTextPayload`
   - 把 kernel 结构化语义渲染成宿主文本消息的结果
7. `CompatibilityProjection`
   - v0 兼容 legacy TS 插件时，向旧 runtime 文件投影的兼容层

原因：
1. 这些对象本质上是宿主现实，不是治理现实
2. 它们应该随着宿主变化而变化，而不是写死在 kernel

---

## 6. crate 分层建议
### 6.1 `loom-domain`
它负责：
1. 基础 id、enum、aggregate type
2. `interactionLane / managedTaskClass / WorkHorizonKind / DelegationLevel`
3. 各类引用对象和 payload

建议：
1. 只放纯类型和不依赖 I/O 的领域规则

### 6.2 `loom-task`
它负责：
1. `managedTaskRef`
2. `activeManagedTaskRef`
3. task aggregate
4. task-scoped `workflowStage`

这里的 `workflowStage` 是什么：
1. 单个受管任务当前处在哪个阶段
2. 它不再是 session-scoped 的聊天状态

### 6.3 `loom-phase`
它负责：
1. `StagePackage`
2. `PhasePlan`
3. 当前阶段执行状态
4. 阶段推进与返工回退

这层要特别守住边界：
1. `StagePackage` 是模板
2. `PhasePlan` 是这次选了哪些阶段
3. 当前阶段执行状态只作为 `IsolatedTaskRun` 下的从属运行态存在

### 6.4 `loom-harness`
它负责：
1. `AgentCapabilityProfile`
2. `AgentBinding`
3. `review_group / validate_group / recorder` 等系统保留角色
4. 从 `AgentPool` 选出当前任务/阶段 roster

### 6.5 `loom-policy`
它负责：
1. `AcceptancePolicy`
2. `ReworkPolicy`
3. `BudgetPolicy`
4. `WipPolicy`
5. `DelegationLevel`

更深层上，这层是“治理策略集合”，不是执行器。

### 6.6 `loom-approval`
它负责：
1. 把 `DelegationLevel` 与 `ExecutionAuthorization` 收成可执法决策
2. 给 adapter / host executor 返回：
   - 放行
   - 阻断
   - 需要用户确认

这里要强调：
1. `DelegationLevel`
   - 表示抽象授权语义
2. `ExecutionAuthorization`
   - 表示本轮真正被授予的执行能力
3. `approval-gate`
   - 最终看的是后者

### 6.7 `loom-watchdog`
它负责：
1. 消费 `TaskEventModel`
2. 读取 `WipPolicy / BudgetPolicy / ReworkPolicy`
3. 判断何时：
   - 发进度通知
   - 发卡点通知
   - 升级给用户

### 6.8 `loom-events`
它负责：
1. `TaskEventModel`
2. append-only 事件写入
3. event projection contract

建议：
1. 不做“只有 event sourcing 没有 projection”的极端设计
2. 保留事件链为事实留痕
3. 同时保留可读取的聚合状态投影

### 6.9 `loom-store`
它负责：
1. SQLite / 本地存储
2. aggregate snapshot
3. event append
4. query projection

建议：
1. v0 优先 `SQLite + append-only task events`
2. `runtime/loom/events/` 只做导出/调试 projection，不做 authoritative event source

### 6.10 `loom-bridge`
它负责：
1. adapter 与 kernel 的协议定义
2. transport 无关的请求/响应/event schema

这里要先把“逻辑协议”与“物理 transport”分开。

---

## 7. transport 与部署形态建议
### 7.1 当前不先写死 daemon 或嵌入式
因为你已经明确：
1. 这取决于 adapter 和 OpenClaw 实际给了什么能力
2. 不能先拍脑袋

所以我建议：
1. 先固定 `kernel-bridge` 协议
2. transport 做成可替换

### 7.2 v0 可接受的 transport 形态
建议保留 3 种实现位点：
1. `InProcessBridge`
   - 最容易做实验
2. `LocalHttpBridge`
   - 最容易调试和跨语言
3. `UnixSocketBridge`
   - 更适合本地长期运行

取舍：
1. v0 不需要先做 gRPC
2. v0 不应该把 kernel 重新塞回 TypeScript 插件内部，逻辑上仍要保持分层

### 7.3 我的建议
1. **逻辑上按 sidecar daemon 设计**
2. **工程上允许 v0 先做 bridge-first 落地**
3. **代码 spike 先选 `LocalHttpBridge`**

这样既不把自己锁死在 OpenClaw 插件里，也不在第一天就被部署细节拖慢。

---

## 8. 当前 TypeScript reality 到 Rust crate 的映射建议
### 8.1 `currentTurnStore`
它是什么：
1. 当前入站 turn 的 journal / lookup store

建议迁移：
1. 它的长期 owner 应是 adapter ingress journal
2. kernel 只消费结构化入站 turn，不把宿主 turn 文件格式当正式内核对象

### 8.2 `sessionContextStore`
它是什么：
1. 当前 session-scoped runtime projection

建议迁移：
1. 降级为 compatibility projection
2. 不再当作 managed task 的正式 truth

### 8.3 `bindingStore`
它是什么：
1. session alias 与 runtime context/binding 的恢复层

建议迁移：
1. 拆成两部分：
   - adapter 的 host identity mapping
   - kernel 的 task-to-session projection

### 8.4 `taskRuntimeStore`
它是什么：
1. 任务运行时与 close 行为的当前 TS owner

建议迁移：
1. 这是最该迁进 Rust `kernel-task` 的一层

### 8.5 `taskEventSink`
它是什么：
1. 当前 TS 中 append task event 的端口

建议迁移：
1. 直接进入 `kernel-events`

### 8.6 `compatProjectionWriter`
它是什么：
1. 当前事件与 close request 文件写入器

建议迁移：
1. 留下兼容投影
2. 真正的 task event append 转到 Rust

---

## 9. 推荐的 v0 owner 切法
### 9.1 Kernel 先接管什么
我的建议：
1. 先接管治理真相源
2. 不强求第一天接管宿主执行细节

也就是 v0 先让 kernel 成为：
1. 任务 owner
2. 阶段 owner
3. 授权 owner
4. 事件 owner
5. 通知决策 owner

### 9.2 OpenClaw adapter 先保留什么
adapter 先保留：
1. hook 订阅
2. host identity 解析
3. host capability 发现
4. 文本渲染
5. tool / subagent 真实执行桥接
6. 向 legacy runtime 文件写兼容投影

这条取舍的好处是：
1. 不会因为 v0 就重写整个 OpenClaw 执行层
2. 但治理真相已经开始从 TS 迁出

---

## 10. 我的问题与建议
### 10.1 还没完全锁死的点
有 2 个点现在仍然是 open question：
1. kernel 最终是否常驻 daemon
2. adapter 与 kernel 的物理 transport 最终选哪种

这两个点之所以还没直接定死，不是因为不重要，而是因为：
1. 你已经明确要先看 OpenClaw adapter 现实
2. 这是正确取舍

### 10.2 我的建议
1. 现在就把 crate 分层写成 **daemon-ready**
2. 现在不要把 transport 和部署方式写成硬依赖
3. 把真正不可退让的东西先定死：
   - 单治理真相源
   - task owner 和 session owner 分离
   - adapter 只是宿主边界，不是治理引擎

这 3 条比“今天到底是 HTTP 还是 socket”重要得多。
