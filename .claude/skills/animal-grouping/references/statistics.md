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
组内方差为 0 时权重 `nᵢ/sᵢ²` 发散，Rust 与 Python 都会得到 NaN，`NaN > α` 为假，
该指标因此被判为不通过——这是可接受的失败方式，但要知道原因是数据退化而非组间失衡。

**Tukey HSD** —— `q = |x̄ᵢ−x̄ⱼ| / √(MSE(1/nᵢ+1/nⱼ)/2)`，P 值取自
**studentized range 分布** `q(k, df_within)`。这是唯一正确的来源；用 t 分布加
Bonferroni 之类的替代都会引入方向不定的偏差（见 §3）。

**Dunnett's T3** —— 每对用 Welch t 统计量与 Welch df，然后按
**studentized maximum modulus** 分布（C = k(k−1)/2 个比较）做多重性校正。
没有这一步校正就不是 T3，只是裸的两两 Welch t 检验。

## 3. 已量化的 Rust 偏差

用同一份真实测试数据（`docs/通用动物实验自动分组软件_测试用数据.xlsx`，9 只，6 雄 3 雌）
对比 Rust `cargo test --lib real_data_test -- --ignored` 与 Python `verify` 的结果：

**完全一致**（6 位小数逐项相同）：xlsx 解析出的指标 key、候选枚举数量（60 / 540）、
Levene P、Student/Welch t 的 P、One-way / Welch ANOVA 的 P、min(P)/mean(P) 排序、最佳划分。
也就是说 **2 组场景下 Rust 与精确实现没有差别**，可以直接信任。

**有偏差**（只在 k ≥ 3 的事后检验上）：

*Tukey HSD*（`tukey.rs::tukey_q_to_p`）—— Rust 把 q 折算成 `t = q/√2` 后取双尾 t 概率再乘以 k
并截到 [0,1]。偏差方向随 k 反转，不是代码注释所说的"保守近似"：

| k | df | q | 精确 P | Rust P | 比值 |
|---|---|---|---|---|---|
| 3 | 6 | 3.0 | 0.16546 | 0.23442 | 1.42（偏松） |
| 3 | 12 | 4.0 | 0.03764 | 0.04566 | 1.21（偏松） |
| 4 | 8 | 5.0 | 0.03139 | 0.03068 | 0.98 |
| 5 | 15 | 4.0 | 0.08046 | 0.06354 | 0.79（偏严） |
| 5 | 15 | 5.0 | 0.02139 | 0.01498 | 0.70（偏严） |

在实际数据上后果更极端：3 组、每组 3 只（df_within = 6）时，乘 k 后几乎所有比较都被截到
**恰好 1.000000**（Rust 输出的 21 个两两比较全是 1.0，精确值分布在 0.656–0.9997）。
事后检验此时形同虚设——`is_valid` 实际只由 ANOVA 的 P 决定。

比 P 值倍数更能说明后果的是**判定门槛**，因为真正要担心的是"会不会把真实失衡的组对放过去"。
α = 0.05 下，精确临界值与 Rust 实际生效的门槛（解 `min(1, k·t₂(q/√2, df)) = 0.05`）：

| k | df | 精确 q₀.₀₅ | Rust 生效门槛 | 误判窗口 |
|---|---|---|---|---|
| 3 | 6 | 4.3392 | 4.6492 | q ∈ [4.34, 4.65] 会被误判为通过 |
| 3 | 12 | 3.7729 | 3.9308 | q ∈ [3.77, 3.93] |
| 4 | 8 | 4.5288 | 4.5339 | 几乎重合 |

也就是说 3 组小样本时窗口最宽。项目测试数据恰好没有候选落进这个窗口（540 个候选用两种算法
筛出的合格数都是 534），所以现有结论没被改变——但这是数据碰巧，不是算法保证。

**交付物暴露面**：`exporter.rs::write_statistics_sheet_to` 只写指标名、Levene P、差异检验 P、
检验方法、是否达标五列，导出的 xlsx **不含事后比较**。所以这两处偏差只影响界面显示与结果
JSON，不会污染导出文件——回答"能不能交付"时这一点决定了是"替换界面数字"还是"重新导出"。

*Dunnett's T3*（`dunnett.rs`）—— Rust 返回未校正的两两 Welch t 双尾 P，缺 studentized
maximum modulus 校正，P 值系统性偏小，把合格方案判为不合格：

| C | df | t | 精确 P | Rust P | 比值 |
|---|---|---|---|---|---|
| 3 | 4 | 2.5 | 0.15961 | 0.06677 | 0.42 |
| 3 | 8 | 3.0 | 0.04698 | 0.01707 | 0.36 |
| 6 | 10 | 2.5 | 0.15381 | 0.03145 | 0.20 |

即 3 组时 Rust 的事后 P 只有真值的约 0.35–0.43 倍，4 组时约 0.2 倍。α = 0.05 下，
一个真实 P = 0.10 的比较会被 Rust 报成 0.04 并判为失衡。

**结论**：k = 2 用 Rust 结果即可；k ≥ 3 且要对外交付（报告、审计、监管材料）时，
用 `verify --compare` 复核事后检验。修 Rust 时，两处的正确做法分别是接入
studentized range 与 studentized maximum modulus 分布——`grouping_engine.py`
里的 `srange_sf` / `smm_sf` 是可移植的参考（纯数值积分，无外部依赖）。

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
