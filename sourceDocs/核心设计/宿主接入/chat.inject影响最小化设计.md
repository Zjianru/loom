# `chat.inject` 影响最小化设计

状态：temporary mitigation draft（phase 1 implemented, phase 1 validated）  
定位：`loom-openclaw` 在不修改宿主 OpenClaw 源码前提下，对 `chat.inject` 时序缺陷做的临时缓解设计  
更新时间：2026-03-12

---

## 1. 这份文档为什么存在
这份文档不是新的主线协议，也不是新的产品规范。

它只回答一个现实问题：

**在当前宿主只能正式依赖 `chat.inject` 完成用户可见治理消息投递时，插件侧怎样尽力把时序影响降到最低。**

这里必须先讲清楚立场：
1. 当前 v0 用户可见治理消息主路径仍是 `chat.inject`
2. 这份设计是迫不得已的插件侧缓解，不是要把临时逻辑抬成新的正式 owner
3. 后续只要宿主能力补齐，必须回归更干净的主线

当前代码只落了一期，范围刻意收窄：
1. 只有 `start_card` 完整接入 `initial_grace_ms + host_not_ready` 快重试 + `quiescent + wake`
2. `boundary_card`、`approval_request`、`result_summary`、`tool_decision`
   - 当前只接入共享的失败分类和 `last_error` 前缀化
   - 不在一期里启用 `quiescent + wake`
3. 一期 wake trigger 只接：
   - 同 session 新 `message_received`
   - `/loom help`
   - `/loom probe`
   - bridge 恢复 `active`
4. 一期明确不接：
   - `before_agent_start` / capability sync 成功唤醒
   - `structured replacement` 升主路径

---

## 2. 这份设计不改变什么
这份设计不改变以下正式约束：
1. `managed_task_candidate` 的第一条用户可见 managed 消息仍应是 start card
2. `delivery_id -> visible delivery -> ack_outbound` 仍是正式出站闭环
3. adapter 不得绕开 Loom authoritative outbox 伪造第二套可见消息真相
4. adapter 不得因为缓解 `chat.inject` 缺陷，就把 `structured replacement` 提前写成已落地主链

也就是说：
1. 顶层产品规范不降级
2. 验收口径不放松
3. 只是插件侧把失败处理做得更细、更克制

---

## 3. 根因回顾
当前问题不是 Loom 没创建 candidate，也不是没有生成 `StartCardPayload`。

真正的根因是：
1. 宿主实时 chat 可见链路
   - 来自 agent stream -> Gateway `chat` 事件 -> WebUI 前端内存
2. `chat.inject`
   - 则是先写 transcript，再广播 WebUI chat
3. 当宿主 session entry 已存在，但 transcript 文件尚未 materialize 时
   - `chat.inject` 会失败
4. 对 Loom 来说：
   - candidate、`PendingDecisionWindow`、`decision_token`、`OutboundDelivery`
     都可能已经 authoritative 成立
   - 只是“用户可见投递”尚未成功

这意味着插件侧必须分清：
1. 宿主还没准备好
2. 真正的宿主拒绝或桥接失败

如果继续把这两类失败混在一起处理，只会把：
1. 可恢复的时序窗
2. 和真正的 hard failure

都打成同一种 retry 行为，最后要么过早 `terminal_failed`，要么制造更差的晚到卡片体验。

---

## 4. 设计目标
### 4.1 要达到什么
1. 尽量提高 `chat.inject` 在 start card 首发阶段的成功率
2. 不要因为盲目拉长 retry，制造随机晚到的 start card
3. 保住 authoritative durable outbox 真相
4. 不引入新的宿主可见消息 owner
5. 把“宿主未就绪”和“真正失败”在插件层分流

### 4.2 明确不做什么
1. 不修改 OpenClaw 宿主源码
2. 不把 `before_message_write` 方案写成实时首显主路径
3. 不做 adapter-local 假消息注入
4. 不做无限 retry
5. 不在插件侧偷偷建立第二套“已可见”真相源

---

## 5. 当前插件真正能控制的杠杆
在不动宿主源码前提下，插件侧真正能控制的只有这些：
1. 什么时候调用 `chat.inject`
2. `chat.inject` 失败后，如何给 `delivery_id` 分类和重试
3. 什么时候主动唤醒 `retry_scheduled` delivery 再试一次
4. 哪些 payload 应该更激进，哪些 payload 应该更保守
5. 什么时候只记风险、不继续制造更差的用户体感

