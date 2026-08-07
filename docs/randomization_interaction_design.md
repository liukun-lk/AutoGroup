# 随机化交互设计：均衡强度档、候选分组与复现展示

> 在 `docs/randomization_design.md`（v0.6）已定框架之上，回答三个交互层问题：
> 无主指标时多指标均衡的强度如何声明；结果页如何回退与切换候选；
> GLP 的复现要求如何在界面上呈现。
>
> 文档版本: v0.1
> 创建日期: 2026-08-07
> 状态: 已实施
> 前置依赖: randomization_design.md 第七、八章（`Random` / `ConstrainedRandom` /
> `BlockedRandom` 与种子契约已实现，见 `src-tauri/src/core/grouping/randomizer.rs`）

## 一、决策记录

四个决策均已与用户确认（2026-08-07）：

| # | 决策点 | 结论 |
| --- | --- | --- |
| 1 | 无主指标的多指标均衡强度 | 基础档（现有 P > α 接受准则）保底，新增可调增强档（目标接受率定标 min(P) 门槛）；不做 Mahalanobis 再随机化 |
| 2 | GLP 场景下"再抽一签"与候选切换 | 锁死，灰掉并说明原因；不提供"允许但留痕"的折中 |
| 3 | 随机化候选的生成方式 | 按需追加抽签，种子由主种子确定性派生，不预生成 |
| 4 | 结果页展示复现信息 | 做成"复现信息卡"，定位为说明书；权威记录仍是导出文件与历史库 |

三条不变量继承自 randomization_design.md，本设计不得破坏：

- 随机化路径不得看指标值择优（§1.3）；预声明、机器自动执行的接受准则除外；
- 全部路径可复现：种子、算法标识、输入指纹、引擎版本四件套（§8.2）；
- GLP 场景执行分配隐藏，不提供看到结果后重算的入口（§3.5）。

## 二、均衡强度档（决策 1）

### 2.1 模型

接受准则从布尔开关升级为枚举。两档共用同一台拒绝采样机器，只是验收规则不同：

```rust
/// Acceptance rule applied to each draw. Both variants are declared before any
/// draw is made and executed by the machine, so both stay inside the
/// "restricted randomization" boundary; neither inspects candidates to pick a
/// winner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
pub enum AcceptanceCriterion {
    /// Tier 1 (existing behavior): every tested indicator must clear alpha.
    /// Rejects only draws with a detectable difference (~10% of them).
    AlphaLine,
    /// Tier 2: accept only draws in the most-balanced `target_rate` fraction,
    /// ranked by min(P) over all tested indicators. The min(P) cutoff is
    /// calibrated on this dataset by a seeded simulation, because the scale of
    /// min(P) collapses as the indicator count grows (median ~0.30 at 2
    /// indicators, ~0.01 at 70) — a fixed threshold cannot work.
    TopFraction { target_rate: f64 }, // e.g. 0.10
}

pub struct RandomizationConfig {
    pub seed: Option<u64>,
    pub primary_indicator: Option<String>,
    /// None = no acceptance criterion (pure `Random`).
    /// Replaces the old `enforce_criteria: bool` (feature is unreleased, no
    /// migration needed).
    pub acceptance: Option<AcceptanceCriterion>,
    pub max_attempts: usize,
    /// 1-based draw number within this run. See §3.2 for seed derivation.
    pub draw_index: usize,
}
```

`BlockedRandom` 与两档正交：区组保证主指标，准则兜住其余指标（randomization_design.md
§7.7 已实测两个开关互不干扰）。`ConstrainedRandom + TopFraction` 是"不指定主指标、
全部指标都重要"这一需求的落点。

### 2.2 增强档的定标流程

固定门槛在指标数变化时失效，因此门槛由数据定标，定标本身种子化：

```text
calibrate(dataset, indicators, alpha, target_rate, effective_seed):
    calib_rng  = ChaCha12Rng(splitmix64(effective_seed, CALIBRATION_TAG))
    draws      = 1000 sex-stratified indicator-blind allocations from calib_rng
    min_ps     = [min over indicators of main-test P, for each draw]
    p0         = empirical (1 - target_rate) quantile of min_ps
    return p0                     # a draw is accepted iff its min(P) >= p0
```

- 定标复用 `evaluator.rs` 现有的 P 值管线（`PostHocDetail::ValidityOnly` 路径），
  不引入新统计量；
- `p0`、定标抽样数（1000）、定标种子、目标接受率全部写入 `RandomizationRecord`
  并随导出输出——定标过程是预声明规则的一部分；
- `max_attempts` 默认按档位缩放：`AlphaLine` 维持现值；`TopFraction` 取
  `ceil(50 / target_rate)`（10% → 500 次上限，期望 ~10 次即中，毫秒级）；
- 正式抽签的接受判定：`AlphaLine` 沿用 `meets_criteria`；`TopFraction` 判
  `min_p >= p0`。达到 `max_attempts` 仍未中时沿用现有 `acceptance_failure`
  语义：显式报错，绝不静默降级。

