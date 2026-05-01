# 你说得对：之前的设计仍然太“项目状态机”

之前那套设计有三个根本问题：

1. **过度绑定 coding/project 场景**
   `.chronos/current-state.md`、文件变更、测试结果、项目约束，这些确实更像 coding agent 的增强器，不是通用 Agent 时间框架。
2. **太像状态机**
   虽然我用了 causal state、PSR、不可逆性这些词，但落地形式仍然是“维护状态、更新状态、输出状态”。这和当前 Agent 的 task state / memory / context summary 没有本质差异。
3. **没有真正把“时间哲学”抽象成 Agent 的通用认知结构**
   时间不应该只是“任务进度”。它应该决定 Agent 如何理解连续性、机会、后果、承诺、身份、环境节律、叙事、风险和未来可能性。

所以要重新构思。

------

# 1. 新定义：它不是 Temporal State，而是 Temporal Field

我会把这个框架重新定义为：

> **Temporal Field Kernel，时间场内核。**

它不是状态机，不是 memory，不是任务管理器。
它是一个作用在 Agent 周围的**时间场**。

状态机的形式是：
$$
s_{t+1} = T(s_t, a_t)
$$
这仍然是“当前状态 + 转移”。

时间场的形式应该是：
$$
\mathcal{F}_t(q, a, h, p)
$$
其中：

| 符号            | 含义                                             |
| --------------- | ------------------------------------------------ |
| (q)             | 当前问题、意图、感知输入                         |
| (a)             | 候选行动                                         |
| (h)             | 未来时间尺度，例如下一句话、今天、本周、长期关系 |
| (p)             | 视角，例如用户、Agent、第三方、环境、制度        |
| (\mathcal{F}_t) | 当前历史生成的时间场                             |

它回答的不是：

> 当前状态是什么？

而是：

> 在当前历史影响下，不同动作在不同时间尺度、不同视角中会产生什么后果？

这就从“状态维护”变成了“时间场评估”。

------

# 2. 时间场的核心直觉

物理里，场不是一个点的状态，而是空间中每个位置都能感受到的影响结构。
类比到 Agent：

> **过去事件不是被存起来的记忆，而是像质量、电荷、边界条件一样，在当前形成一个时间场。**

过去影响现在，不是因为它被检索到了，而是因为它仍然改变行动的未来分布。

所以时间场不是：

```text
过去事件列表
当前任务状态
用户偏好表
```

而是：

```text
历史轨迹对未来行动的影响函数
```

更形式化：
$$
\mathcal{F}_t(q, a, h, p)

\int_{\tau < t}
K_\theta(q, a, h, p; e_\tau)
\phi(e_\tau)
d\tau
$$
这里：

| 项             | 含义                               |
| -------------- | ---------------------------------- |
| (e_\tau)       | 过去事件                           |
| (\phi(e_\tau)) | 事件的语义/情绪/承诺/因果/风险表征 |
| (K_\theta)     | 学到的时间影响核                   |
| (q,a,h,p)      | 当前查询、动作、时间尺度、视角     |

这不是普通 recency decay。
一个十分钟前的闲聊可能权重为零；一个十年前的创伤、合同、承诺、研究结论、身份变化，可能仍然强烈影响当前行动。

这才更接近人类时间感。

------

# 3. 通用时间框架要覆盖五种时间

一个通用 Agent 不能只知道 clock time。它需要五种时间。

## 3.1 Chronos：钟表时间

这是最基础的：

```text
现在几点
多久以前
截止日期
持续时间
间隔
时区
```

它用于日程、旅行、新闻、金融、医疗、法律、会议等任务。

------

## 3.2 Causal Time：因果时间

不是“先后”，而是：

```text
什么导致了什么
什么只是同时发生
哪个行动改变了未来
哪个事件只是噪声
```

