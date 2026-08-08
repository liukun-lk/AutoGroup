# 最小化法分组设计方案

> 在 `docs/randomization_design.md`（§2.5 方法学定位、§8.2 随机源契约、第九章导出原则）
> 已定框架之上，回答 `GroupingMethod::Minimization` 如何从占位变成可用的
> 经典最小化法（Pocock–Simon 序贯协变量自适应随机化）。
>
> 文档版本: v0.2
> 创建日期: 2026-08-08
> 修订日期: 2026-08-08
> 状态: **已实现**（`core/grouping/minimizer.rs` + 导出 + 前端，v0.2 全部决策按推荐值落地）
> 前置依赖: randomization_design.md §2.5 / §8.2 / 第九章；现有随机化路径
> `src-tauri/src/core/grouping/randomizer.rs`（种子契约、校验、记录已实现）
>
> v0.1 → v0.2 的改动集中在三个方法学缺陷（分档口径、度量归一、p 的语义）与
> 一批遗漏的落地点，逐条列在文末「附录 A」。

## 一、背景与现状

`GroupingMethod::Minimization` 目前在两个入口均直接拒绝：

- `src-tauri/src/core/grouping/mod.rs:40` 的 `compute_grouping` 分派中 bail「暂未实现」；
- `src-tauri/src/core/grouping/randomizer.rs:270` 的 `validate_randomization` 同样 bail。

前端 `src/lib/grouping-method.ts:87` 将最小化法标为「规划中，尚未实现」并灰掉，
`docs/randomization_design.md` §2.5 已给出方法学定位：

| | 经典最小化法（Pocock–Simon） | 现有 `Optimized` |
| --- | --- | --- |
| 分配顺序 | 序贯，动物一只一只进入 | 一次性，全部动物同时定组 |
| 决策依据 | 不平衡度量（各协变量层的组间计数差） | 全部指标的组间检验 P 值 |
| 随机成分 | 有，以概率 p（常取 0.7–0.8）分到最优组 | 无，恒取排序第一 |
| 输入要求 | 协变量须先分档为分类变量 | 直接用连续值做检验 |

最小化法与现有 `Optimized` 目标相同（协变量组间均衡）、机制不同，不能把
`Optimized` 直接冠以最小化法之名。补上真正的序贯最小化后，确证性临床试验
场景的推荐方法才名副其实，也才谈得上用于申报。

## 二、决策记录

以下决策点中，#1 为本方案推荐主路径，其余为次级决策，均建议按推荐值执行；
「待确认」项在实施前由用户拍板。

| # | 决策点 | 结论 | 状态 |
| --- | --- | --- | --- |
| 1 | 实现形态 | 新增独立模块 `core/grouping/minimizer.rs`，经典 Pocock–Simon 序贯分配 | 推荐 |
| 2 | 协变量来源 | 独立于 `StatConfig.selected_indicators`，新增 `MinimizationConfig.covariates` | 推荐 |
| 3 | 分配概率 p | 默认 0.8，可配置，取值范围 (0, 1) 开区间；**1−p 的概率质量分给非最优组**，使导出的 p 就是「分到最优组」的实际概率 | v0.2 修订 |
| 4 | 连续协变量分档 | v1 仅支持三分位，且**在性别层内分档**、按值切点、并列同档；用户自定义切点留 v2 | v0.2 修订 |
| 5 | 入口顺序 | 规范化排序 + 种子洗牌 | 推荐 |
| 6 | 不平衡度量 | 各协变量各「(性别, 档位)」单元的**配额归一**实验组计数极差之和；sex 本身不参与度量 | v0.2 修订 |
| 7 | 备用组语义 | 实验组满员前不可选，实验组满员后作为溢出承接（等价于随机入组顺序的尾部，无偏） | v0.2 补充理由 |
| 8 | 复现契约 | 从「Excel 按随机数重排复现」变为「同种子重跑软件逐位复现」；导出改用「入组顺序 + 逐只决策日志」作为人工可核对的审计面 | v0.2 修订 |
| 9 | 确证性临床试验默认方法 | **不切换**，默认仍为 `Optimized`，最小化法在场景说明中标为推荐方法但需用户显式选择 | v0.2 修订 |
| 10 | 逐只决策日志 | **v1 就做**：落 `MinimizationRecord.decisions`，导出为「最小化过程」Sheet | v0.2 修订 |
| 11 | RNG 流消费 | 每只动物**无条件**消费 2 个 uniform，与决策分支无关 | v0.2 新增 |

三条不变量继承自 `randomization_design.md`，本设计不得破坏：

