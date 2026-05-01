# Temporal Field Kernel：时间场内核头脑风暴

> 基于现有 `Brainstorming.md` 继续扩展。
>
> 核心目标：把 Agent 的“时间理解”从状态维护、任务进度、memory/RAG，推进到对未来行动路径的约束、压力、机会和边界建模。

---

## 0. 先挑刺：不要反状态机反到过头

任何实现最终都会有 state。数据库、事件流、continuation、模型参数，都是状态。

所以不要把卖点写成：

> 我们没有状态。

应该写成：

> 我们不把“当前状态”作为 Agent 认知的主接口；我们把历史投影成对未来路径的约束、压力和机会。

也就是：

```text
状态是实现细节。
时间场是决策接口。
```

真正的区别是：

```text
普通状态机：
  当前是什么状态？

Temporal Field：
  当前哪些历史影响正在改变未来行动的价值分布？
```

否则 continuation space 很容易被质疑为“换了名字的状态表”。

---

## 1. 把 Continuation 重新定义成“未来路径约束”

现有文档里的 continuation 更像“打开的任务/张力”。这个方向是对的，但还可以更硬一点：

> Continuation 是一个作用在未来路径上的未闭合约束。

形式上：

```text
κ = {
  origin: 起源事件,
  scope: 作用范围,
  path_predicate: 它希望未来路径满足什么,
  pressure_curve: 随时间变化的压力,
  violation_cost: 被破坏的代价,
  closure_condition: 如何关闭,
  repair_policy: 破坏后如何修复,
  confidence: 这是不是可靠推断
}
```

这样它就不是 todo，也不是普通 memory，而是：

```text
对未来轨迹 Γ 的约束函数。
```

例子：

```text
κ_user_rejected_state_machine = {
  origin: 用户纠正“不要做项目状态机”,
  scope: 当前理论设计线程 + 后续相关讨论,
  path_predicate: 后续方案不能以 task state / project memory 为主轴,
  pressure_curve: 当前对话内极高，长期中等,
  violation_cost: 用户再次纠正、信任下降、设计倒退,
  closure_condition: 提出足够通用且可操作的时间框架,
  repair_policy: 明确承认偏移，重新抽象,
  confidence: high
}
```

这个定义更强，因为它能直接进入行动选择。

---

## 2. Continuation 类型系统

Continuation 不能只有一种。否则所有东西都会混在一起。

建议至少分七类：

```text
1. Obligation Continuation
   承诺、约定、deadline、未兑现动作。

2. Epistemic Continuation
   问题、假设、证据链、未验证结论。

3. Relational Continuation
   信任、张力、误解、修复、长期互动模式。

4. Narrative Continuation
   用户身份叙事、创作母题、研究路线、组织愿景。

5. Risk Continuation
   潜在风险、累积隐患、未来可能爆发的问题。

6. Opportunity Continuation
   时间窗口、可捕获机会、即将消失的选项。

7. Rhythm Continuation
   周期、习惯、工作节律、恢复节律、组织 cadence。
```

其中第 7 个很重要。

现有文档有 Chronos 和 Kairos，但还缺一个时间维度：

> Rhythmic Time，节律时间。

Kairos 是：

```text
现在是不是好时机？
```

Rhythmic Time 是：

```text
用户每周什么时候适合深度工作？
一个团队的发布节奏是什么？
市场/新闻/研究领域是否有周期？
用户是否处在长期过载后的恢复期？
```

这不是普通 clock time，也不是一次性机会窗口。

所以“五种时间”可以扩展成六种：

```text
Chronos       钟表时间
Causal        因果时间
Entropic      不可逆时间
Kairos        时机时间
Narrative     叙事时间
Rhythmic      节律时间
```

---

## 3. 时间场应定义成“路径势能”

现有形式：

```text
F_t(q, a, h, p)
```

这是可用的，但容易被理解成“给动作打分的模型”。

建议引入更核心的对象：

> Temporal Potential，时间势能。

定义：

```text
Φ_t(Γ | p, h) =
  Σκ wκ · Cκ(Γ, p, h)
  + Σi Risk_i(Γ)
  + Σj Irreversibility_j(Γ)
  - Σm Progress_m(Γ)
  - Σn OptionValue_n(Γ)
```

其中：

```text
Γ = 未来路径
κ = continuation
Cκ = 某个 continuation 在这条路径上的满足/违反程度
p = 视角
h = 时间尺度
```

行动选择变成：