插件侧控制不了的事情也要明确：
1. 宿主 transcript 何时 materialize
2. WebUI 是否已经短暂看到了普通 assistant 实时文本
3. 宿主实时 chat 事件和 transcript 回读的时序一致性

这三个限制决定了：
1. 插件侧只能“尽量降低影响”
2. 不能承诺“完全消除影响”

---

## 6. 新增变量与状态
这份缓解设计建议在插件里新增三组概念。

### 6.1 `delivery_visibility_class`
它表示这条 delivery 对用户体感的敏感度。

建议分成：
1. `interactive_primary`
   - `start_card`
2. `interactive_secondary`
   - `boundary_card`
   - `approval_request`
3. `async_notice`
   - `result_summary`
   - `tool_decision`

取舍：
1. `start_card`
   - 对“第一条可见消息”的要求最强
2. `boundary_card / approval_request`
   - 仍然交互性很强，但比 start card 稍弱
3. `result_summary / tool_decision`
   - 晚一点到达，通常比 start card 晚一点更可接受

### 6.2 `inject_failure_class`
它表示一次 `chat.inject` 失败到底属于哪一类。

建议至少分成：
1. `host_not_ready`
   - 宿主 session 存在，但 transcript/materialize 相关依赖尚未就绪
   - 典型例子：`transcript file not found`
2. `bridge_or_transport_failure`
   - bridge 鉴权、peer 可达性、gateway 调用异常
3. `hard_failure`
   - 参数非法、宿主拒绝、返回形状异常等真正不该靠“再等等”解决的问题

这里几个变量的含义要固定：
1. `host_not_ready`
   - 不是“投递失败就算了”
   - 而是“当前宿主可见投递窗口未就绪”
2. `hard_failure`
   - 不是“晚一点就会好”
   - 而是“继续用同样方式重试价值很低”

### 6.3 `interactive_delivery_state`
这是插件本地的临时运行态，不是 authoritative owner。

建议按 `delivery_id` 记录：
1. `delivery_id`
2. `host_session_id`
3. `visibility_class`
4. `first_attempt_at`
5. `last_attempt_at`
6. `host_not_ready_count`
7. `entered_quiescent_at`
8. `last_failure_class`

这里的取舍是：
1. authoritative truth 仍在 Loom store
2. 插件本地状态只用于：
   - 决定下一次何时再唤醒
   - 是否进入 quiescent
   - 记录当前是否值得继续主动推送

---

## 7. 具体投递策略
### 7.1 `start_card` 首发策略
`start_card`
1. 不应直接零延迟撞宿主 transcript 时序窗
2. 也不应一上来就等很久

建议：
1. 对 `start_card` 增加一个很小的 `initial_grace_ms`
2. 初始建议值：
   - `initial_grace_ms = 500`

它代表：
1. 不是业务等待
2. 只是给宿主 session/transcript materialize 一个极短缓冲

取舍：
1. 牺牲约半秒首响
2. 换更高的首发成功率

### 7.2 `interactive_primary` 的前置重试窗口
对于 `start_card`，建议采用“前置压缩式重试”，而不是长时间均匀重试。

建议：
1. 第一轮：`initial_grace_ms = 500`
2. 第二轮：`+1000ms`
3. 第三轮：`+2000ms`
4. 第四轮：`+4000ms`

也就是在前 `7.5s` 左右，快速把“宿主刚好慢半拍”的场景尽量吃掉。

这里的变量含义：
1. `fast_retry_budget`
   - 专门留给 `host_not_ready` 的前置快速重试预算
2. 它不是全局 `max_attempts`
   - 而是插件对“值得主动立刻再试”的局部策略

### 7.3 `interactive_primary` 的 quiescent 策略
如果 `start_card` 在前置重试窗口内仍然持续 `host_not_ready`，不应该继续按 timer 一直主动推送。

建议：
1. 将这条 delivery 标记为 `quiescent`
2. 停止插件本地短周期 timer 重试
3. 不再追求“随机某个时间点自己突然补发成功”