- 随机化路径不得看指标值择优；预先声明、机器自动执行的规则除外（最小化法的
  不平衡度量即此类规则，须随参数一并写入导出文件）；
- 全部路径可复现：种子、算法标识、输入指纹、引擎版本四件套（§8.2）；
- GLP 场景执行分配隐藏，不提供看到结果后重算的入口（§3.5）。

## 三、备选方案与权衡

| 方案 | 思路 | 优点 | 缺点 |
| --- | --- | --- | --- |
| A（推荐） | 独立协变量配置 + 经典序贯最小化 | 方法学口径干净，导出可写「协变量、分档、p」；与检验指标职责分离 | 前端多一组配置；实现面略大 |
| B | 复用 `selected_indicators` 当协变量，p 固定 0.8 不暴露 | 改动最小、UI 零新增 | 平衡变量与检验变量混用，导出描述含糊；p 不可配置违反文档约定，申报口径经不起追问 |
| C | 只做贪心（p=1）确定性版本 | 最简单、完全确定 | 没有随机成分，不再是最小化法；「确定性择优」会滑向被禁的优化类口径，不建议 |

方案 C 被否掉这件事在 v0.2 里有一个直接后果：**p = 1 不是合法配置**，校验按开区间
(0, 1) 拒绝。文档正文与测试用例中不得再出现「p = 1 时退化为纯贪心」这类描述——
需要验证贪心行为时，直接单测决策函数，不经过配置校验层。

## 四、核心设计

### 4.1 数据结构（`src-tauri/src/core/models.rs`）

```rust
/// RandomizationConfig 内新增嵌套字段，方法为 Minimization 时必须为 Some。
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomizationConfig {
    // ...existing fields (seed / primary_indicator / acceptance / max_attempts / draw_index)
    /// Minimization-specific parameters. Required when method == Minimization.
    #[serde(default)]
    pub minimization: Option<MinimizationConfig>,
}

/// Sequential covariate-adaptive minimization (Pocock-Simon) parameters.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationConfig {
    /// Indicator keys used as balancing covariates. Numeric, complete, deduplicated, >= 1.
    pub covariates: Vec<String>,
    /// Probability of allocating to a minimizer. In the open interval (0, 1); default 0.8.
    #[serde(default = "default_allocation_probability")]
    pub allocation_probability: f64,
    /// How continuous covariates are categorized. v1: tertiles only.
    #[serde(default)]
    pub binning: CovariateBinning,
}

/// How continuous covariates are categorized into levels.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CovariateBinning {
    /// Tertiles cut inside each sex stratum, on values (not ranks), ties kept together.
    #[default]
    Tertiles,
    // v2: CutPoints { cut_points: HashMap<String, Vec<f64>> },
}

fn default_allocation_probability() -> f64 {
    0.8
}
```

`RandomizationConfig::Default`（`models.rs:215`）要补 `minimization: None`，否则
`compute_random_grouping` 里的 `unwrap_or_default()` 拿到的结构与新字段不一致。
现有 `RandomizationConfig` 没有 `PartialEq`，本方案也不需要新增。

记录侧的结构要做到一件事：**光看导出文件，就能复述这次分组用的是什么规则、
每只动物为什么去了那一组**。

```rust
/// What a minimization run actually used. Written into the record so the exported method
/// description can name the real parameters, and so the run can be replayed on paper.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationRecord {
    pub covariates: Vec<String>,
    /// Binning scheme identifier, e.g. "tertiles-within-sex".
    pub binning: String,
    /// The cut points that binning actually produced, one entry per covariate.
    pub bins: Vec<CovariateBins>,
    pub allocation_probability: f64,
    /// Imbalance measure identifier, e.g. "quota-normalized-range". A later engine reading
    /// an archived record has to be able to tell which formula produced it.
    pub imbalance_measure: String,
    /// Allocation rule identifier, e.g. "minimizer-or-uniform-over-others".
    pub allocation_rule: String,
    /// Per-animal decision log, in entry order.
    pub decisions: Vec<MinimizationDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CovariateBins {
    pub covariate: String,
    /// One entry per sex stratum present in the dataset.
    pub strata: Vec<CovariateStratumBins>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CovariateStratumBins {
    pub sex: Sex,
    /// Boundaries between adjacent levels; `levels == cut_points.len() + 1`.
    pub cut_points: Vec<f64>,
    pub levels: usize,
}

/// One step of the sequential allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationDecision {
    /// 1-based position in the seeded entry order.
    pub entry_index: usize,
    pub animal_id: String,
    /// Level index per covariate, aligned with `MinimizationRecord.covariates`.
    pub levels: Vec<usize>,
    /// Imbalance score per group id; groups that were not eligible carry `None`.
    pub scores: Vec<Option<f64>>,
    /// True when the animal went to a minimizer, false when the 1 - p branch fired.
    pub took_minimizer: bool,
    pub group_id: usize,
}

// RandomizationRecord 内新增：
pub struct RandomizationRecord {
    // ...existing fields
    /// Present only for Minimization runs.
    #[serde(default)]
    pub minimization: Option<MinimizationRecord>,
}

// GroupAssignment 内新增：
pub struct GroupAssignment {
    // ...existing fields (animal_id / sex / group_id / random_number / block_index)
    /// 1-based position in the seeded entry order. Present only for Minimization, where
    /// `random_number` and `block_index` are both None: the allocation is not "sort by a
    /// number and deal", so exporting a per-animal draw would suggest a hand check that
    /// does not exist. Entry order is the field that *is* auditable.
    #[serde(default)]
    pub entry_index: Option<usize>,
}
```

