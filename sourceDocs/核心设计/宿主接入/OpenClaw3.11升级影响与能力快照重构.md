# OpenClaw 3.11升级影响与能力快照重构

状态：design note  
定位：`OpenClaw v2026.3.11` 对 Loom 接入和 `HostCapabilitySnapshot` 的影响整理  
更新时间：2026-03-12

---

## 1. 背景
本地参考宿主已升级到：
1. OpenClaw CLI `2026.3.11`
2. 参考源码 tag `v2026.3.11`
3. 对应提交 `29dc65403f`

这次整理不是单纯记 changelog，而是回答两个业务问题：
1. `OpenClaw 3.11` 到底改变了哪些 Loom 真的依赖的运行时事实
2. 这些变化为什么逼着我们重做 `HostCapabilitySnapshot` 的建模

---

## 2. 哪些上游变化和 Loom 真正相关
### 2.1 `sessions_spawn(runtime="acp")` 新增 `resumeSessionId`
它表示：
1. ACP child session 不一定总是新开
2. 它可以恢复一个已有 ACP runtime 会话

对 Loom 的意义：
1. spawn 能力不再只是“能不能 spawn”
2. 还要区分：
   - `runtime=subagent`
   - `runtime=acp`
   - 是否支持 resume existing session
3. ACP policy 中 `allowedAgents` 为空
   - 不是“没有 agent 可用”
   - 而是“默认允许全部 agent”
4. 因此 `spawn_capabilities[]`
   - 还必须显式表达 host agent scope 是 `All / ExplicitList / None / Unknown`

### 2.2 subagent authority 被持久化
OpenClaw 3.11 现在把这些事实固化到 session lineage 里：
1. 当前会话是 `main / orchestrator / leaf`
2. 当前会话的 control scope 是 `children / none`

对 Loom 的意义：
1. worker 派发能不能继续向下 spawn
2. 当前宿主会话能不能控制 children
3. 恢复后的会话会不会重新拿回 orchestrator 权限
4. 这些值是不是来自 authoritative lineage state
   - 现在和“值本身是什么”同样重要

这些都已经不是实现细节，而是正式治理前提。

### 2.3 plugin runtime 安全收紧
OpenClaw 3.11 明确阻止：
1. 未鉴权 plugin HTTP route
2. 通过 `runtime.subagent.*` 自动继承 synthetic admin scope

对 Loom 的意义：
1. “插件侧理论上能调到 runtime” 不再等于“当前请求真的有控制权限”
2. `worker_control_capabilities` 不能乐观默认支持

### 2.4 copied-workspace sandbox 的 child workspace 继承修复
OpenClaw 3.11 修正后：
1. parent 在 copied-workspace sandbox 中运行时
2. child `/agent` mount 继承真实配置 workspace
3. 不再错误继承 parent sandbox copy

对 Loom 的意义：
1. `workspace_ref / readable_roots / writable_roots / gateway cwd`
   - 更可以稳定地从宿主 workspace root 派生
2. 当前文档里“不要从 `cwd` 猜 workspace”这条要求，被 3.11 进一步坐实

### 2.5 cron/doctor 迁移
OpenClaw 3.11 对 legacy cron store 和 legacy notify metadata 做了收紧，并要求必要时运行：
1. `openclaw doctor --fix`

对 Loom 的意义：
1. 这不是 Loom 主治理链的核心变更
2. 但它是本地升级后的运维风险点
3. 如果宿主状态目录里存在旧 cron 数据，后续回归测试要记得单独校验

---

## 3. 这次暴露出来的真正根因
不是 `3.11` 平白“搞坏了” Loom，而是它把我们原来就有的文档和实现错位放大了。

### 3.1 当前代码里的能力快照是简化版
当前实现仍然偏向这些字段：
1. `allowed_tools`
2. `readable_roots / writable_roots`
3. `secret_classes`
4. `max_budget_band`
5. `available_agent_ids`
6. `supports_spawn_agents`
7. `supports_pause / supports_resume / supports_interrupt`

问题不在于这些字段没价值，而在于它们没有形成完整正式合同。

### 3.2 原正式合同又漏掉了另一半关键事实
旧版 `宿主能力快照合同` 更强调：
1. `available_agents / available_models / available_tools`
2. `render_capabilities`
3. `background_task_support / async_notice_support`
4. `worker_control_capabilities`