Computational mechanics 中的 causal state 思想是：若两段历史对未来预测等价，它们可归为同一预测状态；其 epsilon-machine 被描述为与准确预测一致的最小表示。这个思想可转译为：Agent 不必记住全部过去，只需保留会改变未来预测的历史影响。([arXiv](https://arxiv.org/abs/cond-mat/9907176?utm_source=chatgpt.com))

------

## 3.3 Entropic Time：不可逆时间

这是“时间箭头”。

Agent 需要知道：

```text
哪些事可撤回
哪些事不可撤回
哪些事会留下痕迹
哪些事会改变他人的世界
哪些事会锁死未来路径
```

例如：

| 行动                     | 不可逆性 |
| ------------------------ | -------- |
| 思考一个方案             | 极低     |
| 写一段草稿               | 低       |
| 给用户发建议             | 中       |
| 发送邮件                 | 高       |
| 发布内容                 | 高       |
| 删除数据                 | 极高     |
| 医疗/法律/金融建议被执行 | 极高     |

这不是项目管理问题，而是任何 Agent 行动的核心。

------

## 3.4 Kairos：时机时间

人类做事不只看“现在几点”，还看：

```text
现在是不是合适的时机
这个窗口是否即将关闭
用户现在是否需要安慰而不是分析
这个问题是否应该先问清楚
这个机会是否再等就消失
```

Kairos 是“时机感”。

很多 Agent 失败不是因为不知道信息，而是因为**时机错了**：

- 用户愤怒时继续解释技术细节；
- 用户已经要决策了还继续发散；
- 需要紧急行动时还在补充背景；
- 创作任务中在不该收束时提前收束；
- 谈判中错过让步窗口。

------

## 3.5 Narrative / Identity Time：叙事时间

这是人类最重要、而当前 Agent 最弱的一层。

人不是只在完成任务。
人有：

```text
长期目标
自我叙事
关系历史
价值变化
创伤/期待
风格连续性
承诺与失信
反复出现的模式
```

Agent 若要真正理解人类，必须知道：

```text
这件事在用户人生/组织/关系/研究/创作中的位置是什么？
这不是单次任务，而是哪条长期轨迹的一部分？
```

这和项目管理无关。
这适用于心理支持、教育、写作、战略、研究、个人成长、团队协作、谈判、法律、医疗、旅行、生活助理等所有任务。

------

# 4. 关键转向：从“状态”改成“Continuation”

状态机维护的是：

```text
现在处于状态 S
```

但人类时间感更像：

```text
有什么事情正在延续？
有什么张力尚未解决？
有什么承诺仍在未来等待兑现？
有什么问题仍悬而未决？
有什么模式正在重复？
```

所以核心对象不应该是 state，而应该是：

> **Continuation，延续体。**

一个 continuation 是一个尚未关闭的时间结构：
$$
\kappa =
\langle
origin,
tension,
expectation,
horizon,
termination,
value
\rangle
$$
例子：

| Continuation     | 说明                       |
| ---------------- | -------------------------- |
| 一个未回答的问题 | 需要未来某个回答关闭       |
| 一个承诺         | 需要未来行动兑现           |
| 一个用户长期目标 | 跨越多次对话               |
| 一个研究假设     | 需要证据验证或推翻         |
| 一个创作母题     | 需要在文本中持续保持       |
| 一个关系张力     | 需要被承认、修复或避免恶化 |
| 一个风险         | 未来可能爆发               |
| 一个机会         | 有时间窗口                 |
| 一个身份叙事     | 用户正在成为某种人         |

Agent 的时间框架不应问：

> 当前状态是什么？

而应问：

> 当前有哪些 continuation 正在延续？
> 哪些需要推进？
> 哪些需要关闭？
> 哪些正在互相冲突？
> 哪些被当前输入重新激活？

这比状态机更一般。

------

# 5. Temporal Field Kernel 的真正核心

它由四个对象构成：

```text
Event Stream
Continuation Space
Temporal Field
Temporal Lens
```

## 5.1 Event Stream：事件流

任何 Agent 感知到的东西都是事件：

```text
用户说话
用户沉默
用户纠正
用户情绪变化
网页内容更新
环境反馈
工具执行结果
他人回应
文件变化
价格变化
新闻变化
模型输出
Agent 自己承诺
Agent 自己失败
```

事件不是项目事件，而是任意交互事件。

标准形式：

```json
{
  "time": "...",
  "source": "user | agent | tool | world | institution | memory",
  "modality": "text | audio | image | action | environment | social",
  "content": "...",
  "act_type": "ask | assert | promise | reject | correct | observe | decide | act",
  "affective_tone": "...",
  "stakes": "...",
  "irreversibility": "...",
  "evidence_status": "...",
  "linked_continuations": []
}
```

------

## 5.2 Continuation Space：延续空间

系统不是维护一个状态，而是维护一个空间：
$$
\mathcal{K}_t = {\kappa_1, \kappa_2, ..., \kappa_n}
$$
每个 continuation 都有：

```text
来源
当前张力
时间尺度
价值权重
关闭条件
冲突对象
未来风险
可行动作
```

例如用户说：

> “继续构思，不要只停留在工程。”

这会激活几个 continuation：

```text
κ1: 用户追求一个更通用的时间框架
κ2: 用户不满意状态机式设计
κ3: 用户要求创新性
κ4: 用户希望数学/物理视角真正落地
κ5: 当前回答必须推进构思，而不是总结旧方案
```

这不是状态。
这是一个延续空间。

------

## 5.3 Temporal Field：时间场

每个过去事件和 continuation 都对当前形成影响。
$$
\mathcal{F}_t(q,a,h,p)
$$
它输出一组场量：

```text
expected_progress
expected_confusion
expected_user_correction
expected_trust_change
expected_irreversibility
expected_information_gain
expected_continuation_closure
expected_continuation_damage
expected_opportunity_loss
```

这就让 Agent 的行动不再只是：

```text
回答当前问题
```

而是：

```text
选择一条能在多个时间尺度上最好推进 continuation 的路径
```

------

## 5.4 Temporal Lens：时间透镜

Agent 不直接读取整个时间场。
它通过一个小型透镜使用时间场。
$$
L_t = \Pi(\mathcal{F}_t, q, B)
$$
其中 (B) 是上下文预算。

Temporal Lens 输出给 Agent 的不是 memory，而是：

```text
当前输入在长期轨迹中的位置
被激活的 continuation
高影响历史痕迹
当前时机判断
不可逆边界
未来几条可能路径
建议的 temporal stance
```

示例：

```text
Temporal Lens

Activated continuations:
- 用户正在逼近一个通用 Agent 时间理论，而不是项目管理方案。
- 用户已拒绝“状态机化”和“memory 化”的方案。
- 当前回答应提出新的抽象结构，并解释其可落地性。

Temporal stance:
- 不要再给 coding/project 例子作为主轴。
- 以 Agent 对任意世界任务的时间理解为对象。
- 以 continuation、temporal field、path functional、lens 作为核心。

Likely failure:
- 如果继续讲状态表、project hooks、MCP 文件投影，用户会认为没有创新。
```

这就是 Agent 实际需要的东西。

------

# 6. 算法不是状态机，而是“历史影响核 + 未来路径泛函”

## 6.1 学习时间影响核

过去事件对当前行动的影响由 (K_\theta) 决定：
$$
\mathcal{F}_t(q, a, h, p)

\sum_{i<t}
K_\theta(q, a, h, p; e_i)
\phi(e_i)
$$
这个核必须学会：

```text
哪些旧事件仍然重要
哪些近期事件只是噪声
哪些承诺永不过期
哪些偏好会随情境改变
哪些失败模式会复现
哪些关系张力会长期存在
```

训练目标不是复原历史，而是预测未来：
$$
\min_\theta
-\log P_\theta(e_{t+1:t+H} \mid H_t, a_t)
+
\beta I(z_t; H_t)
$$
也就是：

> 用尽可能压缩的历史表征，预测未来事件、反馈、纠正、成功、失败、风险、机会。

这和 Predictive State Representation 的精神一致：PSR 用动作条件化的未来观察预测来表示系统状态，而不是假设一个隐藏状态。([NeurIPS Proceedings](https://proceedings.neurips.cc/paper/2001/hash/1e4d36177d71bbb3558e43af9577d70e-Abstract.html?utm_source=chatgpt.com))

------

## 6.2 用 temporal point process 建模事件发生

很多任务不是固定步长的。

人类时间是稀疏、突发、不规则的：

```text
用户突然改需求
市场突然变化
某个机会窗口出现
几周后一个承诺到期
某个长期关系张力被再次激活
```

这更适合用 temporal point process。TPP 是用于描述连续时间事件序列的数学框架，事件通常带有时间戳和属性，也常被用于预测和干预动态行为。([ThinkLab](https://thinklab.sjtu.edu.cn/TPP_Tutor_IJCAI19.html?utm_source=chatgpt.com))

形式上：
$$
\lambda_k(t \mid H_t)
$$
表示未来某类事件在时间 (t) 发生的强度。

例如：

```text
λ_user_correction(t)
λ_deadline_pressure(t)
λ_opportunity_closes(t)
λ_trust_loss(t)
λ_followup_needed(t)
λ_fact_staleness(t)
```

这样 Agent 能获得：

```text
这个问题现在不回答会不会变坏？
这个事实多久后可能过期？
这个承诺什么时候会开始产生压力？
用户纠正概率是否正在升高？
```

这比普通 memory 系统强很多。

------

## 6.3 用路径泛函选择行动

Agent 的行动不是一步决策，而是一条路径。
$$
\Gamma =
(e_t, a_t, e_{t+1}, a_{t+1}, ...)
$$
定义路径作用量：
$$
\mathcal{S}(\Gamma)

\int [ \lambda_1 U(\tau) + \lambda_2 R(\tau) + \lambda_3 I(\tau) + \lambda_4 C(\tau)

- \lambda_5 G(\tau)

- \lambda_6 M(\tau)
]
d\tau
$$
其中：

| 项   | 含义                                         |
| ---- | -------------------------------------------- |
| (U)  | 不确定性                                     |
| (R)  | 风险                                         |
| (I)  | 不可逆性                                     |
| (C)  | 认知/上下文/时间成本                         |
| (G)  | 目标进展                                     |
| (M)  | continuation closure，延续体被良好推进或关闭 |

行动选择：
$$
a^*

\arg\min_a
\mathbb{E}_{\Gamma \sim P(\Gamma \mid H_t, a)}
[
\mathcal{S}(\Gamma)
]
$$
这不是状态机。
这是路径选择。

它可以自然解释：

```text
什么时候该问问题
什么时候该直接行动
什么时候该安慰
什么时候该查证
什么时候该等待
什么时候该中断旧计划
什么时候该承认不确定
什么时候该关闭一个 continuation
```

Active inference 里也有类似思想：以 expected free energy 评分 policies，考虑未来观察中的风险和歧义。这个框架可借鉴，但不必完全照搬。([arXiv](https://arxiv.org/html/2401.12917v1?utm_source=chatgpt.com))

------

## 6.4 用连续时间模型，而不是离散状态机

很多 Agent 框架天然是离散 turn：

```text
用户消息 → Agent 回复 → 工具调用 → 回复
```

但通用时间框架应该允许连续时间。

可以借鉴 Neural ODE 的形式：
$$
\frac{dz(t)}{dt}

f_\theta(z(t), u(t), t)
$$
Neural ODE 把隐藏状态的导数参数化为神经网络，并用微分方程求解器计算输出，适合连续时间潜变量建模。([arXiv](https://arxiv.org/abs/1806.07366?utm_source=chatgpt.com))

在这里，(z(t)) 不是“项目状态”，而是时间场的潜在生成结构：

```text
信任如何随时间变化
机会窗口如何衰减
承诺压力如何增长
事实可信度如何过期
用户情绪如何缓慢恢复或恶化
长期目标如何稳定存在
```

这比“turn-based 状态表”更接近真实时间。

------

# 7. 通用任务中的时间场如何工作？

## 7.1 研究任务

普通 Agent：

```text
检索资料 → 总结
```

时间场 Agent：

```text
这个结论是否过期？
这个领域最近是否发生范式变化？
这篇论文在证据链中处于什么位置？
哪些假设还没被验证？
用户的研究路线正在通向哪里？
现在应该扩展、收敛、质疑，还是设计实验？
```

这里的 continuation 是：

```text
研究问题
证据链
未验证假设
理论张力
知识边界
```

------

## 7.2 写作任务

普通 Agent：

```text
按要求写一段
```

时间场 Agent：

```text
这个文本的叙事节奏到了哪里？
之前铺垫的意象是否需要回收？
读者期待是否应该满足或打破？
角色弧线是否连续？
现在该扩张、转折、沉默，还是收束？
```

continuation 是：

```text
主题
母题
人物弧线
情绪节奏
未解决冲突
```

------

## 7.3 个人助理任务

普通 Agent：

```text
安排日程
```

时间场 Agent：

```text
用户最近是否过载？
这个安排会不会破坏恢复窗口？
这个承诺是否与长期目标冲突？
某个关系是否需要及时回应？
今天适合深度工作还是轻任务？
```

continuation 是：

```text
精力节律
社交义务
长期目标
恢复需求
承诺压力
```

------

## 7.4 谈判/沟通任务

普通 Agent：

```text
生成回复
```

时间场 Agent：

```text
关系历史是什么？
对方让步窗口在哪里？
现在强硬是否会损害长期信任？
过去哪句话埋下了张力？
这次回复是在推进关系还是赢得短期优势？
```

continuation 是：

```text
信任
立场变化
未明说的张力
对方预期
关系未来
```

------

## 7.5 学习/教育任务

普通 Agent：

```text
解释概念
```

时间场 Agent：

```text
用户当前卡在哪个认知阶段？
之前误解是否还在影响现在？
现在该给定义、类比、练习，还是反例？
什么时候该让用户自己推导？
```

continuation 是：

```text
理解轨迹
误区
掌握度
认知负荷
迁移能力
```

------

# 8. 这才是通用 Agent 的“时间理解”

通用时间框架应让 Agent 具备这些能力：

| 能力       | 不是            | 是                       |
| ---------- | --------------- | ------------------------ |
| 记住过去   | 存聊天记录      | 判断过去是否仍改变未来   |
| 理解现在   | 当前状态表      | 当前 continuation 场     |
| 面向未来   | 计划步骤        | 比较未来路径             |
| 处理承诺   | todo list       | 未来被语言行为约束       |
| 理解人     | 用户偏好表      | 用户长期叙事与情境时机   |
| 避免错误   | 安全规则        | 不可逆性与路径锁定       |
| 上下文选择 | top-k retrieval | 最小预测充分透镜         |
| 自我理解   | agent state     | 自身行动轨迹对未来的影响 |

------

# 9. 与现有 Agent 的差异

当前多数 Agent 的结构大致是：

```text
context window
+ memory retrieval
+ tool use
+ plan/act loop
+ task state
```

Temporal Field Kernel 的结构是：

```text
event stream
+ continuation space
+ learned temporal influence kernel
+ path forecast
+ action functional
+ temporal lens
```

对比：

| 当前 Agent         | Temporal Field Agent             |
| ------------------ | -------------------------------- |
| 关注当前 prompt    | 关注 prompt 在长期轨迹中的位置   |
| 维护 task state    | 维护 continuation field          |
| 检索相似 memory    | 估计历史对未来的影响             |
| 生成计划           | 比较未来路径                     |
| 工具调用后更新状态 | 行动后更新时间场                 |
| 主要按步骤思考     | 按时间尺度、时机和不可逆性思考   |
| 单一上下文         | 多视角、多时间尺度 lens          |
| 偏工程任务         | 任意人类任务、社会任务、认知任务 |

------

# 10. 兼容主流 Agent：不要做插件，做“时间透镜协议”

为了兼容大部分 Agent，不应该要求它们深度接入。
需要一个通用协议：

> **Temporal Lens Protocol，时间透镜协议。**

它只需要四个操作。

## 10.1 `observe(event)`

任何 Agent、工具、环境都可以送入事件：

```json
{
  "source": "user",
  "modality": "text",
  "act_type": "correction",
  "content": "不要把这个设计成项目管理系统",
  "time": "...",
  "stakes": "high",
  "scope": "current_conversation"
}
```

------

## 10.2 `lens(query, horizon, perspective)`

Agent 请求当前时间透镜：

```json
{
  "query": "我要如何回答用户当前问题？",
  "horizon": ["next_response", "conversation_arc", "long_term_design"],
  "perspective": ["user", "agent", "system"]
}
```

返回：

```json
{
  "activated_continuations": [
    "用户要求通用时间框架",
    "用户拒绝项目管理式方案",
    "用户要求创新性和数学/物理视角"
  ],
  "temporal_stance": "constructive theoretical redesign",
  "likely_failure_modes": [
    "继续讲状态机",
    "继续讲 coding project",
    "继续讲 memory/RAG"
  ],
  "high_influence_traces": [
    "用户多次强调不要做重 memory",
    "用户要求不要只从工程出发"
  ],
  "recommended_response_shape": "提出 Temporal Field Kernel，而不是 task-state system"
}
```

------

## 10.3 `forecast(actions)`

Agent 提供候选行动：

```json
{
  "actions": [
    "继续解释原方案",
    "提出时间场框架",
    "询问用户想用于哪些 Agent",
    "给一个代码项目落地方案"
  ]
}
```

返回：

```json
{
  "best": "提出时间场框架",
  "scores": {
    "continuation_progress": 0.91,
    "user_correction_risk": 0.08,
    "conceptual_novelty": 0.86,
    "context_drift": 0.05
  }
}
```

------

## 10.4 `assimilate(outcome)`

Agent 行动后，系统吸收结果：

```json
{
  "action": "提出时间场框架",
  "outcome": "user_accepts | user_corrects | user_refines | task_closed",
  "new_events": []
}
```

这会更新时间场。

------

# 11. 兼容方式：任何 Agent 都能接

不依赖 MCP，但可兼容 MCP。

MCP 官方规范把它描述为连接 LLM 应用与外部数据源和工具的开放协议，因此它适合作为一种标准控制接口。([Model Context Protocol](https://modelcontextprotocol.io/specification/2025-11-25?utm_source=chatgpt.com)) 但时间场框架不应该依赖 MCP，因为高频事件流、连续时间建模、透镜投影都不适合完全通过 MCP 工具调用承载。

更通用的兼容方式是：

| 接入方式                 | 适合对象          | 作用                           |
| ------------------------ | ----------------- | ------------------------------ |
| Prompt middleware        | 所有聊天 Agent    | 在用户输入前插入 temporal lens |
| Browser/desktop observer | 通用工作流 Agent  | 观察环境事件                   |
| Tool wrapper             | 工具型 Agent      | 捕获行动与结果                 |
| CLI/HTTP                 | 本地 Agent        | 请求 lens / forecast           |
| MCP thin adapter         | 支持 MCP 的 Agent | 标准化低频调用                 |
| OTel/log ingestion       | 自动化系统        | 被动吸收 traces/logs/events    |
| SDK                      | 自研 Agent        | 深度集成                       |

OpenTelemetry 是开源、供应商中立的 observability 框架，用于生成、收集和导出 traces、metrics、logs；它可以作为通用事件观测层，而不是只服务编程项目。([OpenTelemetry](https://opentelemetry.io/docs/?utm_source=chatgpt.com))

所以架构是：

```text
Any Agent
  ↓
Temporal Lens Protocol
  ↓
Temporal Field Kernel
  ↓
Event Stream + Continuation Space + Influence Kernel
```

而不是：

```text
Coding Agent
  ↓
Project Memory
  ↓
State File
```

------

# 12. 技术实现：不是状态机，而是三模型混合

## 12.1 Symbolic layer：处理承诺、有效期、冲突

用逻辑结构处理：

```text
某承诺何时开始
何时终止
何时被违反
哪个事实被当前输入覆盖
哪个 continuation 被关闭
```

Event Calculus 适合这部分，因为它明确使用事件来 initiate 或 terminate fluents，用于表示行动及其对世界状态的影响。([Imperial College London Documentation](https://www.doc.ic.ac.uk/~mpsha/ECExplained.pdf?utm_source=chatgpt.com))

但我们只把它作为**符号约束层**，不是整个系统。

------

## 12.2 Neural temporal layer：处理连续时间影响

用：

```text
temporal point process
neural ODE
structured sequence model
long-context temporal transformer
```

来学习：

```text
事件何时再次出现
承诺压力如何随时间增长
用户纠正概率如何变化
事实过期风险如何变化
长期模式如何复现
```

S4 等 structured state space sequence models 被提出用于高效建模长序列依赖；这类模型适合做长程时间影响建模，但在这里它只是候选技术，不是有限状态机。([arXiv](https://arxiv.org/abs/2111.00396?utm_source=chatgpt.com))

------

## 12.3 LLM layer：处理语义抽象和透镜表达

LLM 不负责“记忆全部东西”。
LLM 负责：

```text
把事件转成语义事件
识别 continuation
生成 temporal lens 文本
模拟候选未来
解释冲突
```

核心学习和时间影响不应全靠 LLM prompt 记忆，而应由时间场模型维护。

------

# 13. 数据不是 memory，而是事件-延续-场

存储对象应分三类。

## 13.1 Event

```json
{
  "id": "e1",
  "time": "...",
  "source": "user",
  "act_type": "correction",
  "content": "不要做项目管理式设计",
  "semantic_tags": ["rejection", "scope_correction", "architecture"],
  "irreversibility": 0.1
}
```

## 13.2 Continuation

```json
{
  "id": "k1",
  "origin_event": "e1",
  "type": "conceptual_requirement",
  "tension": "framework must be general, not project-specific",
  "horizon": "current_thread_and_future_design",
  "closure_condition": "a general temporal field architecture is specified",
  "value": 0.95
}
```

## 13.3 Temporal Lens

```json
{
  "query": "respond_to_current_user",
  "activated_continuations": ["k1", "k2", "k3"],
  "temporal_stance": "redesign_from_first_principles",
  "avoid": ["state-machine framing", "project management examples"],
  "recommended_path": "define temporal field, continuation calculus, influence kernel, lens protocol"
}
```

这三者构成通用时间框架。

------

# 14. 如何让 Agent 真正变强？

它提升的不是“记忆量”，而是以下能力。

## 14.1 Temporal relevance

当前 Agent 经常不知道什么历史相关。

时间场问：

[
\Delta P(Future)
]

如果去掉某个过去事件，未来预测变化大，它就相关。

------

## 14.2 Temporal stance

不同时间情境需要不同姿态：

```text
解释
安慰
追问
总结
质疑
等待
警告
收束
发散
行动
暂停
```

Agent 不应只问“答什么”，还要问：

> 当前时间姿态是什么？

------

## 14.3 Continuation closure

很多任务失败，是因为 Agent 没有关闭应该关闭的 continuation：

```text
用户问的真正问题没有回答
承诺没有兑现
前面提出的假设没有验证
创作中的伏笔没有回收
研究中的冲突没有处理
```

时间场会显式追踪这些延续体。

------

## 14.4 Opportunity awareness

Agent 需要知道：

```text
现在该不该行动
继续等会不会损失机会
提问是否会拖慢用户
直接输出是否更合适
```

这就是 Kairos。

------

## 14.5 Irreversibility awareness

Agent 需要知道：

```text
哪些话说出口会改变关系
哪些建议可能被执行
哪些工具调用会改变世界
哪些误导会长期影响用户
```

这不是安全规则，而是时间箭头。

------

# 15. 最终框架形态

我现在会把整个设计定为：

```text
Temporal Field Kernel
│
├── Universal Event Stream
│   └── 所有感知、行动、反馈都转成事件
│
├── Continuation Space
│   └── 追踪未完成问题、承诺、张力、目标、叙事、机会、风险
│
├── Temporal Influence Kernel
│   └── 学习过去事件对未来行动的影响，而不是做 top-k memory
│
├── Path Forecast Engine
│   └── 对候选行动模拟未来路径和多时间尺度后果
│
├── Irreversibility / Kairos Evaluator
│   └── 判断不可逆性、时机、机会窗口和延迟代价
│
├── Temporal Lens Generator
│   └── 生成给 Agent 的最小时间透镜
│
└── Adapter Layer
    └── prompt middleware / CLI / HTTP / MCP / OTel / SDK
```

------

# 16. 一句话压缩

之前那套是：

> **Agent 的外部状态管理器。**

这不够。

真正通用的版本应该是：

> **Agent 周围的时间场：它把历史变成对未来的影响，把任务变成延续体，把行动变成路径选择，把上下文变成时间透镜。**

最终定义：

> **Temporal Field Kernel = event stream → continuation space → learned temporal influence field → path forecast → temporal lens.**

它不服务“项目管理”。
它服务所有 Agent 的核心能力：

```text
理解人类的长期性；
理解行动的后果；
理解时机；
理解承诺；
理解叙事连续性；
理解哪些过去仍然活着；
理解哪些未来正在形成。
```

这才更接近“时间哲学”对 Agent 的真正意义。