选择嵌套 `Option` 的理由：`RandomizationConfig` 已天然挂在所有带种子方法的
`GroupConfig` 下，校验函数签名不变；serde default 落 `None`，历史配置与历史
记录反序列化行为不变。

决策日志的体积是 `n × (协变量数 + 组数)` 个标量，60 只 3 组 2 协变量约 300 个数，
上千只动物也只是几万个数，对 IPC 与导出都不构成压力。

### 4.2 协变量分档：必须在性别层内做

v0.1 写的是「按全数据（不分性别）三分位」。这条在双性别数据上会让整个方法失效：

雌雄在体重、脏器重量等指标上的分布通常几乎不重叠，全局三分位会把雄性整体压进
高档、雌性整体压进低档。而分配的可选组集合是按性别过滤的（见 §4.4），于是在任一
性别层内所有动物档位相同，每次分配增加的都是同一个计数单元，极差度量退化成
「平衡各组的动物数」——而这件事性别配额本来就已精确保证。**净效果是协变量提供
零区分度，最小化法在双性别数据上等价于完全随机。** e2e 夹具正是 6 雄 3 雌，这个
退化一定会被触发。

v0.2 的分档规则：

1. 分档在**每个性别层内**独立进行，计数单元是 `(性别, 档序)` 的二元组。单性别
   数据（本项目 §5.1 的 60 只全雌数据即是）与全局分档完全等价；
2. **按值切，不按秩切**。取层内经验 1/3、2/3 分位处相邻不同取值的中点作为切点，
   取值相同的动物必然落在同一档。按秩切会把两只体重完全相同的动物劈进不同档，
   对离散型或取值重复多的协变量（评分、少数几个刻度的指标）尤其荒谬；
3. 层内不同取值数少于 3 时，档数退化为不同取值数；协变量在该层内取值全同则只有
   1 档，对不平衡度量的贡献恒为常数，算法仍正确，只是该变量在该层不提供区分度；
4. 实际切点与档数写入 `CovariateBins`，导出时可直接复述。

### 4.3 不平衡度量：按配额归一

v0.1 用的是原始计数极差。本软件通过 `GroupSize::Custom`（`models.rs:246`）明确支持
不等组容量，原始计数在这种配置下会系统性错配：设 A 组配额 20、B 组配额 10，原始
计数极差会把两组一路拉平到各 10 只，B 满员后剩余 10 只全部灌进 A——A 拿到的是入组
顺序的后半段，均衡度反而比完全随机更差。

v0.2 的度量：对进入的动物，令 `s` 为其性别，`cell_c` 为其在协变量 `c` 上所属的
`(s, 档序)` 单元，则把它假设性地分给候选组 `g` 之后的不平衡度为

```
score(g) = sum over covariates c of
             range over experimental g' with quota[g'][s] > 0 of
               ( counts[c][cell_c][g'] + (1 if g' == g else 0) ) / quota[g'][s]
```

三点说明：

- **只对该动物自己所在的单元求和**。其余单元的计数不受本次分配影响，其极差对
  所有候选 `g` 是同一个常数，不改变 argmin。v0.1 写的「对所有协变量所有档求和」
  与此**数学等价**，只是每只动物的复杂度从 O(c·k) 变成 O(c·L·k)。实现按本节的
  形式写，并在代码注释里写明这层等价性——否则后来者照文献改成本节形式时，会
  以为改出了行为差异而反复求证；