但它漏掉了：
1. `host_session_id`
2. `readable_roots / writable_roots`
3. `secret_classes`
4. `max_budget_band`
5. runtime 级 spawn 能力
6. 当前 session authority scope

所以旧状态是：
1. 代码有一半现实
2. 文档有另一半现实
3. 两边都不完整

### 3.3 3.11 之后再继续用粗粒度布尔值会越来越错
最典型的是：
1. `supports_spawn_agents`
   - 无法表达 `subagent` 和 `acp` 的 runtime 差别
   - 无法表达 `resumeSessionId`
2. `supports_pause / supports_resume / supports_interrupt`
   - 无法表达 authority scope、gateway auth、plugin auth、child ownership 的差别
3. `allowed_host_agent_refs=[]`
   - 无法区分“ACP 默认允许全部 agent”和“当前一个 agent 都不允许”
4. 只写 `session_role / control_scope`
   - 也无法区分它们来自 authoritative 宿主状态，还是 adapter fallback 推断

因此，问题的根因不是“字段少几个”，而是建模层级不对。

---

## 4. 这次文档修正后的核心设计
### 4.1 `HostCapabilitySnapshot` 改成 session-scoped
现在这份正式合同明确：
1. 快照必须带 `host_session_id`
2. 它描述的是“当前这个宿主会话”的现实能力
3. 不是宿主全局常量表

业务含义：
1. 同一个 OpenClaw 宿主里
2. `main`、`orchestrator`、`leaf`
3. 能力可能不一样
4. Loom 必须按会话看能力，而不是按宿主品牌看能力

### 4.2 spawn 能力单独建模
现在用：
1. `spawn_capabilities[]`

来显式描述：
1. `runtime=subagent` 是否可用
2. `runtime=acp` 是否可用
3. 当前 runtime 下 host agent scope 是：
   - `All`
   - `ExplicitList`
   - `None`
   - `Unknown`
4. 如果是 `ExplicitList`
   - 再明确列出允许哪些 `host_agent_ref`
5. 是否支持 `resumeSessionId`
6. 是否支持 thread spawn
7. 是否支持 parent progress stream

业务含义：
1. Loom 后面做 `AgentBinding`
2. 不再只看“能不能 spawn”
3. 而是看“能用哪个 runtime 去 spawn，能不能恢复，能带哪些 host agent，能不能把 child 进度回流给 parent”

### 4.3 authority scope 单独建模
现在用：
1. `session_scope.session_role`
2. `session_scope.control_scope`
3. `session_scope.source`

来描述：
1. 当前会话是 `main / orchestrator / leaf`
2. 当前会话能不能控制 children
3. 这份 authority 是来自宿主 authoritative lineage state，还是 adapter fallback derivation

业务含义：
1. `host_session_id`
   - 只是宿主聊天容器或宿主执行容器 id
2. `managedTaskRef`
   - 才是 Loom 任务 owner
3. `helperSessionKey`
   - 只是 adapter-local dispatch 句柄
4. authority scope
   - 不能再从这几个 id 里“顺便猜”
5. 如果当前只能根据 session depth 或 adapter 本地 dispatch 线索推断
   - 也必须显式标成 `source=Derived`

### 4.4 工程边界重新回到正式快照
这次把这些字段重新提升回正式 `HostCapabilitySnapshot`：
1. `readable_roots`
2. `writable_roots`
3. `secret_classes`
4. `max_budget_band`

业务含义：
1. `ExecutionAuthorization`
   - 的 `granted_areas` 必须建立在 `HostCapabilitySnapshot ∩ TaskScopeSnapshot`
     的现实上限之内
2. 如果宿主快照里不正式持有这些边界
3. 后面的授权就会建立在 adapter 私有推断上

### 4.5 worker control 改成 fail-closed
现在文档明确：
1. 未证实的 `pause / resume / cancel / interrupt`
2. 一律不能乐观写成支持

业务含义：
1. 用户看到的 pause/resume/cancel 能力
2. 必须来自已声明、可解释、可追溯的宿主现实能力
3. 不能来自“OpenClaw 大概应该支持”

### 4.6 执行授权也要同步升级
现在需要把：
1. `spawn_agent_allowed`

从 formal owner 降成 compat projection。

正式授权要改成：
1. 按 `runtime_kind` 收敛
2. 按 `host_agent_scope` 收敛
3. 如果 `session_scope.source != Authoritative`
   - 只能保守不扩权
   - 不能继续当作“有 children control”