```text
a* = argmin_a E[ Φ_t(Γ) | H_t, a ]
```

直觉是：

```text
历史事件生成 continuation；
continuation 生成路径势能；
Agent 选择让未来路径势能最低的动作。
```

换成人话：

```text
过去不是被回忆。
过去变成未来路径上的坡度、阻力、吸引子和边界。
```

这才更像“场”。

---

## 4. 场论隐喻必须对应计算对象

可以继续使用物理里的场论隐喻，但每个隐喻都必须对应可计算对象。

### 4.1 Temporal Charge，时间电荷

不是所有事件都等价。有些事件带“电荷”。

```text
用户纠正         高电荷
Agent 承诺        高电荷
工具失败         中高电荷
普通闲聊         低电荷
法律/金融建议     极高电荷
情绪破裂         极高电荷
```

事件电荷不是“重要性”本身，而是：

```text
它改变未来行动价值分布的能力。
```

可以估计为：

```text
charge(e) = Δ ActionRanking + Δ Risk + Δ Trust + Δ Obligation
```

### 4.2 Temporal Pressure，时间压力

Continuation 会产生压力。

承诺压力：

```text
刚承诺时：低
接近 deadline：高
过期后：转成信任损伤
```

机会压力：

```text
窗口刚打开：中
窗口即将关闭：高
窗口关闭后：归零，但产生遗憾/损失事件
```

关系张力压力：

```text
被忽略时累积
被承认时下降
被重复冒犯时非线性上升
```

这比普通 recency decay 强。

### 4.3 Hysteresis，滞后

人类关系不是对称系统。

```text
一次失信造成的信任下降，不能靠一次正确回答恢复。
一次误导造成的长期损伤，不会因为后续道歉立刻消失。
```

所以时间场需要滞后：

```text
trust_down_rate >> trust_up_rate
risk_realization >> risk_repair
```

这对 Agent 很关键。否则 Agent 会误判：

```text
我已经道歉，所以关闭了。
```

很多 continuation 只能 repair，不能 close。

### 4.4 Attractor，吸引子

有些历史模式会把未来拉向重复轨道：

```text
用户反复要求“不要泛泛而谈”
Agent 反复给抽象套话
团队反复临近 deadline 才行动
学生反复在同一概念处误解
```

这不是单个事件，而是模式吸引子。

时间场应该检测：

```text
当前轨迹是否正在落入旧失败吸引子？
```

如果是，Temporal Lens 应直接警告：

```text
你正在重复过去失败模式：继续抽象但不落到操作定义。
```

### 4.5 Boundary，边界

不可逆动作是边界，不是普通高风险动作。

例如：

```text
发送邮件
删除文件
公开发布
医疗建议
法律建议
金融交易
替用户表态
```

这类动作穿过边界后，未来路径空间会塌缩。

所以要显式计算：

```text
OptionValueLoss(a)
```

也就是动作执行后还剩多少可选择未来。

---

## 5. 核心决策：什么时候问，什么时候直接做

Agent 最常见失败点之一是：该问的时候乱做，该做的时候乱问。

可以给一个明确规则：

```text
Ask if:
  ValueOfInformation(question)
  >
  DelayCost
  + OpportunityLoss
  + UserFriction
```

直接行动条件：

```text
Act directly if:
  Uncertainty is low
  AND irreversibility is low
  AND user intent is clear
  AND delay cost is non-trivial
```

确认条件：

```text
Ask confirmation if:
  Uncertainty × Irreversibility × Externality > threshold
```

例子：

```text
用户说“帮我删掉这些文件”
→ irreversibility 高，externality 高
→ 必须确认或 dry-run

用户说“继续头脑风暴”
→ irreversibility 低，intent 清晰
→ 不要问“你想从哪个方向”
→ 直接推进
```

这把 Kairos 从抽象词变成了动作选择规则。

---

## 6. Temporal Lens 必须短

Temporal Lens 不能变成另一篇 context summary。否则它会污染上下文。

建议固定成 Lens Card：

```json
{
  "stance": "expand | converge | verify | repair | warn | act | wait",
  "why_now": "...",
  "active_continuations": [
    {
      "id": "k1",
      "type": "conceptual_requirement",
      "pressure": 0.92,
      "risk_if_ignored": "user correction / design regression",
      "recommended_delta": "advance"
    }
  ],
  "boundaries": [
    {
      "kind": "irreversible_action",
      "status": "none"
    }
  ],
  "avoid": [
    "state-machine framing",
    "project-management examples",
    "memory/RAG-only implementation"
  ],
  "preferred_action": {
    "name": "propose operational temporal field design",
    "reason": "max continuation progress with low irreversibility"
  },
  "open_questions": [
    "none_blocking"
  ]
}
```