- **归一分母是该组在该性别上的配额**，不是总配额：计数单元本身是性别特定的，
  期望计数正比于 `quota[g][s]`。某组在该性别上配额为 0 时，其计数必然也是 0，
  直接把它排除出极差，避免 0/0；
- 全部实验组等配额时，归一只是给每个单元乘上同一个常数因子，排序与原始计数极差
  完全一致——这条要写成回归测试，保证 v0.2 没有改变等配额场景的行为。

备用组不进入度量（理由见 §4.5），性别本身也不进入：性别平衡由配额精确保证，
加进去只会稀释协变量信号。

### 4.4 序贯算法（新模块 `minimizer.rs`）

```
entry_order = seeded_shuffle(normalized_order(animals), rng)
levels[c][animal] = level of `animal` in covariate `c`, binned within its sex stratum

counts[c][cell][group] = 0
remaining[group][sex]  = quota from sex_constraints

for entry_index, animal in entry_order:
    s = sex(animal)
    eligible = [g for g in groups if remaining[g][s] > 0]
    if any g in eligible is experimental:
        eligible = experimental groups in eligible   // reserve only takes the overflow

    score(g) for g in eligible                       // see 4.3
    minimizers = argmin over eligible of score
    others     = eligible \ minimizers

    u1 = rng.gen::<f64>()                            // always consumed, see 4.6
    u2 = rng.gen::<f64>()                            // always consumed, see 4.6
    pool = if u1 < p or others.is_empty() { minimizers } else { others }
    g    = pool[(u2 * pool.len() as f64) as usize]

    assign(animal, g)
    counts[c][cell_c(animal)][g] += 1 for each covariate c
    remaining[g][s] -= 1
    log decision(entry_index, animal, levels, scores, took_minimizer = pool is minimizers, g)
```

要点：

- **入口顺序**是新的随机自由度。用「规范化排序 + 种子洗牌」而非 Excel 行序，
  否则「同一份数据换个行序、同种子」结果会漂移，违反 §8.2 契约；洗牌本身由
  种子锁定，不破坏可复现性。`normalized_order` 直接复用
  `randomizer.rs:312` 的实现，不另起一套排序规则；
- **分配规则**：以概率 p 落在最优组集合内均匀抽取，以 1−p 落在**非最优组**内均匀
  抽取。这是经典写法，其直接好处是导出文案里的 p 就是「分到最优组」的真实概率。
  v0.1 的写法（1−p 分支在全部可分配组内均匀抽取）会让实际概率变成 p + (1−p)/k，
  p = 0.8、3 组时是 0.867，与导出声明的 0.8 对不上——第十章把申报口径列为最大
  风险，这里正是最容易被一句话问倒的地方；
- **全平局时**（例如第一只动物，所有计数为 0）`others` 为空，退回在 `minimizers`
  内均匀抽取。这同时保证了 §8.3 要求的「组标号本身必须随机」：哪一组是对照组由
  随机数决定，不偏向编号小的组；
- **统计评估**：分配完成后走现有 `evaluate_grouping_with_constraints`
  （`Untestable::Skip`）；`total_evaluated = 1`、`total_valid = meets_criteria`，
  与受限随机化路径一致。最小化法不带接受准则，也不重抽——失衡时按 §七 的文案
  提示用户调整协变量或指标，不得提示「换个种子再试」；
- **性能**：单趟序贯，复杂度 O(n × g × c)，对数千只动物可忽略，无需枚举。

### 4.5 配额与备用组

实验组满员前备用组不可选；实验组满员后只剩备用组，强制承接。性别配额逐组精确
满足：任一时刻某性别的剩余槽位总数恒等于该性别未分配动物数，因此不会走进死胡同。
配额本身的合法性（逐性别精确匹配、实验组至少 2 只）复用
`enumerator::validate_config`，与随机化路径调用顺序一致。

这条规则的实际语义要写明白，不能一句「无需第二趟分配」带过：由于入组顺序是均匀
洗牌，「承接尾部」等价于**备用动物是一个均匀随机子集**——无偏，但它在协变量上的
代表性只是期望意义上的，方差大于 `BlockedRandom`（那里备用组在每个区组占固定配额，
§8.5 据此承诺「备用动物不会全是最轻或最重的」）。

要恢复固定配额那种强代表性，必须让备用组**全程参与可选集合**，而不是只把它加进
不平衡度量——度量改了也没用，因为备用组在满员前根本不在 `eligible` 里。而让它
全程参与的代价是：把有限的均衡能力分给一个不进入任何统计的组。权衡下来 v1 保持
「溢出承接」，把「备用组作为普通组全程参与」记为 v2 的可选项。