### 2.3 界面文案（用户要求"选择处必须有很明确的说明"）

档位选择器放在 `ConfigurePage` 的随机化参数区，两档各配一段说明，直接使用以下文案：

**基础档（默认）——排除可检出差异的分组**

> 每一签都检验全部所选指标，任何一个指标 P ≤ α 就废签重抽。
> 只排除统计上能检出差异的约一成分组，其余一律等概率接受。
> 均衡程度与普通随机接近，随机性保留最足。适合"不出最差情况即可"的研究。

**增强档——只接受最均衡的前 X%**

> 软件先在本数据上做 1000 次种子化模拟，定出"最均衡的前 X%"对应的门槛
> （按全部所选指标中最差的那个 P 值），再正式抽签，达不到门槛就废签重抽。
> 全部指标一视同仁，没有主次之分。X 越小分得越匀、自动重抽越多（通常仍在毫秒级）。
> 门槛与定标过程会写入导出文件，作为预先声明的接受准则。

两档下方共用一行注脚：

> 两档都是抽签之前定死、由软件自动执行的规则，属于受限随机化；不构成看结果择优。

`target_rate` 提供 10% / 25% / 50% 三个预设加自定义输入，默认 10%。

## 三、候选分组模型（决策 3）

### 3.1 统一候选概念

候选是一个有序序列，两类方法的定义方式不同，但结果页用同一个切换器承载：

| 方法 | 候选是什么 | 生成时机 | 数量 |
| --- | --- | --- | --- |
| `Optimized` | Top-N 排名（后端已返回 `MultiGroupingResult.candidates`） | 一次算完 | 固定 N（默认 10） |
| 随机化三种 | 第 k 次抽签 | 按需追加（"再抽一签"） | 无上限，每签可由 (base_seed, k) 复现 |

关键转念："同种子重跑得到同结果"是可复现性契约的正确行为，不是缺陷。
"想要一个不同的结果"必须表达为显式的、有编号的新抽签，而不是重跑。

### 3.2 种子派生

```text
draw 1:      effective_seed = base_seed
draw k >= 2: effective_seed = splitmix64_mix(base_seed, k)
```

draw 1 特意不走派生：GLP 场景一签定终身，方案里预先写定的种子必须在不知道
派生函数的前提下直接重放出最终分配——QA 拿种子重放是硬要求（§3.5）。
k ≥ 2 只出现在非 GLP 场景，记录里同时落 `base_seed`、`draw_index` 与
`effective_seed`，重放取 `effective_seed` 即可，契约不变。

`RandomizationRecord` 增加字段：

```rust
pub struct RandomizationRecord {
    /// The seed that reproduces this allocation (draw 1: the base seed itself).
    pub seed: u64,
    pub rng_algorithm: String,
    pub input_fingerprint: String,
    pub engine_version: String,
    pub attempts: usize,
    // -- new fields --
    pub base_seed: u64,
    pub draw_index: usize,
    pub acceptance: Option<AcceptanceCriterion>,
    /// Present only for `TopFraction`: the calibrated min(P) cutoff and how it
    /// was obtained.
    pub calibrated_threshold: Option<f64>,
    pub calibration_draws: Option<usize>,
    // existing blocked-randomization fields unchanged
    pub primary_indicator: Option<String>,
    pub block_size: Option<usize>,
    pub incomplete_last_block: bool,
}
```

### 3.3 前端状态与交互

`resultAtom: GroupingResult | null` 升级为运行态：

```ts
interface GroupingRun {
  candidates: GroupingResult[];   // Optimized: Top-N; randomized: draws so far
  selectedIndex: number;          // which candidate the user is looking at
  totalEvaluated: number;         // Optimized only
  totalValid: number;             // Optimized only
}
```

- 结果页顶部加候选切换器：`Optimized` 显示"排名 #k · min(P)/mean(P)"；
  随机化显示"第 k 签 · 种子 xxxx"，尾部一个"再抽一签"按钮，调用后端
  `draw_index = len + 1` 追加；
- 导出使用 `candidates[selectedIndex]`，导出前的确认弹层显示当前选中的是哪个候选；
- 抽过的签全部保留在 `candidates` 里，切换不重算。

### 3.4 回退导航与运行边界

- `ResultsPage` 增加"返回修改配置"：仅置 `currentStepAtom = "configure"`，
  dataset / groupConfig / statConfig / selectedIndicators 全部保留；
- 回退后 configure 页检测到已有运行结果时显示提示条："当前已有计算结果，
  修改配置并重新计算后将开始新的一次运行"；
- 边界语义：同一运行内切换候选 / 再抽一签 = 不改配置、共享 base_seed；
  回退改配置（或改种子）后重算 = 新运行，产生新的 `GroupingRun`，旧运行被替换
  （历史落库后可查，见 §4.2）。

## 四、GLP 门控（决策 2）

### 4.1 锁死规则

`scenario === "GlpSubmission"` 时：