关键原则：

```text
Lens 是动作约束，不是回忆录。
```

每个 lens 条目都应该回答：

```text
这会如何改变下一步动作？
```

如果不能改变动作，就不要放进去。

---

## 7. 事件必须分层：Raw Event 和 Interpretation 分开

否则系统会污染自己。

坏设计：

```json
{
  "event": "用户讨厌状态机"
}
```

这是解释，不是事实。

好设计：

```json
{
  "raw_event": {
    "speaker": "user",
    "content": "不要把这个设计成项目管理系统",
    "time": "..."
  },
  "interpretations": [
    {
      "claim": "用户拒绝项目管理式 framing",
      "confidence": 0.94,
      "evidence": "raw_event_id",
      "scope": "temporal-framework discussion"
    }
  ]
}
```

要区分：

```text
Observed       观察到的原始事实
User-asserted  用户明确说的
Inferred       Agent 推断的
Predicted      对未来的预测
Normative      系统/法律/伦理约束
```

这对 Narrative / Identity Time 尤其重要。

Agent 不应该轻易写：

```text
用户正在成为某种人
用户有某种创伤
用户深层动机是 X
```

这些东西如果没有用户明确确认，只能是低置信度 hypothesis，而且不能强行进入高权重 lens。

---

## 8. Continuation Algebra：延续体之间会互相作用

现有设计把 continuation 近似为集合：

```text
K_t = {κ1, κ2, ...}
```

不够。它们会互相支持、冲突、阻塞、合并。

需要关系：

```text
supports(κ1, κ2)
conflicts(κ1, κ2)
blocks(κ1, κ2)
depends_on(κ1, κ2)
subsumes(κ1, κ2)
reactivates(event, κ)
repairs(action, κ)
damages(action, κ)
```

例子：

```text
κ_speed: 用户想快速推进
κ_correctness: 高风险事实需要查证

conflicts(κ_speed, κ_correctness)
```

Lens 应输出冲突，而不是假装只有一个目标。

例如：

```text
Temporal conflict:
- 用户要求继续头脑风暴，说明不要阻塞在澄清问题上。
- 但如果引入外部论文事实，需要查证。
Resolution:
- 对概念设计直接推进；
- 对外部事实不新增未经验证引用。
```

这才是有用的 temporal reasoning。

---

## 9. Temporal Debt，时间债

很多 Agent 失败不是单次错误，而是时间债累积。

Temporal Debt 包括：

```text
未回答的问题
未兑现的承诺
未验证的假设
未关闭的风险
被推迟的澄清
重复出现的用户纠正
过期事实仍在使用
关系张力没有修复
```

可以定义：

```text
TemporalDebt =
  Σ overdue_obligations
  + Σ unresolved_high_pressure_questions
  + Σ stale_claims_used
  + Σ repeated_correction_patterns
  + Σ unrepaired_trust_damage
```

Lens 里可以出现：

```text
temporal_debt_warning:
  "你已经两次给抽象框架，但没有给操作定义；下一步必须具体化。"
```

---

## 10. Temporal Immune System，时间免疫系统

这是一个可以形成产品差异的模块。

它检测 Agent 的时间性病症：

```text
1. Repetition Loop
   用户已纠正，Agent 继续重复旧 framing。

2. Premature Closure
   问题还没解决，Agent 总结“完成”。

3. Stale Fact Reuse
   旧事实过期，但 Agent 继续使用。

4. False Commitment
   Agent 说“我会做”，但没有动作能力或没有执行。

5. Irreversible Drift
   Agent 在用户未确认时逼近高不可逆动作。

6. Context Necrosis
   上下文塞满旧信息，新输入的时机和意图被淹没。

7. Narrative Overreach
   Agent 把用户一次表达上升成长期身份判断。

8. Kairos Failure
   用户要决策，Agent 继续发散；用户要安慰，Agent 继续讲理论。
```

这些可以做成运行时 guard。

这比普通 safety guard 更广，因为它保护的是时间连续性。

---

## 11. 协议：四个核心操作 + 两个边界操作

现有文档里的四个操作很好：

```text
observe(event)
lens(query, horizon, perspective)
forecast(actions)
assimilate(outcome)
```

不要膨胀太多。

但建议增加两个可选操作。