### 4.6 随机源与可复现契约

复用 §8.2 的四件套：`ChaCha12Rng`（`RNG_ALGORITHM = "chacha12"`）、规范化排序、
输入指纹、引擎版本。种子经 `derive_draw_seed(base_seed, draw_index)` 派生，
GLP 下 `draw_index > 1` 仍被拒绝（分配隐藏）。

**流消费必须无条件**：可复现性依赖 RNG 流的位置，因此每只动物固定消费 2 个
uniform（p 硬币 + 池内均匀抽取），即使 `eligible` 只剩一个组、结果被完全强制，
也照抽不误。这条要写进模块文档并配一条断言消耗次数的测试——否则未来任何「只剩
一个组就跳过抽签」的优化都会悄悄改变所有后续动物的分配，而它看起来完全无害。

**复现契约的变化**：其它随机化方法承诺「按区组 + 随机数重排即得分配」，最小化法
做不到——分配由序贯决策链决定，Excel 里只有随机数无法反推。因此：

- `GroupAssignment.random_number` 与 `block_index` 对最小化法**恒为 `None`**，改写
  `entry_index`。这不只是模型层的事：`exporter.rs:129` 的 `AuditColumns::of()` 按
  `random_number.is_some()` 决定是否输出「随机数」列，其文档注释还写死了「sort the
  sheet by 区组 then 随机数 and deal each group its quota in turn」，必须同步改；
- `models.rs:349` 上 `GroupAssignment.random_number` 的注释、以及 `CLAUDE.md` 里
  「区组 和 随机数 是审计列……reviewer 可以重排复现」那一段，都要限定为「仅随机化
  三兄弟」；
- 前端 `ResultsPage.tsx:395` 的「复现步骤」卡片本来就写的是「导入同一份数据 → 填
  种子 → 重新计算」，与最小化法并不冲突，无需改文案。真正要改的是它上方按方法
  分支展示的参数块（§七）；
- **人工可核对的审计面由决策日志承担**：导出「最小化过程」Sheet 后，审阅者可以逐
  行核对档位与不平衡分数，复算出每一步的最优组集合，并检查最终组别是否落在「最优
  组」或「非最优组」中——与日志记录的分支一致。随机硬币本身无法手工复现，但规则
  是否被如实执行可以被完整核对。

## 五、校验规则（`validate_randomization` 扩展）

| 检查项 | 规则 |
| --- | --- |
| 配置存在性 | `Minimization` 必须携带 `randomization: Some`，且其 `minimization` 为 Some |
| 配额合法性 | 复用 `enumerator::validate_config`（逐性别精确匹配、实验组 ≥ 2 只），与随机化路径同序调用 |
| 参数冲突 | `primary_indicator` / `acceptance` 必须为 None，冲突即报错（不静默忽略） |
| 协变量 | 非空、去重、均在指标列中、数值型、所有动物无缺失（缺失时点名动物，复用主指标报错风格） |
| 协变量区分度 | 若所有协变量在所有性别层内都只有 1 档，直接报错：最小化法在此配置下退化为按配额随机，用户选错了协变量，不能让它静默跑完 |
| p 范围 | `allocation_probability ∈ (0, 1)` 开区间；p = 1 亦拒绝（方案 C 已否，见第三章） |
| GLP 分配隐藏 | 复用现有 `draw_index > 1` 拒绝规则 |
| 方法谓词 | 新增 `uses_random_source()`（覆盖四种带种子方法），用于 `mod.rs:65`「必须提供随机化参数」校验与前端表单门控；`is_randomized()` 语义保留给纯随机化三兄弟，导出措辞因此不被污染 |

协变量个数不设硬上限，但前端应提示：协变量越多，每个协变量分到的均衡能力越少
（度量是各协变量的等权和）。这属于文案，不属于校验。

## 六、记录与导出

- `RandomizationRecord`：seed / base_seed / rng_algorithm / input_fingerprint /
  engine_version 全套照旧；`attempts = 1`；新增 `minimization` 块。
- **「分组原理」**（`exporter.rs:44` 的 `grouping_principle`）由静态字符串改为按
  `MinimizationRecord` 动态拼接：

  > 最小化法（协变量自适应随机化，分配概率 p = 0.80；协变量：体重、CD45 比例，
  > 按性别层内三分位分档）

