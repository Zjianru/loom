# watchdog通知状态合同

状态：contract draft  
定位：`watchdogNoticeState` 正式规格  
更新时间：2026-03-11

---

## 1. 目的
`watchdogNoticeState` 是 Loom 中 `watchdog` 的正式通知状态对象。

它回答：
1. 哪些 notice 已经发过
2. 哪些事件还在等待通知
3. 哪些升级已经被去重
4. `watchdog` 下次扫描时如何避免重复提醒

更深层上，它解决的是：

**让 `watchdog` 的通知行为不退化成“看到事件就再发一遍”的临时逻辑。**

---

## 2. 设计边界
### 2.1 它是治理状态，不是 UI 展示状态
它是什么：
1. `runtime/loom/` 中的正式治理状态对象

它不是什么：
1. 不是宿主聊天区的 notice 文案
2. 不是 adapter 的缓存
3. 不是看板 UI 的已读状态

### 2.2 它不发明泛化升级策略聚合
固定边界：
1. `watchdog` 读取的是显式 policy 来源：
   - `WipPolicy.escalation_policy`
   - `ReworkPolicy.escalation_policy`
   - `AcceptancePolicy.escalation_policy`
2. 不再引入无 owner 的泛化升级策略对象

### 2.3 第一版注意力治理先并入 notice 状态
固定边界：
1. 第一版不单独新建总注意力聚合
2. notice 优先级、冷却和打断预算，先收进 `watchdogNoticeState` 与现有 `...escalation_policy`
3. 等个人任务 pack 成型后，再判断是否需要独立用户级注意力对象

### 2.4 如果未来引入用户级注意力对象
固定优先级：
1. 安全强打断
2. 用户硬偏好
3. 任务级收紧
4. pack 默认

### 2.5 第三层当前阶段状态
固定结论：
1. 第三层当前是 deferred
2. 当前不引入独立 `AttentionPolicy`
3. 当前不引入用户级注意力对象
4. 当前不进入生活节律治理实现

---

## 3. 正式对象定义
```rust
pub struct WatchdogNoticeState {
    pub managed_task_ref: ManagedTaskRef,
    pub pending_notices: Vec<PendingWatchdogNotice>,
    pub sent_notices: Vec<SentWatchdogNotice>,
    pub suppressed_notice_keys: Vec<NoticeDedupKey>,
    pub interruption_budget_counter: u32,
    pub interruption_budget_window_started_at: Option<Timestamp>,
    pub last_scan_at: Option<Timestamp>,
}

pub struct PendingWatchdogNotice {
    pub notice_key: NoticeDedupKey,
    pub notice_kind: WatchdogNoticeKind,
    pub notice_priority: WatchdogNoticePriority,
    pub source_event_ref: TaskEventRef,
    pub source_policy_ref: WatchdogPolicyRef,
    pub cooldown_until: Option<Timestamp>,
    pub first_observed_at: Timestamp,
}

pub struct SentWatchdogNotice {
    pub notice_key: NoticeDedupKey,
    pub notice_kind: WatchdogNoticeKind,
    pub notice_priority: WatchdogNoticePriority,
    pub source_event_ref: TaskEventRef,
    pub source_policy_ref: WatchdogPolicyRef,
    pub cooldown_until: Option<Timestamp>,
    pub sent_at: Timestamp,
}

pub enum WatchdogNoticeKind {
    StageEntered,
    StageCompleted,
    WorkItemAging,
    WorkItemBlocked,
    ApprovalPendingTooLong,
    ReworkExceeded,
    AcceptanceEscalated,
}

pub enum WatchdogPolicyRef {
    WipPolicy(WipPolicyId),
    ReworkPolicy(ReworkPolicyId),
    AcceptancePolicy(AcceptancePolicyId),
}

pub enum WatchdogNoticePriority {
    Silent,
    Notify,
    Interrupt,
}
```

---

## 4. 核心约束
1. `watchdogNoticeState` 必须按 `managedTaskRef` 归属。
2. 同一 `notice_key` 在同一个 open 条件周期内不得重复发送。
3. notice 去重不能只靠 adapter 或宿主侧幂等。
4. `watchdog` 重新扫描时，必须先读 `watchdogNoticeState`，再决定是否发新 notice。
5. `watchdogNoticeState` 不拥有任务生命周期，它只记录通知生命周期。
6. 第一版注意力治理只允许扩展 notice 状态与 escalation policy，不得另起平行注意力主链。

---

## 5. 与其它对象的关系
### 5.1 和 `TaskEvent`
`TaskEvent` 提供“发生了什么”。  
`watchdogNoticeState` 记录“这件事是否已经被通知过”。

### 5.2 和 `WipPolicy`
`WipPolicy.escalation_policy` 决定：
1. aging / blocked / waiting-user 何时升级

`watchdogNoticeState` 决定：
1. 升级 notice 是否已发
2. 后续扫描是否需要继续发

### 5.3 和 `ReworkPolicy`
`ReworkPolicy.escalation_policy` 决定：
1. 返工失败或返工超轮次何时升级

`watchdogNoticeState` 决定：
1. 这类升级是否已经通知过用户

### 5.4 和 `AcceptancePolicy`
`AcceptancePolicy.escalation_policy` 决定：
1. 什么时候必须把收口失败升级给用户

`watchdogNoticeState` 记录：
1. 该升级 notice 的正式去重状态

### 5.5 和最小注意力治理字段
`watchdogNoticeState` 继续持有：
1. `notice_priority`
2. `cooldown_until`
3. `interruption_budget_counter`

这些字段回答：
1. 这条 notice 该静默、提醒还是打断
2. 当前是否仍处在冷却期
3. 当前任务这一轮已经消耗了多少打断预算

### 5.6 和未来用户级注意力对象
如果未来确有独立用户级注意力对象：
1. 它只能覆盖 pack 默认和任务级收紧
2. 不能压过安全强打断
3. 第一版 `watchdogNoticeState` 仍继续作为当前任务 notice 生命周期的 owner

---

## 6. 我的建议
1. v0 就把 `watchdogNoticeState` 当成正式对象落到 `runtime/loom/`。
2. `watchdog` 的正确性要靠 state 驱动，不要退回“每次扫描重新猜该不该提醒”。
3. 它和 `PendingDecisionWindow` 一样，都是治理系统避免串线和重复的重要状态面。

---

## 7. 第三层未来进入条件
只有同时满足这些条件，才允许讨论独立用户级注意力对象：
1. `OpenClaw WebUI` 单入口验收体系已经稳定可跑
2. 至少一个个人场景 pack 已冻结
3. 真实交互已经证明现有 notice 链不够表达