业务含义：
1. Loom 之后做 `approve_start -> AgentBinding -> execute`
2. 不能再只带一个“允许 spawn”布尔值
3. 而要带“允许哪种 runtime、允许哪些 host agent、这些 authority 事实是否可信”

---

## 5. 对 Loom 主线的实际影响
### 5.1 现阶段不会立刻把 worker 路径改成 ACP
当前 Loom worker 派发主链仍然是：
1. `HostExecutionCommand`
2. adapter 读取待派发命令
3. OpenClaw 用 `sessions_spawn`
4. 当前实现主要走 `runtime=subagent`

因此：
1. `resumeSessionId`
   - 现在先进入能力建模
   - 不直接改变 Loom worker 主链

### 5.2 但能力建模必须现在就补齐
原因：
1. 后面即使还不切 ACP runtime
2. 也已经需要知道：
   - 当前会话能否 `runtime=subagent`
   - 当前会话能否 `runtime=acp`
   - 当前会话是否有 children authority
   - 当前会话的 authority 是 authoritative 还是 derived
   - ACP 默认 allow-all 到底怎样进入正式 capability

否则：
1. `AgentBinding`
2. `ExecutionAuthorization`
3. `CapabilityDriftAssessment`
4. `WorkerInterruption`

都会继续建立在粗粒度猜测上。

### 5.3 当前还没落完的高影响缺口
文档主链现在已经收口，但实现层仍有几块高影响缺口要留给下一轮：
1. `session_scope`
   - 目前 adapter 还有一部分来自 session depth 推断
   - 还没完全对齐到上游持久化 authority state
2. ACP allow-all 语义
   - 目前还没有完整映射进正式 capability builder
3. `ExecutionAuthorization`
   - 当前运行时代码里仍保留了粗粒度 `spawn_agent_allowed` 影子
4. `worker_control_capabilities`
   - 合同已收口
   - 但 pause/resume/cancel 控制流还没全量落地

---

## 6. 当前文档后的实现顺序
### 6.1 先文档
先冻结：
1. `HostCapabilitySnapshot` 新正式 shape
2. `spawn_capabilities`
3. `session_scope`
4. worker control fail-closed 原则
5. `authorized_spawn_capabilities`
   - 以及 `spawn_agent_allowed` 只剩 compat projection 的结论

### 6.2 再 TDD
优先补这些测试：
1. capability snapshot sync 会带 `host_session_id`
2. snapshot 能同时表达 `subagent` 与 `acp` runtime capability
3. `resumeSessionId` 只在 `runtime=acp` 的 capability 上体现
4. `ACP allow-all`
   - 会进入 `host_agent_scope.mode=All`
   - 不会退化成空 explicit list
5. `main / orchestrator / leaf` 和 `children / none` 会进入 `session_scope`
6. `session_scope.source`
   - 会区分 `Authoritative / Derived / Unknown`
   - 且 derived/unknown 不会扩大授权
7. capability fingerprint / drift assessment 会感知：
   - `spawn_capabilities`
   - `session_scope`
   - `worker_control_capabilities`
8. capability fingerprint / drift assessment 还必须感知：
   - `spawn_capabilities[].host_agent_scope`
   - `session_scope.source`
9. worker control 默认 fail-closed

### 6.3 最后实现
实现时要完成三层迁移：
1. TS wire type
2. Rust domain object
3. adapter capability builder 与 Harness 消费逻辑
4. `ExecutionAuthorization` runtime capability 收敛逻辑

补充取舍：
1. 旧的顶层布尔字段可以暂时保留 compat
2. 但必须降级为 projection/alias
3. 新逻辑不能继续把它们当 owner

---

## 7. 当前最重要的业务结论
1. `OpenClaw 3.11` 带来的不是单点 breaking change，而是“宿主能力语义被做实”
2. Loom 这次最该修的不是某个 spawn 调用，而是 `HostCapabilitySnapshot` 的层级建模
3. 如果这次只做最小兼容补丁，后面 ACP、worker control、authority drift 还会继续反复打脸
4. 所以这轮先修文档、再做 TDD、最后实现，是正确顺序
5. 这轮文档收口的目标，不只是适配 `3.11`
   - 也是把 Loom 主链升级成后续 OpenClaw 小版本还能继续承压的稳定合同