- **「分层变量」**（`exporter.rs:87` 的 `stratification_variable`）必须一并改。
  它目前对最小化法会 fall through 到 `_ if sex_stratified => "性别"`，于是同一张
  汇总表上「分组原理」写协变量、「分层变量」写性别，自相矛盾。应输出：

  > 协变量：体重、CD45 比例（性别层内三分位）

- **「汇总信息」新增行**：分配概率 p、不平衡度量标识、分配规则标识。种子、算法、
  指纹、引擎版本沿用现有行。
- **「分组结果」Sheet 的审计列**：最小化法输出「入组顺序」，不输出「随机数」与
  「区组」。列序为 `组别 | 动物编号 | 性别 | [入组顺序] [区组] [随机数] | 指标…`，
  `AuditColumns` 相应增加一个 `entry` 标志位（由 `entry_index.is_some()` 判定），
  `count()` 与 `first_indicator_col()` 随之调整。
- **新增「最小化过程」Sheet**，仅最小化法输出。表头区先写分档切点（每个协变量
  每个性别层一行），下方逐只动物一行：

  | 入组顺序 | 动物编号 | 性别 | 各协变量档位 | 各组不平衡分数 | 决策分支 | 组别 |

  「决策分支」写「最优组」或「非最优组（1−p）」。这张表是最小化法唯一的人工可核对
  产物，也是把「预先声明的规则被如实执行」这件事落到纸面的地方。

## 七、前端改动

| 文件 | 改动 |
| --- | --- |
| `src/lib/grouping-method.ts` | `Minimization` 标为 implemented；机制文案更新；新增 `usesRandomSource()` 共享谓词，替换 `ConfigurePage.tsx:154`、`ComputePage.tsx:42`、`ResultsPage.tsx:119` 三处零散的 `method !== "Optimized"` 判断 |
| `src/components/features/ConfigurePage.tsx` | method 为 Minimization 时展示「协变量多选（仅列全动物有值的数值指标）+ 分配概率 p（默认 0.8，校验开区间）+ 种子」，隐藏主指标与接受准则；`randomization` 拼上 `minimization` 块；提示协变量过多会稀释信号 |
| `src/components/features/ComputePage.tsx` | 展示协变量与 p；不显示「Top-N 最优」文案（现有 `isRandomized` 分支已覆盖，改用共享谓词后自然生效） |
| `src/components/features/ResultsPage.tsx` | 参数块按方法分支：最小化法展示协变量、分档口径、p、入组顺序说明，不展示区组结构；**把 `ResultsPage.tsx:407` 那条「分层变量 P 值必然接近 1」的告警扩展到协变量**——协变量（尤其当它同时出现在 `selected_indicators` 中时）有完全相同的虚高误读风险，§9.1 已就分层变量强调过一次；失衡告警的处置建议改为「调整协变量或参与统计的指标」，不得出现「重新随机」 |
| `src/types/index.ts`、`frontend-types.ts` | 类型同步（含 `MinimizationConfig` / `MinimizationRecord` / `CovariateBins` / `MinimizationDecision` / `GroupAssignment.entry_index`） |

## 八、测试与验证

新增/修改测试（`cd src-tauri && cargo test`）：

- **可复现性**：同种子两次运行逐位一致（含入口顺序与每次抽签）；不同种子不同
  分配；打乱 Excel 行序同种子结果不变；**断言 RNG 消耗次数**为「洗牌 + 2n」，
  构造一个末段只剩单一可选组的配置，验证消耗次数不随分支变化（§4.6 契约）。
- **分档**：按值切、并列同档（构造含重复值的协变量，断言等值动物同档）；层内
  不同取值数 < 3 时档数退化；双性别数据下档位键为 `(性别, 档序)`，且雌雄分布
  完全不重叠时协变量仍具区分度（这是 v0.1 会失败的用例）；切点如实写入记录。
- **度量**：等配额时与原始计数极差给出相同的 argmin（保证 v0.2 未改变主流场景
  行为）；不等配额（如 20/10）时，配额归一的 argmin 与原始计数极差不同，且最终
  两组在各档位上的**比例**接近而非绝对计数接近。
- **决策规则**：存在唯一严格最小组且 `others` 非空时，p 分支恒选它、1−p 分支恒不
  选它；全平局时在最优组内均匀；`others` 为空时退回最优组集合。这一组直接单测
  决策函数，不经过配置校验层（因此可以传 p = 0 / p = 1 作为极端输入）。
- **配额与备用组**：3×18 + 6 备用组、双性别场景下各组数量与性别配比精确满足，
  备用组最后承接溢出；备用动物在协变量上的期望分布与总体一致（跨种子统计，
  阈值宽松防 flaky）。