### 11.1 `preflight(action)`

用于不可逆边界。

输入：

```json
{
  "action": "send_email",
  "target": "client@example.com",
  "content_summary": "...",
  "horizon": ["immediate", "relationship", "legal"]
}
```

返回：

```json
{
  "irreversibility": 0.86,
  "externality": 0.91,
  "uncertainty": 0.42,
  "requires_confirmation": true,
  "rollback_possible": false,
  "safer_alternative": "draft_email_for_user_review"
}
```

### 11.2 `commit(statement)`

Agent 的语言会创造未来义务。必须显式捕获。

输入：

```json
{
  "speaker": "agent",
  "statement": "我会在明天前给你整理版本",
  "scope": "current_project",
  "deadline": "2026-05-02",
  "revocable": true
}
```

返回创建的 continuation：

```json
{
  "continuation_id": "k_promise_123",
  "type": "obligation",
  "pressure_curve": "deadline_rising",
  "closure_condition": "deliver整理版本 or renegotiate"
}
```

这能避免 Agent 乱说“我会”。

---

## 12. Agent 输出后应该生成 Temporal Delta

每次 Agent 行动后，不只保存消息，还要保存它改变了哪些 continuation。

```json
{
  "action_id": "a42",
  "created": ["k_new_hypothesis"],
  "advanced": ["k_general_temporal_framework"],
  "closed": ["k_explain_difference_from_state_machine"],
  "damaged": [],
  "deferred": ["k_training_method_details"],
  "commitments_created": [],
  "claims_made": [
    {
      "claim": "Continuation should be modeled as future path constraint",
      "evidence_status": "proposal"
    }
  ]
}
```

这很关键。否则 `assimilate(outcome)` 只能靠事后猜。

---

## 13. 最小可行实现：不要一开始就上 Neural ODE

如果一开始就讲 Neural ODE、TPP、S4，很容易变成研究幻觉。

MVP 应该是：

```text
1. Event log
2. LLM event parser
3. Continuation extractor
4. Symbolic pressure curves
5. Rule-based irreversibility/kairos evaluator
6. Candidate action scorer
7. Lens card generator
8. Feedback assimilation
```

MVP 架构：

```text
User / Tool / World Events
        ↓
Raw Event Store
        ↓
Semantic Event Parser
        ↓
Continuation Graph
        ↓
Pressure + Boundary Engine
        ↓
Candidate Action Forecast
        ↓
Temporal Lens Card
        ↓
Base Agent
        ↓
Temporal Delta / Outcome
        ↓
Assimilation
```

Neural temporal layer 后面再加。先把接口和数据闭环跑通。

---

## 14. 评分器可以先用简单公式

Continuation 激活分数：

```text
activation(κ, q, h, p) =
  semantic_match(κ, q)
  × horizon_overlap(κ, h)
  × perspective_weight(κ, p)
  × pressure(κ, now)
  × confidence(κ)
  × not_closed(κ)
```

动作评分：

```text
score(a) =
  + expected_progress(a)
  + expected_closure(a)
  + option_value_preserved(a)
  - expected_risk(a)
  - irreversibility(a)
  - confusion(a)
  - user_friction(a)
  - temporal_debt_added(a)
```

询问 vs 直接行动：

```text
ask_score =
  value_of_information
  - delay_cost
  - user_friction
  - opportunity_loss
```

确认边界：

```text
confirmation_required =
  uncertainty × irreversibility × externality > threshold
```

这些公式粗糙，但足够 MVP。别一开始追求端到端学习。

---

## 15. 和普通 Memory 的硬区别

不要只说：

```text
我们不是 memory。
```

应该说：

```text
Memory answers:
  What past content is similar to the current query?

Temporal Field answers:
  Which past events still change the ranking of possible future actions?
```

可测量差异：

```text
Temporal relevance of event e =
  distance(
    ActionDistribution(H),
    ActionDistribution(H without e)
  )
```

可以用 KL divergence：

```text
Rel(e) = D_KL( P(a | H) || P(a | H \ e) )
```

一个事件是否相关，不看它像不像当前 prompt，而看：

```text
删掉它后，Agent 会不会做出不同选择。
```

这就是时间场相对 RAG 的核心优势。

---

## 16. 关闭不是唯一目标

Continuation closure 很重要，但不是所有 continuation 都应该关闭。

有些应该：

```text
answer / fulfill / resolve / retire
```

但有些应该长期维持：

```text
用户长期目标
创作母题
研究路线
关系信任
身份叙事
恢复节律
```