原因：
1. start card 晚到，比不出现更容易破坏体验
2. 用户没有任何新动作时，随机跳出一张迟到 card，价值很低

这里要诚实说明插件侧边界：
1. 当前 bridge/store 没有“按策略主动终止但不耗尽 `max_attempts`”的正式接口
2. 所以插件侧的 `quiescent`
   - 只能表示“不再主动 timer 驱动”
   - 不能把 authoritative delivery 真相直接改成新的正式状态

### 7.4 `interactive_primary` 的 activity wake
`quiescent` 后的 `start_card` 不是彻底放弃，而是只在“同 session 真有新活动”时再试。

一期实际落地的 wake trigger 只有这些：
1. 同 session 新 `message_received`
2. `/loom help`
3. `/loom probe`
4. bridge 恢复 `active` 后，且该 session 有 `quiescent` interactive delivery

这里故意没有接：
1. `/loom approve / cancel / modify / keep / replace / reject`
   - 避免在控制面消费窗口前后，突然补发一张迟到的旧 start card
2. capability sync 成功
   - 一期先不把 `before_agent_start` 支线再拉进可见投递唤醒路径

具体做法：
1. 插件本地根据 `interactive_delivery_state` 找到该 session 尚未投递成功的 `delivery_id`
2. 调用 `schedule_outbound_retry(delivery_id, now, "woken_by_session_activity")`
3. 然后立刻 `drainOutbound(host_session_id)`

这样做的好处是：
1. 仍通过 authoritative retry 路径推进
2. 不需要本地伪造 delivery 状态
3. 把“再试一次”的时机，尽量绑到用户真的重新回到这个 session 的时刻

### 7.5 `interactive_secondary` 的策略
`boundary_card / approval_request`
1. 仍然是交互型 payload
2. 但它们通常出现在用户已经进入 Loom 控制面的后段

建议：
1. 沿用 bounded retry
2. 仍做失败分类
3. 对 `host_not_ready` 可以保留比 `start_card` 更长一点的 timer retry

原因：
1. 这些卡片晚一点出现，体验问题仍存在
2. 但通常没有 start card 那么致命

### 7.6 `async_notice` 的策略
`result_summary / tool_decision`
1. 继续按当前 bounded retry 主逻辑走
2. 但 `last_error` 要带上失败分类

建议：
1. 保留现在的 `attempts / max_attempts / next_attempt_at / terminal_failed`
2. 不为它们额外引入 quiescent 逻辑

原因：
1. 异步结果消息最需要的是“最终有真相”
2. 它们不像 start card 那样强依赖“第一眼首帧体验”

---

## 8. 错误分类规则
插件侧需要一段明确的分类逻辑，而不是继续只看 exit code。

建议规则：
1. 命中 `transcript file not found`
   - 归类为 `host_not_ready`
2. 命中 `session not found`
   - 优先归类为 `hard_failure`
   - 因为这表示 `host_session_id` 本身可能就无效
3. `chat.inject` 返回 payload shape 非法
   - 归类为 `hard_failure`
4. gateway 命令超时、bridge peer 不可达
   - 归类为 `bridge_or_transport_failure`

还建议把 `last_error` 规范化为带前缀的字符串，例如：
1. `host_not_ready: transcript file not found`
2. `bridge_or_transport_failure: gateway timeout`
3. `hard_failure: invalid chat.inject payload`

原因：
1. 方便日志聚合
2. 方便之后按失败类统计
3. 不需要改当前 authoritative 字段结构

---

## 9. 事件驱动唤醒的接线点
插件侧当前已经有这些天然入口：
1. `message_received`
2. `/loom` command handler
3. bridge 恢复为 `active` 的时刻

一期代码实际接的是：
1. 普通 `message_received`
   - `text` 不是 `/loom ...` 时，作为 session activity wake
2. `/loom help`
3. `/loom probe`
4. bridge 恢复 `active`

一期明确没接的是：
1. `before_agent_start` 内的 capability sync
2. `/loom approve / cancel / modify / keep / replace / reject`

这份设计建议把它们变成唤醒点，但注意顺序：
1. 先做原有主逻辑
2. 再判断该 session 是否存在 `quiescent` 的 interactive delivery
3. 如果存在，再执行一次 `wake-and-drain`

