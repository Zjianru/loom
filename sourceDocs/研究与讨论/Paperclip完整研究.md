# Paperclip完整研究

状态：research draft  
定位：`Paperclip` 产品、架构、运行时与竞争边界完整拆解  
更新时间：2026-03-10

---

## 1. 研究范围与取证边界
这份文档回答 6 个问题：
1. `Paperclip` 到底在做什么，不做什么。
2. 它的最小正式对象和主运行循环是什么。
3. 它的控制平面根基落在什么数据模型和服务结构上。
4. 它和 `OpenClaw / Codex / Claude Code / Cursor` 的关系是什么。
5. 它真正强的地方在哪里，薄弱点又在哪里。
6. 如果 Loom 后面一定向上生长，它对我们意味着什么。

本轮取证来源固定为：
1. 官方 GitHub 仓库与 README
2. 官方文档站中的 `what-is / architecture / core-concepts / agents-runtime`
3. 仓库源码静态拆解
4. 仓库最新公开提交快照

本轮源码拆解基线：
1. 仓库：`paperclipai/paperclip`
2. inspected commit：`49b9511889c839ffb9a2c790d4d18403b8b2eeef`
3. 最近提交时间：`2026-03-10 07:25:30 -0500`

固定边界：
1. 这是源码级静态研究，不包含完整本地运行实验。
2. 对内部实现强弱的判断，若不是源码直接写死的事实，都会显式标为“判断”或“推断”。

---

## 2. 一句话结论
`Paperclip` 不是任务治理内核，而是一个已经成型的 **AI 公司控制平面**。

它的根是：
1. `Company`
2. `Agent`
3. `Issue`
4. `Heartbeat`
5. `Approval`
6. `Budget`
7. `Audit trail`

它的打法不是“把单个 agent 做深”，而是：

**把很多异构 agent 变成一个可管理、可预算、可审批、可追踪的组织系统。**

对 Loom 的意义很直接：
1. 如果 Loom 只做任务治理内核，它不是正面冲突。
2. 如果 Loom 要继续往上长成控制平面，它就是直接竞争对手。
3. 但它目前看上去更强在“上层控制面”，不一定强在“复杂任务治理内核”。

---

## 3. 官方自我定义
截至 2026-03-10，`Paperclip` 官方 README 的自我定义非常明确：
1. 它是“Open-source orchestration for zero-human companies”。
2. 它是 `Node.js server + React UI`。
3. 它强调自己不是 chatbot，不是 agent framework，不是 single-agent tool。
4. 它直接写了：“If OpenClaw is an employee, Paperclip is the company.”

这几句话很关键，因为它们把产品边界写死了：
1. `OpenClaw / Codex / Claude Code / Cursor`
   - 在它的体系里是 employee / runtime / adapter target
2. `Paperclip`
   - 才是上层控制平面

它公开支持的运行体：
1. `OpenClaw`
2. `Claude Code`
3. `Codex`
4. `Cursor`
5. `Bash`
6. `HTTP`

从产品定义上看，它试图占住的不是某个模型或某个 agent 的能力，而是：

**所有 agent 之上的公司层 orchestration。**