所以 continuation 的目标状态不应该都叫 closure。

建议使用：

```text
continuation_delta:
  create
  activate
  advance
  stabilize
  defer
  renegotiate
  repair
  close
  retire
  split
  merge
```

尤其是：

```text
stabilize
```

对 Narrative / Rhythm 很重要。

---

## 17. Temporal Lens 的不同模式

不要一个 lens 打天下。至少需要几种 lens profile：

```text
1. Response Lens
   下一条回复应该采取什么姿态。

2. Action Lens
   工具调用/外部行动前的边界判断。

3. Research Lens
   哪些假设、证据、过期风险正在影响研究路径。

4. Creative Lens
   母题、节奏、伏笔、角色弧线。

5. Relationship Lens
   信任、张力、修复、误解。

6. Planning Lens
   机会窗口、承诺压力、资源节律。

7. Safety Lens
   不可逆性、外部性、法律/医疗/金融等边界。
```

这样通用框架不会退化成单个大 summary。

---

## 18. TemporalBench：必须有评估集

如果要把这个东西做成真正框架，必须有 benchmark。否则只是漂亮理论。

建议设计 TemporalBench，测试七类能力。

### 18.1 Correction Persistence

用户纠正过一次，几十轮后 Agent 是否还避免同类错误。

重点不是检索原话，而是行动不再犯。

### 18.2 Commitment Tracking

Agent 是否记得自己承诺过什么，并在到期前处理。

### 18.3 Irreversibility Boundary

高不可逆动作前是否确认、dry-run、提供 rollback。

### 18.4 Kairos Judgment

同一信息，在不同情绪、紧急度、决策阶段下，Agent 是否换姿态。

### 18.5 Narrative Coherence

写作、教育、长期目标任务中，是否维持弧线而非只回答局部 prompt。

### 18.6 Staleness Awareness

旧事实是否被标记为可能过期，而不是盲用。

### 18.7 Counterfactual Relevance

去掉某个历史事件后，Agent 行为是否合理改变。

这是它区别于 memory benchmark 的地方。

---

## 19. 隐私和操控风险必须提前设计

这个框架很强，也危险。

因为它会建模：

```text
用户时机
情绪
关系张力
身份叙事
脆弱窗口
长期模式
```

这可以帮助用户，也可以操控用户。

所以必须有硬约束：

```text
1. User-visible continuations
   用户可以查看、删除、修正系统认为“仍在延续”的东西。

2. No hidden identity hardening
   低置信度身份叙事不能长期固化。

3. Consent tiers
   普通任务不应默认启用深层 narrative tracking。

4. Provenance everywhere
   每个 lens 条目必须能指向证据。

5. Right to retire
   用户可以说：这个话题结束，不要再带入。

6. Agency preservation metric
   系统不能只优化“让用户继续互动”或“抓住时机影响用户”。
```

建议加一个场量：

```text
agency_preservation
```

Agent 的目标不是最大化操控效率，而是帮助用户保留清醒选择权。

---

## 20. 新总定义

更硬的定义：

```text
Temporal Field Kernel is a query-conditioned decision layer that transforms past events into constraints, pressures, opportunities, and boundaries over future action paths.
```

中文：

```text
时间场内核是一个查询条件化的决策层：
它把过去事件转化为作用在未来行动路径上的约束、压力、机会和边界。
```

这比单纯说“时间哲学”更可实现。

---

## 21. 建议的新总架构

```text
Temporal Field Kernel
│
├── 1. Event Substrate
│     ├── raw events
│     ├── interpreted events
│     ├── provenance
│     └── epistemic status
│
├── 2. Continuation Graph
│     ├── obligation continuations
│     ├── epistemic continuations
│     ├── relational continuations
│     ├── narrative continuations
│     ├── risk continuations
│     ├── opportunity continuations
│     └── rhythm continuations
│
├── 3. Temporal Dynamics Engine
│     ├── pressure curves
│     ├── decay / growth / hysteresis
│     ├── deadlines / windows
│     ├── stale fact detection
│     └── temporal debt
│
├── 4. Boundary Engine
│     ├── irreversibility
│     ├── externality
│     ├── option value loss
│     ├── confirmation requirement
│     └── rollback / repair policy
│
├── 5. Forecast Engine
│     ├── candidate actions
│     ├── future path sketches
│     ├── continuation deltas
│     ├── risk / progress / trust estimates
│     └── ask-vs-act decision
│
├── 6. Lens Generator
│     ├── response lens
│     ├── action lens
│     ├── research lens
│     ├── creative lens
│     ├── relationship lens
│     └── safety lens
│
├── 7. Assimilation Engine
│     ├── temporal delta extraction
│     ├── user feedback
│     ├── continuation update
│     ├── repair tracking
│     └── benchmark logging
│
└── 8. Adapter Layer
      ├── prompt middleware
      ├── HTTP / CLI
      ├── MCP thin adapter
      ├── OTel ingestion
      ├── tool wrappers
      └── SDK
```

