# 统计检验：公式、Rust 实现与已知偏差

本文是 `src-tauri/src/core/stats/` 与 `scripts/grouping_engine.py` 的统计学对照表。
要改统计代码、要判断某个 P 值可不可信，先读这里。

- [1. 检验选择与判定](#1-检验选择与判定)
- [2. 各检验的公式](#2-各检验的公式)
- [3. 已量化的 Rust 偏差](#3-已量化的-rust-偏差)
- [4. 参考实现如何保证精度](#4-参考实现如何保证精度)
- [5. 新增一个检验](#5-新增一个检验)

## 1. 检验选择与判定

```
p_levene = levene(groups)                     # 方差齐性
homoscedastic = p_levene > α

k == 2:  homoscedastic ? student_ttest : welch_ttest        # 无事后检验
k >= 3:  homoscedastic ? anova + tukey_hsd : welch_anova + dunnett_t3

is_valid = (p_main > α) && all(p_posthoc > α)
```

α 同时用于三处：方差齐性分流、主检验判定、事后比较判定。改 α 会同时改变走哪条分支，
所以 α 变化后的结果不能与旧结果直接比较（不是单调收紧的关系）。

## 2. 各检验的公式

**Levene（方差齐性）** —— 把每个观测换成 |x − center|，再对变换后的数据跑 one-way ANOVA。
`center = mean` 是原始 Levene（**Rust 用这个**）；`center = median` 是 Brown–Forsythe
变体（scipy 默认），对偏态指标更稳。项目 CLAUDE.md 里写的 "Brown-Forsythe variant" 与
`levene.rs` 的实际实现不符——实现用的是均值。Python 侧 `--levene mean` 与 Rust 对齐，
`--levene median` 提供更稳健的版本。

**Student t** —— 合并方差 `s²_p = ((n₁−1)s₁² + (n₂−1)s₂²)/(n₁+n₂−2)`，
`t = (x̄₁−x̄₂)/√(s²_p(1/n₁+1/n₂))`，df = n₁+n₂−2，双尾。

**Welch t** —— `t = (x̄₁−x̄₂)/√(s₁²/n₁+s₂²/n₂)`，Welch–Satterthwaite df：
`df = (s₁²/n₁+s₂²/n₂)² / [(s₁²/n₁)²/(n₁−1) + (s₂²/n₂)²/(n₂−1)]`，双尾。

**One-way ANOVA** —— `F = (SSB/(k−1)) / (SSW/(N−k))`，上尾 F 概率。

**Welch ANOVA** —— 权重 `wᵢ = nᵢ/sᵢ²`，加权总均值 `x̄_w = Σwᵢx̄ᵢ/Σwᵢ`，
`h = Σ(1−wᵢ/Σw)²/(nᵢ−1)`，
`F = [Σwᵢ(x̄ᵢ−x̄_w)²/(k−1)] / [1 + 2(k−2)h/(k²−1)]`，df₁ = k−1，df₂ = (k²−1)/(3h)。
任一组内方差为 0 时权重 `nᵢ/sᵢ²` 发散：Python 返回 NaN，Rust 返回 `Err`（`FisherSnedecor::new`
拒绝 NaN 自由度）。两边都把这种输入判为**无定义**，但后续处理不同——见下面「退化输入」。

**Tukey HSD** —— `q = |x̄ᵢ−x̄ⱼ| / √(MSE(1/nᵢ+1/nⱼ)/2)`，P 值取自
**studentized range 分布** `q(k, df_within)`。这是唯一正确的来源；用 t 分布加
Bonferroni 之类的替代都会引入方向不定的偏差（见 §3）。

**Dunnett's T3** —— 每对用 Welch t 统计量与 Welch df，然后按
**studentized maximum modulus** 分布（C = k(k−1)/2 个比较）做多重性校正。
没有这一步校正就不是 T3，只是裸的两两 Welch t 检验。

### 退化输入（组内方差为 0）

只要某次划分让**每个组内部各自恒定**，检验统计量就退化成 `0/0`。这不是罕见构造：随机化路径
每抽一次都可能让某个指标在某组内取值全同，而抽出来的那个划分就是最终分配，没有别的候选可退。
处理方式与 Python 参考实现逐字一致——**没有离散度可言时，答案由均值是否相等直接决定**：

| 位置 | 条件 | 返回 |
| --- | --- | --- |
| `one_way_anova`（Levene 也走它） | `SSW ≤ 0` | 均值全同 → 1.0，否则 → 0.0 |
| `student_ttest` / `welch_ttest` | `SE ≤ 0` | 两均值相等 → 1.0，否则 → 0.0 |
| `tukey_hsd` / `tukey_all_valid` | 该对的 `SE ≤ 0` | 两均值相等 → 1.0，否则 → 0.0 |
| `welch_anova` / Dunnett's T3 | 任一组方差为 0 | Python NaN / Rust `Err`：**无定义**，不产出 P 值 |

前三行必须成对存在。缺了它们，`FisherSnedecor::cdf(NaN)` 与 `StudentsT::cdf(NaN)` 会在 statrs
内部 `unwrap()` 上 **panic**，整次运行崩掉；Tukey 则会给出一整列 NaN，静默读作「不通过」，
而实际上三个完全相同的组是最均衡的情形。Tukey 的两条路径（`ValidityOnly` 快捷判定与 `Exact`
精确 P 值）共用同一个 `pairwise` 判定，退化分支也不例外——两者结论必须一致，这条有单测钉住。

最后一行的「无定义」在引擎里由 `evaluator::Untestable` 决定后果：优化路径用 `Abort`（丢弃该候选，
反正还有十万个），随机化路径用 `Skip`（跳过该指标，与「组内有效值不足 2 个」同一约定）。因此
随机化结果里 `total_indicators` 可能小于用户所选指标数，报告通过率时必须把两者一起报。

## 3. Rust 与精确实现的一致性

**当前状态：两侧在所有检验上一致，Rust 的结果可以直接采信。**

`src/core/stats/distributions.rs` 是 `grouping_engine.py` 里 `srange_sf` / `smm_sf` /
`_chi_scale_integral` 的直接移植（同样的 Gauss-Legendre 求积、同样的 per-k 插值表），
Tukey 与 Dunnett's T3 因此都走精确分布。实测：

- e2e 固定数据（9 只，3 组，70 指标）的全部 **210 个两两比较**，Rust 与 Python 的
  最大绝对偏差 **1.79e-11**，达标判定 0 处不一致。
- 公开临界值表：q₀.₀₅(3,12) = 3.773、q₀.₀₅(4,20) = 3.958、q₀.₀₅(5,10) = 4.654，
  Rust 侧 `srange_crit` 三项全部命中（`srange_crit_matches_published_tables`）。
- 解析式恒等：studentized range 在 k = 2 时退化为双尾 t(q/√2)，studentized maximum
  modulus 在 C = 1 时退化为双尾 t，两条恒等式都有单元测试钉住。

Levene P、Student/Welch t、One-way / Welch ANOVA、min(P)/mean(P) 排序、候选枚举数量
本来就逐项一致，未受影响。

### 曾经的偏差（2026-08 之前，已修复）

历史上这两个事后检验是近似实现，留档以便识别回归：

- *Tukey HSD* 用 `t = q/√2` 的双尾 t 概率乘以 k 再截到 [0,1]。3 组每组 3 只
  （df_within = 6）时 21 个比较**全部饱和成 1.000000**（精确值分布在 0.656–0.9997），
  事后检验形同虚设；判定门槛被抬到 q = 4.649，而精确临界值是 4.339，
  q ∈ [4.34, 4.65] 的组对会被误判为通过。
- *Dunnett's T3* 返回未校正的两两 Welch t 双尾 P，P 值只有真值的 0.20–0.43 倍，
  会把合格方案判为不合格。

修复后在 e2e 数据上的可观测变化：3 组 × 每组 5 只的性能用例里合格候选从 123540 降到
119124（−3.6%），正是原先落在误判窗口里的那些；最优划分与导出的前三张 sheet 逐格未变。

`tukey.rs::tukey_p_values_are_not_saturated_on_small_samples` 与
`dunnett.rs::dunnett_t3_is_more_conservative_than_bare_welch_t` 分别守着这两种回归。

### 性能与判定路径

精确 P 值每个都要做一次数值积分，不能在 10^5 量级的评分循环里逐候选算。按算法契约，
**排序只看主检验 P，事后比较只影响 `is_valid`**，所以 `compute_indicator_test` 接受一个
`PostHocDetail`：

- `ValidityOnly`（评分阶段）——Tukey 拿缓存的 `srange_crit(alpha, k, df)` 与 q 比大小；
  T3 用「单个未校正 t（下界）/ Bonferroni 倍数（上界）」夹逼，只有落进窗口的比较才做积分
  （T3 的 Welch df 每对都不同，没有可缓存的临界值）。
- `Exact`（Top-N 上报）——真正算 `srange_sf` / `smm_sf`。

两条路径的判定结果按构造完全相同（临界值是尾概率的反函数，夹逼是严格不等式），
`tukey_all_valid_agrees_with_exact_p_values` / `dunnett_all_valid_agrees_with_exact_p_values` /
`smm_exceeds_agrees_with_exact_tail` 逐点验证了这一点。改这里时别让两条路径分叉——
一旦分叉，候选会以"合格"入选，却报出一个不达标的比较。

**交付物暴露面**：导出的 xlsx 现在包含 `事后比较` sheet（每行一个「指标 × 组对」），
所以事后 P 值会直接进入交付文件——这也是它必须精确的原因。

## 4. 参考实现如何保证精度

`grouping_engine.py` 只用标准库，所以每个分布都是自己算的，靠 `self-test` 的 27 项检查兜底：

- **正则化不完全 beta**（t 与 F 的尾概率来源）对照独立的数值积分，含 a < 1 的奇点情形。
- **解析式恒等**：ANOVA(k=2) ≡ Student t；Welch ANOVA(k=2) ≡ Welch t；
  studentized range 在 k=2 时 ≡ 双尾 t(q/√2)；studentized maximum modulus 在 C=1 时 ≡ 双尾 t。
  这些恒等式把新写的分布函数钉在已验证的分布上。
- **公开临界值表**：q₀.₀₅(3, 12) = 3.773、q₀.₀₅(4, 20) = 3.958、q₀.₀₅(5, 10) = 4.654，
  实现给出 3.7729 / 3.9583 / 4.6543。
- **Monte-Carlo 交叉验证**：直接模拟 studentized range 与 maximum modulus 的定义，
  与解析结果在 4σ 内一致。

改动任何数值代码后跑 `python3 scripts/grouping_engine.py self-test`；它约 1 秒完成，
27 项必须全过。

实现上值得知道的一点：studentized range 的 P 值是二重积分，内层只依赖 (w, k)，
所以按 k 缓存成插值表，外层再对合并标准差的分布积分。这让精确 Tukey P 值在纯 Python 里
也便宜到可以随便算。

## 5. 新增一个检验

1. 在 `src-tauri/src/core/stats/` 新建模块，签名 `fn name(groups: &[Vec<f64>]) -> Result<f64>`
   （事后检验返回 `Vec<(usize, usize, f64)>`）。
2. 在 `stats/mod.rs::compute_p_value` 的级联里接入，并在返回的 `test_method` 字符串里
   写明方法名——这个字符串会直接出现在导出结果里，是审计线索。
3. 在 `grouping_engine.py` 的 Section 2 写同一个检验，并在 `cmd_self_test` 里加至少一条
   解析式恒等或已发表临界值的检查。没有这条检查，oracle 对新检验就是失效的。
4. 单元测试放模块内 `#[cfg(test)]`，至少覆盖"均值相同 → P 高"和"均值差异大 → P 低"两个方向，
   以及退化输入（组内方差为 0、n = 2）。