这样可以避免两类错误：
1. 因为 wake 逻辑反向阻断原有治理主链
2. 因为每次进入 hook 就无条件强拉所有旧 delivery

---

## 10. 观测与日志
如果要把这套缓解设计做稳，必须补最少的观测事件。

建议新增这些日志：
1. `bridge.peer.outbound_inject_failure_classified`
   - 记录 `delivery_id / payload_type / failure_class / attempts`
2. `bridge.peer.outbound_interactive_quiesced`
   - 记录 `delivery_id / host_session_id / age_ms / host_not_ready_count`
3. `bridge.peer.outbound_activity_wakeup`
   - 记录 `delivery_id / host_session_id / trigger`
4. `bridge.peer.outbound_late_delivery_risk`
   - 记录“这条 interactive delivery 已经进入明显晚到风险区”

这些日志的意义分别是：
1. 能区分“为什么失败”
2. 能区分“是继续重试还是主动 quiet down”
3. 能看清“到底是 timer 驱动救起来，还是 session 活动救起来”

---

## 11. 当前插件侧做不到的事
这部分必须明写，防止后面误判能力边界。

### 11.1 做不到“严格禁止晚到卡片”
原因：
1. 当前 authoritative API 只有：
   - `next_outbound`
   - `ack_outbound`
   - `schedule_outbound_retry`
2. 没有一个正式接口允许插件在不耗尽 `max_attempts` 的情况下，
   把某条 delivery 因“体验已过时”主动终止成新的 authoritative 终态

因此：
1. 插件可以减少主动 timer retry
2. 但不能完全保证某条 interactive delivery 以后绝不会被再次唤醒

### 11.2 做不到知道用户到底看没看到普通 assistant 泄漏
原因：
1. 当前插件看不到 WebUI 前端内存态
2. 也没有一个“用户已见到普通 assistant bubble”的正式回传

因此：
1. 插件不能以“已泄漏”作为 authoritative 判据
2. 只能做更保守的 retry 节奏控制

---

## 12. 为什么不选其它方案
### 12.1 为什么不继续盲目加大 `max_attempts`
因为它解决的是“更久地继续试”，不是“更合理地试”。

坏处：
1. start card 成功率表面提高
2. 但更容易制造晚到卡片
3. 还会让真正的宿主未就绪问题被埋在更长的等待后面

### 12.2 为什么不把 `structured replacement` 直接升成主路径
因为当前宿主里：
1. `before_message_write`
   - 只能改持久化前的 message
2. 实时首屏 assistant bubble
   - 仍来自另一条链

所以它现在最多是：
1. transcript/history 修正手段
2. suppression 合同未来收口方向

不是：
1. 现成可替代 `chat.inject` 的正式首显主路径

### 12.3 为什么不让 adapter 本地伪造可见消息
因为那会破坏：
1. `delivery_id`
2. `ack_outbound`
3. authoritative outbox

之间的正式闭环。

一旦这么做，等宿主或 bridge 重放时，极容易长出第二套“看起来已经投递成功”的假真相。

---

## 13. 未来怎样回归主线
这份设计从一开始就必须写清退出条件。

只要满足下面任一条件，就应开始拆除这套临时缓解：
1. 宿主 `chat.inject` 本身补齐 transcript lifecycle 契约
2. 宿主提供新的正式接口，能稳定完成“实时首显 + transcript 持久化”一致投递
3. 宿主提供正式的“结构化替换当前 assistant 首屏消息”接口

回归主线时，应删除或弱化这些东西：
1. `start_card` 的 `initial_grace_ms`
2. `interactive_primary` 的 quiescent 状态
3. activity wake 专用重试逻辑
4. `host_not_ready` 的特殊分流分支
5. adapter-local 对 interactive delivery 的额外缓存状态

回归后的目标形态应重新简化为：
1. `next_outbound`
2. 正式宿主可见投递
3. `ack_outbound`
4. bounded retry 只处理真正的 transport / host failure

---

## 14. 我的建议
这份设计里，我建议优先落下面三件事：
1. `chat.inject` 失败分类
2. `start_card` 的前置压缩式重试
3. `quiescent + activity wake`

原因是：
1. 这三件事都能纯插件侧完成
2. 不需要改宿主源码
3. 能明显降低“过早 terminal_failed”和“随机晚到卡片”这两类最坏体验