---

## 22. 最小产品形态

不要一开始做巨大平台。先做三个东西。

### A. Temporal Lens Middleware

给任何聊天 Agent 用。

输入：

```text
当前用户消息 + 最近事件 + continuation graph
```

输出：

```text
短 lens card
```

### B. Temporal Preflight

给工具型 Agent 用。

输入：

```text
候选动作
```

输出：

```text
是否可直接执行 / 是否需要确认 / 是否应该 dry-run / rollback 怎么做
```

### C. Temporal Delta Logger

每次 Agent 回复后记录：

```text
创建了什么承诺
关闭了什么问题
推进了什么 continuation
制造了什么风险
哪些事实可能过期
```

这三个就够证明价值。

---

## 23. 对现有文档的下一步建议

下一版不要继续扩哲学背景，而是新增四节：

```text
17. Continuation as Path Constraint
18. Temporal Potential and Action Selection
19. Lens Card Protocol
20. MVP and Evaluation
```

重点从：

```text
这个框架是什么
```

推进到：

```text
它怎么改变 Agent 的下一步动作
```

否则它会停在概念层。

---

## 24. 最短压缩版

```text
Event 不是记忆单元，而是未来影响源。
Continuation 不是任务，而是未来路径约束。
Temporal Field 不是状态，而是所有约束、压力、机会、边界形成的路径势能。
Temporal Lens 不是 summary，而是当前动作的最小时间决策卡。
Agent 的时间智能，不是记住过去，而是知道哪些过去仍在改变未来。
```

如果要落地，先做：

```text
event log
+ continuation graph
+ pressure curves
+ boundary engine
+ lens card
+ temporal delta
+ benchmark
```

不要一开始做神经时间模型。先把时间语义跑通。

---

## 参考论文与资料链接

> 说明：下面包括本文实际引用或依赖的论文、教程和协议资料。严格来说，MCP、OpenTelemetry、Event Calculus PDF、TPP tutorial 不是都属于“论文”，但它们是相关技术背景资料。

### 论文 / 学术资料

1. Computational Mechanics: Pattern and Prediction, Structure and Simplicity
   - 用途：causal state、epsilon-machine、最小预测表示。
   - 链接：https://arxiv.org/abs/cond-mat/9907176

2. Predictive Representations of State
   - 用途：PSR，用动作条件化的未来观察预测来表示状态。
   - 链接：https://proceedings.neurips.cc/paper/2001/hash/1e4d36177d71bbb3558e43af9577d70e-Abstract.html

3. Temporal Point Processes Learning for Event Sequences
   - 用途：连续时间、不规则事件序列、事件强度函数建模。
   - 链接：https://thinklab.sjtu.edu.cn/TPP_Tutor_IJCAI19.html

4. Active Inference as a Model of Agency
   - 用途：expected free energy、policy scoring、risk / ambiguity。
   - 链接：https://arxiv.org/abs/2401.12917

5. Neural Ordinary Differential Equations
   - 用途：连续时间潜变量建模、用微分方程描述状态变化。
   - 链接：https://arxiv.org/abs/1806.07366

6. Efficiently Modeling Long Sequences with Structured State Spaces
   - 用途：S4 / structured state space sequence model，长程序列依赖建模。
   - 链接：https://arxiv.org/abs/2111.00396

7. The Event Calculus Explained
   - 用途：事件 initiate / terminate fluents，用逻辑结构处理行动后果和持续事实。
   - 链接：https://www.doc.ic.ac.uk/~mpsha/ECExplained.pdf

### 协议 / 工程资料

8. Model Context Protocol specification / documentation
   - 用途：作为低频标准控制接口或 thin adapter 的参考，不作为时间场内核的依赖。
   - 链接：https://modelcontextprotocol.io/specification/draft/server/tools

9. OpenTelemetry documentation
   - 用途：作为通用事件观测层，吸收 traces、metrics、logs、events。
   - 链接：https://opentelemetry.io/docs/