- "再抽一签"与候选切换器禁用，灰掉并显示原因（与 `Optimized` 被禁用同一套交互语言）：

  > GLP 场景执行分配隐藏：一次抽签即为最终分配，不提供看到结果后重抽或挑选的入口。
  > 需要更高的均衡度，请在计算前调整接受准则的目标接受率。

- 随机化运行强制 `draw_index = 1`，后端同样校验（防御前端绕过）：GLP 场景收到
  `draw_index > 1` 直接报配置错误；
- `Optimized` 在该场景本就被禁用（randomization_design.md §2.6），不变。

### 4.2 已知边界：seed-shopping

锁死再抽后仍存在一个后门：回退到第 2 步换一个种子重算，效果等同于再抽。
软件无法绝对阻止（重启应用也能重来），诚实机制是 randomization_design.md §3.5
已列出的历史落库——同一输入指纹上的历次运行（时间、种子、方法、参数）可查。
`history_repo.rs` 落地前，本设计不新增额外防线，仅在此记录该边界；
GLP 的最终防线本来也在流程侧：种子由试验方案预先规定，不由操作员现场挑选。

## 五、复现信息卡（决策 4）

### 5.1 定位

结果页对随机化方法展示一张"复现信息卡"。定位是**给 QA 的说明书，不是记录本身**：
权威记录始终是导出文件的汇总信息 Sheet 与历史库。卡片是把已经落在
`RandomizationRecord` 里的字段展示出来，零新增数据。

展示种子不构成数据窥探：重放是确定性的，看到种子无法帮助挑结果；
锁死再抽（§4.1）之后，种子在 GLP 场景下也没有可滥用的入口。

### 5.2 内容

| 区域 | 字段 |
| --- | --- |
| 本次分组 | 场景、方法（含档位与参数：目标接受率、定标门槛 p0、区组大小、主指标） |
| 随机源 | 种子（k ≥ 2 时同时显示主种子与抽签序号）、算法标识（chacha12）、实际抽样次数 |
| 输入锚定 | 输入指纹、引擎版本 |
| 复现步骤 | 三步文案（见下） |

复现步骤文案：

> 1. 导入同一份数据文件（软件校验输入指纹一致）；
> 2. 选择相同的场景、方法与参数，在种子栏填入上方记录的种子；
> 3. 重新计算，得到的分配与本次逐动物一致。

卡片底部注明："以上信息已随导出文件写入《汇总信息》表，归档请以导出文件为准。"

### 5.3 后续增强（本版不做）

"一键复现校验"按钮：用记录的种子静默重算一遍并比对分配指纹，把文字说明书变成
可执行验证。对 QA 的说服力高于任何文案，但依赖历史落库先就位，列入 v2。

## 六、影响范围与改动清单

| 层 | 文件 | 改动 |
| --- | --- | --- |
| 后端模型 | `core/models.rs` | `AcceptanceCriterion` 枚举；`RandomizationConfig.acceptance` / `draw_index`；`RandomizationRecord` 新字段 |
| 后端引擎 | `core/grouping/randomizer.rs` | 种子派生（splitmix64）；定标流程；`TopFraction` 接受判定；GLP 的 `draw_index` 校验 |
| 后端命令 | `commands/grouping.rs` | 参数透传，无逻辑 |
| 导出 | `core/exporter.rs` | 汇总信息 Sheet 增加档位、目标接受率、定标门槛、抽签序号行 |
| 前端类型 | `src/types/index.ts` | 与后端模型镜像 |
| 前端状态 | `src/stores/index.ts` | `resultAtom` → `GroupingRun`（candidates + selectedIndex） |
| 前端配置页 | `ConfigurePage.tsx` | 档位选择器 + §2.3 文案；回退提示条 |
| 前端结果页 | `ResultsPage.tsx` | 候选切换器；"再抽一签"；"返回修改配置"；复现信息卡；GLP 灰化 |

现有 `Optimized` 路径的枚举、评分、排序热路径不动；e2e 黄金测试（Optimized 导出）
预期不受影响——汇总信息新增行仅出现在随机化导出中。

## 七、测试与验证

- `randomizer/tests.rs` 新增：
  - 第 k 签可复现：同 (base_seed, k) 两次运行逐动物一致；不同 k 结果不同；
  - 定标确定性：同数据同种子两次定标得到相同 p0；
  - `TopFraction` 语义：被接受的签 min(P) ≥ p0；用定标同款模拟验证长期接受率
    落在 target_rate 邻域；
  - GLP 校验：`GlpSubmission + draw_index > 1` 报错；
- 导出重放测试（现有 `the_exported_sheet_can_be_re_sorted_into_the_same_grouping`）
  扩展到 k ≥ 2 的签；
- 汇总信息新行进 `exporter_test.rs`；
- P 值口径复核照旧走 Python 参考实现（`grouping_engine.py verify`）——定标只是
  对现有 P 值取分位数，不引入新的统计口径；
- 全量门禁按 CLAUDE.md：`cargo fmt` / `cargo test --release`（含 e2e 与
  `--ignored` 慢套件，因触及 `core/grouping/`）/ `cargo clippy --all-targets` /
  `bun run build`。