我不建议当前就做的事：
1. 继续统一加大 `max_attempts`
2. 把 `structured replacement` 提前写成主路径
3. 下调顶层产品规范

一句话收口：

**当前必须继续走 `chat.inject` 主路径，但要把 retry 从“统一重试”升级成“按失败类、payload 类和用户体感分层处理”；这是一套迫不得已的临时缓解，不是未来正式主线。**

---

## 15. Phase 1 实测结果（2026-03-12）
这部分只记录已经发生过的真实验证结果，不再讨论“理论上会怎样”。

### 15.1 这轮到底验证了什么
本轮 clean-room 实测验证了两层不同结论：
1. 技术缓解链路成立
2. 产品首显验收仍未通过

这里几个变量要明确：
1. `managed_task_ref`
   - 表示 Loom authoritative task owner
   - 它已创建，说明 candidate 主链成立
2. `current_pending_window_ref`
   - 表示 start card 对应的 open decision window
   - 它已存在，说明 start window 已正确打开
3. `delivery_status`
   - 表示 start card 这条 authoritative outbox delivery 当前走到了哪一步
   - 本轮经历了 `retry_scheduled -> acked`
4. `attempts / max_attempts`
   - 表示这条 delivery 已消耗几次正式投递预算
   - 本轮最终消耗到 `6 / 6`
5. `next_attempt_at`
   - 表示 authoritative store 允许下一次 redelivery 的时间
   - 本轮在进入 `quiescent` 时被 park 到远期时间
6. `acked_at`
   - 表示宿主正式接受并完成可见投递的时间
   - 它不代表“首条可见消息正确”，只代表“最终曾经投进去过”

### 15.2 真实发生了什么
本轮 clean-room 中：
1. 用户首条输入后，WebUI 第一条可见消息仍然是普通 assistant 文本
2. 约 8 秒后，Loom authoritative side 才出现：
   - `managed_task_ref`
   - `current_pending_window_ref`
   - `start_card` delivery
3. `chat.inject` 随后连续命中：
   - `failed to write transcript: transcript file not found`
4. 插件按一期设计执行了：
   - `host_not_ready` 分类
   - 前置压缩式 retry
   - `quiescent`
   - `late_delivery_risk`
5. 之后通过 `/loom probe` 触发 activity wake
6. 同一条 `delivery_id` 最终 `acked`

也就是说：
1. 一期设计已经把问题从“静默卡死/早死”修成了“可 park、可 wake、最终可 ack”
2. 但它还没有把产品体验修到“start card 首条可见”这条验收线

### 15.3 时间量化
本轮有两个关键时间段：
1. `start_card` 创建到进入 `quiescent`
   - 约 `12.581s`
2. `start_card` 创建到最终 `acked`
   - 约 `224.515s`

这个量化说明的不是“再多等几分钟就没问题”，而是：
1. 前置快重试窗口已经结束
2. 后续成功依赖 session activity wake
3. 因此最终成功属于“晚到成功”，不是“首发成功”

### 15.4 对用户体感的真实结论
本轮用户体感最终是：
1. 普通 assistant 先完整可见
2. `/loom probe` 之后，start card 才晚到出现

这正对应本设计里一直强调的边界：
1. 当前插件侧可以降低 `chat.inject` 的失败率
2. 但不能在不改宿主源码前提下，保证“第一条实时可见消息一定是 start card”

### 15.5 这轮验证后的判断
这轮之后，可以把 phase 1 的定位收紧成：
1. **技术验证：通过**
   - `host_not_ready -> retry -> quiescent -> activity wake -> acked` 已被真实验证
2. **产品验收：未通过**
   - `L-02 explicit-managed-candidate` 仍失败
3. **根因状态：未变**
   - 主阻断仍是宿主 transcript materialize 时序窗

### 15.6 对后续工作的约束
因此后续如果继续在插件侧推进，必须记住：
1. 不能把 `acked` 误写成“用户体感已经合格”
2. 不能把 activity wake 成功误写成“L-02 已通过”
3. phase 2 的目标不应再是继续证明一期逻辑能跑
4. phase 2 应转向决策：
   - 还值不值得继续在插件侧压缩晚到概率
   - 还是把这个问题正式定义为宿主能力缺口并等待上游收敛