- **均衡性**：60 只 × 3 组，跨 200 个固定种子比较最小化法（p = 0.8）与完全随机的
  组均值极差分布，最小化法显著更小（阈值宽松防 flaky）。
- **决策日志的自洽性**：日志每一步的 `group_id` 与最终 `assignments` 一致；把日志
  中的 `scores` 与 `took_minimizer` 重放一遍，能复算出同一份分配。这条测试证明
  导出的「最小化过程」Sheet 是一份忠实的审计记录，而不是事后凑出来的说明。
- **校验**：协变量为空 / 重复 / 不存在 / 有缺失（点名）/ 全无区分度、p 越界、
  p = 1、primary_indicator 或 acceptance 与 Minimization 冲突、GLP 下
  draw_index > 1、缺 `minimization` 块，全部被拒且报错可读。
- **导出**：最小化法的「分组结果」只出「入组顺序」审计列；「分层变量」与「分组
  原理」两行口径一致；「最小化过程」Sheet 行数等于动物数；两组设计下事后比较表
  仍然不生成（沿用现有规则）。
- **既有回归**：`randomization_record_survives_the_ipc_f64_round_trip` 等构造
  `RandomizationRecord` 的测试补新字段；`minimization_is_refused_rather_than_silently_substituted`
  （`grouping/tests.rs:682`）改为断言可运行；e2e golden 夹具不受影响（`Optimized`
  不走该路径，新增字段为纯增量）。
- 前端：`tsc` 类型检查；`bun run tauri dev` 手工走一遍确证性临床试验场景。

## 九、影响范围与改动清单

按实施顺序：

| 阶段 | 文件 | 改动 |
| --- | --- | --- |
| 1 | `src-tauri/src/core/models.rs` | 新增 `MinimizationConfig` / `CovariateBinning` / `MinimizationRecord` / `CovariateBins` / `CovariateStratumBins` / `MinimizationDecision`；扩展 `RandomizationConfig`（含 `Default` impl）、`RandomizationRecord`、`GroupAssignment.entry_index`；`GroupingMethod::uses_random_source()` |
| 1 | `src-tauri/src/core/grouping/minimizer.rs`（新增） | 分档、不平衡度量、序贯决策、配额分配、决策日志与记录构造 |
| 2 | `src-tauri/src/core/grouping/mod.rs` | 分派 `Minimization`；`validate_method_selection` 改用 `uses_random_source()` |
| 2 | `src-tauri/src/core/grouping/randomizer.rs` | `validate_randomization` 校验扩展（§五）；`normalized_order` / `derive_draw_seed` 供 `minimizer.rs` 复用 |
| 3 | `src-tauri/src/core/exporter.rs` | `grouping_principle` 动态拼接；`stratification_variable` 补最小化法分支；`AuditColumns` 增 `entry` 列；汇总信息新增行；新增「最小化过程」Sheet |
| 3 | `CLAUDE.md` | 「区组 / 随机数是审计列，可重排复现」一段限定为随机化三兄弟，并补最小化法的「入组顺序 + 决策日志」口径 |
| 4 | `src/types/index.ts`、`frontend-types.ts` | 类型同步 |
| 5 | `src/lib/grouping-method.ts`、`ConfigurePage.tsx`、`ComputePage.tsx`、`ResultsPage.tsx` | 共享谓词、表单、展示与文案（含协变量 P 值虚高告警） |
| 6 | 测试 | 按第八章清单补齐 |

## 十、风险与待确认项

- **申报口径是最大风险**：实现后最小化法在 GLP 下变为可选（§2.6 矩阵本就允许，
  但要求预先声明）。导出描述与复现契约必须同步改，不能出现「导出写最小化法、
  审计列却是随机数」的自相矛盾。v0.2 把 p 的语义、分档切点、不平衡度量标识、
  决策日志全部落进记录，就是为了让「预先声明的规则被如实执行」这句话有据可查。
- **「随机化」标签**：最小化法有随机成分但非纯随机化。`is_randomized()` 保留给三
  兄弟、新增 `uses_random_source()` 承担「是否需要种子」的判断，前端文案相应区分
  「随机化分组」与「带随机成分的自适应分配」。
- **决策日志的隐私与体积**：日志逐只列出动物编号与档位，属于已在「分组结果」中
  暴露的同类信息，不新增暴露面；体积见 §4.1。