来源：
1. [GitHub 仓库](https://github.com/paperclipai/paperclip)
2. [README](https://raw.githubusercontent.com/paperclipai/paperclip/master/README.md)
3. [What is Paperclip](https://raw.githubusercontent.com/paperclipai/paperclip/master/docs/start/what-is-paperclip.md)

---

## 4. 它的最小正式对象是什么
`Paperclip` 的核心对象很稳定，不是随手拼出来的 UI 词汇。

### 4.1 `Company`
它是什么：
1. 顶层组织单位

它代表什么：
1. 一家公司或一个独立经营体
2. 它有目标、预算、组织结构、issue 前缀和计数器

源码证据：
1. `companies` 表有 `name / description / status / issuePrefix / issueCounter / budgetMonthlyCents / spentMonthlyCents / requireBoardApprovalForNewAgents / brandColor`

说明：
1. `issuePrefix`
   - 代表公司自己的 issue 编号前缀
2. `requireBoardApprovalForNewAgents`
   - 代表 hire 不是默认放开，而是公司级治理策略

### 4.2 `Agent`
它是什么：
1. 员工，不是任务

它代表什么：
1. 某个执行体实例
2. 它有 role、manager、adapterType、runtimeConfig、budget、permissions、status

源码证据：
1. `agents` 表有 `reportsTo`
2. `agents` 表有 `adapterType / adapterConfig / runtimeConfig`
3. `agents` 表有 `budgetMonthlyCents / spentMonthlyCents`

这几个变量分别代表：
1. `reportsTo`
   - 组织汇报线，不是任务依赖
2. `adapterType`
   - 这个 agent 由哪个 adapter 驱动，例如 `codex_local` 或 `openclaw_gateway`
3. `adapterConfig`
   - adapter 自己的配置
4. `runtimeConfig`
   - 调度、cwd、heartbeat 等运行参数

### 4.3 `Issue`
它是什么：
1. 工作单元

它代表什么：
1. 一个可被分配、可 checkout、可 review、可 done 的任务票据

源码证据：
1. `issues` 表有 `projectId / goalId / parentId`
2. `issues` 表有 `assigneeAgentId / assigneeUserId`
3. `issues` 表有 `checkoutRunId / executionRunId / executionLockedAt`

这几个变量分别代表：
1. `checkoutRunId`
   - 这张 issue 当前被哪个 heartbeat run 正式 checkout 了
2. `executionRunId`
   - 当前哪一个 run 拿着执行锁
3. `executionLockedAt`
   - 这把执行锁是什么时候拿到的

这是它任务正确性的一个核心根。

### 4.4 `Heartbeat`
它是什么：
1. agent 的执行窗口

它代表什么：
1. 一次短执行，而不是常驻 agent

源码证据：
1. `heartbeat_runs` 表有 `invocationSource / status / usageJson / resultJson / sessionIdBefore / sessionIdAfter / contextSnapshot`
2. `agent_wakeup_requests` 表单独存在

这两个对象的边界非常重要：
1. `agent_wakeup_requests`
   - 代表“有人或系统请求唤醒”
2. `heartbeat_runs`
   - 代表“真正排队并执行的一次 run”

这说明它把“请求唤醒”和“实际运行”拆成两层了，不是一个表糊到底。

### 4.5 `Approval`
它是什么：
1. 正式审批对象

它代表什么：
1. 某个必须人工拍板的动作

源码证据：
1. `approvals` 表有 `type / status / payload / decisionNote / decidedByUserId / decidedAt`
2. `APPROVAL_TYPES` 当前写死至少有：
   - `hire_agent`
   - `approve_ceo_strategy`

判断：
1. 这说明它的审批是公司级/组织级治理对象
2. 但它不是像 Loom 那样细到 task 内 review/result/boundary 的窗口族

### 4.6 运行连续性对象
`Paperclip` 对“连续运行”用了两层对象：
1. `agent_runtime_state`
2. `agent_task_sessions`

它们分别代表什么：
1. `agent_runtime_state`
   - 某个 agent 的总体运行状态、最近 session、累计 token 和成本
2. `agent_task_sessions`
   - 某个 agent 在某个 `taskKey` 下的可恢复 session

这个拆法说明它很重视：
1. agent 级总体状态
2. task scope 级会话恢复

### 4.7 可移植组织对象
它还有一套很像“模板市场”的对象：
1. `CompanyPortabilityManifest`
2. `CompanyPortabilityImport/Export/Preview`

它们代表什么：
1. 一家公司及其 agent 配置可以被导出、预览、导入
2. secret 不直接打包，而是进入 `requiredSecrets`

这正是 README 里 `ClipMart` 的代码根。

---

## 5. 它的主运行循环是什么
`Paperclip` 的主循环不是“用户发一句话，agent 回一句话”，而是：

**wakeup -> queued run -> adapter execute -> agent 回调 Paperclip API -> checkout/更新任务 -> 记录 run -> 下次 resume**

### 5.1 wakeup 层
官方文档和代码都确认了 wakeup 来源至少有：
1. `timer`
2. `assignment`
3. `on_demand`
4. `automation`

这意味着它的默认世界观不是聊天世界观，而是调度世界观。

### 5.2 wakeup 合并与延期
`server/src/services/heartbeat.ts` 里最关键的逻辑不是单纯排队，而是：
1. 如果已有同 scope 的 queued/running run，会 `coalesced`
2. 如果 issue 正在执行且这次唤醒不适合直接打进去，会 `deferred_issue_execution`

这几个变量代表什么：
1. `coalesced`
   - 合并唤醒，不新开 run
2. `deferred_issue_execution`
   - 暂不执行，等当前 issue execution 结束后再推进
3. `contextSnapshot`
   - 本次 run 的上下文快照，可以在合并时被 merge

这说明它已经认真处理了“重复唤醒”和“正在执行时的二次触发”。

### 5.3 atomic checkout
`server/src/services/issues.ts` 的 `checkout()` 不是一句更新状态，而是带锁语义的：
1. 期望 issue 当前处于允许的 status
2. assignee 要么为空，要么当前 run 本来就持有
3. `executionRunId` 不能冲突
4. 冲突时直接抛 `Issue checkout conflict`

它还额外处理了两类特殊情况：
1. stale checkout run 被后续 run 接管
2. 同一个 run 重入时不自我 `409`

这几条很关键，因为它说明：
1. 它不是“拿到任务就改成 in_progress”
2. 它是在做任务占有的原子化

### 5.4 session resume
官方 `Agent Runtime Guide` 写明支持 session resume，代码里对应：
1. `heartbeat_runs.sessionIdBefore / sessionIdAfter`
2. `agent_task_sessions.sessionParamsJson / sessionDisplayId / lastRunId`

这意味着下次 heartbeat 不是从零开始，而是沿 taskKey 复用可恢复会话。

### 5.5 结果与成本回写
run 完成后，它会回写：
1. `usageJson`
2. `resultJson`
3. stdout/stderr excerpt
4. full logs reference

成本事件独立存入 `cost_events`，然后同步：
1. 增加 agent spend
2. 增加 company spend
3. agent 超预算后自动 `paused`

这里的取舍很明显：
1. 成本是正式数据，不是 run 日志附属信息
2. 预算 enforcement 是控制平面硬约束

### 5.6 approval resolution 后的回路
审批通过不仅更新状态，还会：
1. 对 `hire_agent` 真正激活或创建 agent
2. 调用 adapter 的 `onHireApproved` hook
3. 在审批 requester 上重新排 wakeup

这说明它的审批不是死数据，而是会回流进入 agent 运行系统。

---

## 6. 它的架构是怎么切的
`Paperclip` 现在是一个已经较完整的 monorepo 控制平面。

### 6.1 顶层结构
从仓库结构看，它固定分成：
1. `ui/`
2. `server/`
3. `packages/db`
4. `packages/shared`
5. `packages/adapter-utils`
6. `packages/adapters/*`
7. `cli/`
8. `skills/`
9. `docs/`

这意味着：
1. UI、服务端、共享类型、adapter 协议、adapter 实现、CLI 都是独立层
2. 它不是一个小脚本仓库

### 6.2 adapter model
`packages/adapter-utils/src/types.ts` 把 adapter 模型写得很清楚：
1. `ServerAdapterModule`
   - 真正执行 agent、做环境测试、处理 session codec、可接 hire hook
2. `CLIAdapterModule`
   - 终端格式化
3. UI transcript parser
   - stdout/stderr 到 transcript 的解析

这说明它把 adapter 拆成三面：
1. server execution 面
2. UI 呈现面
3. CLI 呈现面

这是它在工程上比较成熟的一点。

### 6.3 当前内置 adapter
公开包里已经有：
1. `claude-local`
2. `codex-local`
3. `cursor-local`
4. `opencode-local`
5. `pi-local`
6. `openclaw-gateway`

`OpenClaw` 不是旁支，而是正式一等 adapter 目标。

### 6.4 OpenClaw gateway 接法
`openclaw-gateway` adapter 的 README 明确写了：
1. 固定走 WebSocket gateway
2. 支持 device auth
3. `sessionKeyStrategy=issue|fixed|run`
4. `idempotencyKey` 使用 `Paperclip runId`
5. 事件流被转成 Paperclip logs/transcript

这说明它对 `OpenClaw` 的理解，不是“发个 HTTP 请求就完”，而是把它作为一个正式远程 runtime 接入。

---

## 7. 它真正强在哪里
从公开资料和源码看，`Paperclip` 的强点不是单点功能，而是“控制平面闭环已经成型”。

### 7.1 强点一：产品定位清楚
它没有把自己说成 agent framework，也没有说成 prompt manager。

这很重要，因为它避免了产品边界模糊：
1. 上层就是公司治理
2. 下层执行体可以异构

### 7.2 强点二：控制平面对象扎实
它不是只有 UI 概念，而是真有这些正式对象：
1. company
2. agent
3. issue
4. approval
5. wakeup request
6. heartbeat run
7. cost event
8. activity log
9. agent task sessions

这意味着它不是“前端很会讲故事，后端只有几张表”。

### 7.3 强点三：heartbeat 协议是它的根
它真正的差异化逻辑不是 org chart，而是：
1. wakeup
2. coalescing
3. deferred execution
4. session resume
5. task checkout lock

也就是说，它最硬的一层其实是“调度协议”，不只是 dashboard。

### 7.4 强点四：预算和审计是正式一等能力
它把：
1. `cost_events`
2. `spentMonthlyCents`
3. `activity_log`

都做成正式对象。

这说明它不是单纯“让 agent 干活”，而是“让 agent 干活时还要能追责、控成本、回看历史”。

### 7.5 强点五：开源传播性非常强
它的开源条件对传播很友好：
1. `MIT`
2. `npx paperclipai onboard --yes`
3. 默认 embedded PostgreSQL / local disk
4. OpenClaw / Codex / Claude 等都能接

这意味着它很容易被开发者拿来试。

---

## 8. 它的薄弱点和可打穿点
下面这部分区分“事实”和“判断”。

### 8.1 事实：它的治理单位主要是 issue，不是复杂任务内核
从 docs 和 schema 看，它的核心工作单位是 `Issue`。

这意味着它擅长的是：
1. 任务分配
2. 任务占有
3. 任务状态流转
4. 多 agent 组织协调

但公开对象里没有出现 Loom 这种更深层的任务治理对象族：
1. `PendingDecisionWindow`
2. `managedTaskRef`
3. `PhasePlan`
4. `SpecBundle`
5. `IsolatedTaskRun`
6. `ProofOfWorkBundle`

### 8.2 判断：它的 review / result / proof 链可能没有 Loom 深
我当前的判断是：
1. `Paperclip` 很强在调度和控制平面
2. 但它公开暴露的 review/result/proof 语义，明显没有 Loom 现在文档里定义得细

这意味着它可能更像：
1. 一个强控制平面
2. 加一个通用 ticket/work loop

而不是：
1. 一个复杂任务治理内核

### 8.3 事实：它很多正确性依赖外部 agent 自己回调 API
它的主模式是：
1. adapter 拉起 agent
2. agent 再回来调 Paperclip REST API

这很通用，但代价也很明显：
1. agent 行为越不稳定，控制平面越难保证任务语义不漂
2. 控制平面更容易管理“任务有没有动”，不一定能管理“任务是不是被正确治理”

### 8.4 判断：它上层很强，但下层复杂任务边界可能更松
如果 Loom 把任务治理内核做扎实，那么将来真正能打它的，不是：
1. 另一个 dashboard
2. 另一个 org chart
3. 另一个 agent marketplace

而是：
1. 更强的任务边界
2. 更强的审批窗口
3. 更强的 review / proof / result 主链
4. 更低漂移的复杂任务执行

### 8.5 事实：它已经在往“模板市场”方向走
README 里的 `ClipMart` 和源码里的 `company-portability` 一起看，说明它在做：
1. 公司模板
2. 组织导入导出
3. secret scrub
4. portable org configs

这会是它未来很强的增长点。

但这也意味着：
1. 它会更偏“公司模板平台”
2. 不一定会优先把复杂任务治理内核做到最深

---

## 9. 对 Loom 的竞争含义
### 9.1 冲突判断
如果 Loom 未来一定向上生长，那么和 `Paperclip` 的冲突是明确存在的。

冲突层主要有：
1. 多 agent 控制平面
2. 组织结构与目标树
3. ticket / task system
4. budget / governance / approvals
5. OpenClaw 等异构 agent 的统一接入

### 9.2 但不该直接复制它的表层
如果我们只是去补：
1. org chart
2. mobile dashboard
3. company templates

最后很容易变成在它已经占位的上层 UI 赛道里追赶。

更合理的路线是：
1. 先把 Loom 做成真正替不掉的任务治理内核
2. 再向上长控制平面

### 9.3 Loom 最该和它错位竞争的地方
Loom 应该优先赢这些：
1. 复杂任务怎样不漂
2. 任务边界怎样不串线
3. start / approval / boundary 这些窗口怎样 formal 化
4. review / result / proof 怎样形成强交付链
5. adapter 怎样不重新长出双真相源

如果这些能赢，Loom 后面再向上长：
1. company
2. portfolio
3. org chart
4. budget
5. ticket system

才有根。

### 9.4 我对路线的建议
我的建议是：
1. 不要把 `Paperclip` 当成一个应该立刻功能对齐的产品。
2. 要把它当成“上层控制平面的成熟样本”。
3. Loom 先赢根，再长壳。

这里“根”指的是：
1. 任务 owner
2. 决策窗口 owner
3. review/result/proof owner
4. runtime authoritative truth

这里“壳”指的是：
1. 组织面板
2. 调度面板
3. 公司模板
4. 多公司管理

---

## 10. 这次研究后我对它的最终判断
`Paperclip` 已经不是一个随手做出来的竞品雏形，而是一个方向明确、工程完整度不低、传播性很强的控制平面项目。

它最值得警惕的不是某一个功能，而是这三件事已经串起来了：
1. 清晰的产品边界
2. 足够完整的控制平面对象
3. 对 OpenClaw / Codex / Claude 的异构接入能力

但它也给 Loom 留了窗口：
1. 它的根更像 company orchestration，不像复杂任务治理内核。
2. 这意味着 Loom 还有机会先在“治理正确性”上赢。
3. 只要根赢了，未来往上生长不是补 UI，而是自然长出控制平面。

---

## 11. 后续建议
如果继续深入研究 `Paperclip`，我建议按这个顺序做第二轮：
1. 本地真实跑起来，验证 onboarding、company、agent、issue、heartbeat、approval 的一条主链。
2. 实际接一个 `OpenClaw` 或 `Codex` runtime，验证它的 adapter 和 session resume 真实体验。
3. 画一张 `Paperclip vs Loom` 的对象级对照矩阵。
4. 再决定 Loom 上层控制平面第一版只长到哪里，不要一口气长满。

---

## 12. 主要来源
1. [GitHub 仓库](https://github.com/paperclipai/paperclip)
2. [README](https://raw.githubusercontent.com/paperclipai/paperclip/master/README.md)
3. [Core Concepts](https://raw.githubusercontent.com/paperclipai/paperclip/master/docs/start/core-concepts.md)
4. [Architecture](https://raw.githubusercontent.com/paperclipai/paperclip/master/docs/start/architecture.md)
5. [Agent Runtime Guide](https://raw.githubusercontent.com/paperclipai/paperclip/master/docs/agents-runtime.md)
6. [OpenClaw Gateway Adapter README](https://raw.githubusercontent.com/paperclipai/paperclip/master/packages/adapters/openclaw-gateway/README.md)
7. [GitHub API 仓库元信息](https://api.github.com/repos/paperclipai/paperclip)