- **已确认不切换：确证性临床试验默认方法**（决策 #9）。文档 §2.6 写该场景默认最小
  化法，但最小化法的协变量没有合理的默认值：切换后用户一进页面就是「不可提交」
  状态，而 §2.6 的配套约定又要求每次切场景都重置方法，会导致每次切换都撞一次表单
  报错。因此 v1 保持默认 `Optimized`，在场景说明里把最小化法标为推荐方法、由用户
  显式选择（`grouping-method.ts` 的 `defaultMethodFor` 与该场景的 `restriction` 文案）。
  等 UI 有了合理的协变量预选策略（例如沿用上次选择）再回来重议。

### 实现落点

| 位置 | 内容 |
| --- | --- |
| `core/models.rs` | `MinimizationConfig` / `CovariateBinning` / `MinimizationRecord` / `CovariateBins` / `MinimizationDecision`；`GroupAssignment.entry_index`；`GroupingMethod::uses_random_source` |
| `core/grouping/minimizer.rs` | 分档（`BinnedCovariate::build`、`tertile_cuts`）、度量（`imbalance_score`）、决策分支（`choose_group`）、序贯分配（`allocate`）、记录构造 |
| `core/grouping/randomizer.rs` | `validate_randomization` 的 `Minimization` 分支与跨方法互斥校验 |
| `core/exporter.rs` | 动态「分组原理」「分层变量」、`AuditColumns.entry`、`write_minimization_sheet_to`、汇总信息新增行 |
| 前端 | `grouping-method.ts` 的 `usesRandomSource` / `DEFAULT_ALLOCATION_PROBABILITY`；三个页面的表单、展示与协变量 P 值虚高告警 |
| 测试 | `minimizer/tests.rs` 33 项；`exporter_test.rs` 的导出断言；`grouping/tests.rs` 的分派用例 |

---

## 附录 A：v0.1 → v0.2 变更

| # | v0.1 | v0.2 | 原因 |
| --- | --- | --- | --- |
| 1 | 全局三分位分档 | 性别层内分档，计数单元为 `(性别, 档序)` | 全局分档在双性别数据上让协变量与性别共线，最小化退化为完全随机（§4.2） |
| 2 | 原始计数极差 | 按该性别配额归一后取极差 | 不等组容量下原始计数会系统性错配，均衡度差于完全随机（§4.3） |
| 3 | 1−p 在全部可分配组内均匀抽取 | 1−p 在非最优组内均匀抽取 | 前者的实际最优组概率是 p + (1−p)/k，与导出声明的 p 对不上（§4.4） |
| 4 | 「p = 1 时退化为纯贪心」与 (0,1) 开区间校验并存 | 统一为开区间，贪心行为只在决策函数单测中出现 | 三处描述互相矛盾（第三章、§八） |
| 5 | 「复现卡片文案要改」 | 定位到 `exporter.rs` 的 `AuditColumns`、`models.rs` 注释、`CLAUDE.md`；前端复现卡片本就无需改 | v0.1 指错了层，照做会漏掉真正需要改的地方（§4.6） |
| 6 | 未提 `stratification_variable` | 明确要改 | 否则汇总表上「分组原理」与「分层变量」自相矛盾（§六） |
| 7 | 未定义 `random_number` / `block_index` | 恒为 None，新增 `entry_index` 与「入组顺序」列 | 输出随机数会暗示一个不存在的手工核对路径（§4.6、§六） |
| 8 | 记录只有 `levels_per_covariate` | 增加分档切点、分档口径、度量标识、分配规则标识 | 光看记录无法复述「档位是怎么切的、用的哪个公式」（§4.1） |
| 9 | 未提配额校验与 `Default` impl | 补 `enumerator::validate_config` 复用、`RandomizationConfig::Default` 补字段 | 实施遗漏项（§五、§4.1） |
| 10 | 未提协变量 P 值虚高告警 | 扩展 `ResultsPage` 的分层变量告警到协变量 | 与分层变量同类的误读风险（§七） |
| 11 | 无 RNG 流消费约定 | 每只动物无条件消费 2 个 uniform | 条件性抽签会让未来的「无害优化」悄悄破坏可复现性（§4.6） |
| 12 | 备用组「无需第二趟分配」 | 补明它等价于均匀随机子集，代表性弱于区组随机化，并说明为何「纳入度量」并不能修复 | 取舍要显式，不能被一句实现便利带过（§4.5） |
| 13 | 决策 #9 待确认（倾向切换默认） | 明确不切换 | 切换后默认配置不可提交，每次切场景都撞表单报错（第十章） |
| 14 | 决策 #10 决策日志留 v2 | v1 就做 | 放弃「按随机数重排」后，日志是唯一人工可核对的产物，成本只有 n 行（§4.1、§六） |
