---
name: animal-grouping
description: AutoGroup 实验动物自动分组引擎的权威规格（候选枚举、性别配额、备用组、Levene/t/ANOVA/Tukey/Dunnett 检验选择、Strict/Optimized 模式、Top-N 排序），附带零依赖 Python 参考实现用于精确复核 P 值。只要涉及"分组是否统计平衡""这个 P 值对不对""为什么找不到有效分组""导出结果能不能信""要改 grouping/stats 代码""新增统计检验""换 α 或指标后结果变了"等问题，都要用本 skill；即使用户没有说"分组算法"，只要触及 src-tauri/src/core/grouping、src-tauri/src/core/stats、compute_grouping/compute_optimal_grouping、P 值、方差齐性、事后检验、备用动物、min(P)/mean(P) 排序，也一律先加载本 skill 再动手。Use for any AutoGroup grouping-algorithm or statistical-balance question.
---

# 动物实验自动分组引擎

分组的唯一目标：把动物分到各实验组，使**所选全部指标在组间都检验不出差异**（P > α）。这与常规统计相反——这里"高 P 值"是成功，"显著差异"是失败。判断一个方案好不好，靠的是精确的 P 值，不是直觉，也不是手算。

生产引擎是 Rust（快，供 UI 调用），`scripts/grouping_engine.py` 是精确参考实现（慢一点，但统计正确，零依赖）。**两者不一致时以 Python 为准**，Rust 的近似偏差已量化在 `references/statistics.md`。

## 先按任务定位

| 用户在问什么 | 怎么做 |
|---|---|
| 这个 P 值/这份结果对不对，能不能信 | 跑 `verify`（下方），别手算，别只读代码推断 |
| 帮我算一版分组 / 试试换 α、换指标、换组数 | 跑 `group` |
| 为什么"找不到有效分组" | 见下方「排障」，按顺序排除 |
| 要改枚举、评估、排序或统计代码 | 先读本文「算法契约」，再读 `references/rust-map.md` |
| 要加一个统计检验，或质疑现有检验的实现 | 读 `references/statistics.md`（含公式与 Rust 偏差清单） |
| 改完代码怎么验证 | `references/rust-map.md` 的测试矩阵 + `self-test` |

## 算法契约

五个阶段，顺序固定，每个阶段的不变量都不能被后续阶段破坏：

1. **前置校验**（`enumerator::validate_config`）——各组配额之和必须逐性别精确等于数据集；实验组至少 2 只（统计需要方差），备用组可以只有 1 只。这里失败要直接报配置错误，不要进入枚举。
2. **候选枚举**（`enumerator::enumerate_all`）——按性别分层：雄性池里选 male_count，雌性池里选 female_count，逐组递归，最后一组吃掉剩余动物。组是**有标号**的：交换两个配额相同的组会产生另一个候选。2 组走专用快路径，多组走递归穷举；组合数超过 500 000 时 Rust 退化为 Monte Carlo 抽样（`thread_rng`，不可复现——Python 参考实现用固定种子，便于复核）。
3. **候选评估**（`evaluator::evaluate_grouping_with_constraints`）——对每个候选、每个选中指标跑检验级联（见下）。备用组（`GroupType::Reserve`）**不进入任何统计**，只是领走动物。
4. **过滤**——Strict 模式要求 `num_invalid_indicators == 0`；Optimized 模式允许 ≤ 1 个指标不通过。
5. **排序取 Top-N**——先按 `min_p_value` 降序，再按 `mean_p_value` 降序，取前 `max_candidates` 个（默认 10）。

### 检验级联

对每个指标，先用 Levene 判方差齐性，再按组数选主检验：

| 组数 | Levene P > α（方差齐） | Levene P ≤ α（方差不齐） |
|---|---|---|
| 2 | Student t 检验 | Welch t 检验 |
| ≥3 | One-way ANOVA + Tukey HSD | Welch ANOVA + Dunnett's T3 |

判定规则：

```
is_valid = (主检验 P > α) AND (≥3 组时，所有两两事后比较 P 都 > α)
```

**关键事实：排序只看主检验 P（min 然后 mean），事后比较只影响 is_valid。** 这既是语义（组间整体无差异 + 任意两组都无差异），也是性能杠杆：筛选阶段只需判断事后比较是否全部过关（拿临界值比 q 统计量即可），精确事后 P 值只对最终要展示的 Top-N 算。Python 参考实现就是这么做的。

### 数据缺失的处理

某指标在任一实验组中有效值不足 2 个时，该指标被**整体跳过**（`continue`），既不计入 `total_indicators` 也不计入 `mean_p_value`。这是静默行为：结果看起来"全部通过"，实际是少测了指标。凡是报告通过率，都要同时报告 `total_indicators` 与用户选择的指标数是否相等。

真实数据上就会遇到：项目测试用 xlsx 解析出 71 个指标 key，但 `样本号`、`样品识别号`、`FULLNAME` 三列是文本，没有数值，选"全部指标"实际只检验了 68 个。所以结论要写"68/68 通过（71 个中 3 个非数值列未参与）"，不能写"71 个全部通过"。

## 精确复核工具

`scripts/grouping_engine.py`，只用标准库（无需 scipy/openpyxl/pandas；在 Python 3.14 上实测通过），能直接读项目的 .xlsx（复用了 `parser.rs` 的双行表头 key 规则，所以 `--indicators kg,ALT` 这类 key 与 Rust 一致）。

```bash
S=.claude/skills/animal-grouping/scripts/grouping_engine.py

# 1) 算一版分组：9 只动物（6 雄 3 雌）分成 3雄2雌 + 3雄1雌
python3 $S group --excel "docs/通用动物实验自动分组软件_测试用数据.xlsx" \
  --groups "3M+2F,3M+1F" --indicators "kg,ALT,AST,TP,GLU" --top 3

# 三组等配额 + 备用组；--dedup 去掉"同一划分换标号"的重复候选；--means 出 mean±SD（写报告要）
python3 $S group --excel data.xlsx --groups "2M+1F,2M+1F,2M+1F" --dedup --top 5 --means
python3 $S group --excel data.xlsx --groups "3M+1F,3M+1F,0M+1F:reserve=备用动物" --indicators all

# 2) 复核一个既有分配。指标/α/模式必须与被复核的那次运行一致，否则算的是另一个问题
python3 $S verify --excel data.xlsx --assignments result.json \
  --indicators "kg,ALT,AST,TP,ALB,GLU" --alpha 0.05 --mode strict --compare

# group --output 的结果可以直接回喂复核，--candidate 选第几个候选（默认最优的那个）
python3 $S group  --excel data.xlsx --groups "2M+1F,2M+1F,2M+1F" --top 3 --output top3.json
python3 $S verify --excel data.xlsx --assignments top3.json --candidate 1 --indicators "kg,ALT"

# 3) 验证统计内核本身（27 项检查：解析式恒等、公开临界值表、Monte-Carlo 交叉验证）
python3 $S self-test
```

常用开关：`--mode optimized`（允许 1 个指标不达标）、`--alpha`、`--levene median`（Brown-Forsythe，更抗偏态）、`--posthoc rust`（复现 Rust 近似以便对比）、`--means`、`--output result.json`、`--json`。
`--indicators` 默认是 `all`（全部解析到的 key），复核既有运行时**一定要显式传**与原运行相同的指标集。
`--output` 的 JSON 里每个指标带 `levene_p_value` / `diff_p_value` / `test_method` / `is_valid` / `posthoc_results` / `group_stats`（各组 mean、sd、n），可直接用于程序化对账。
分配文件接受三种结构：顶层数组、`{"assignments": [...]}`、`{"candidates": [...]}`；记录里带 `is_reserve` 时会自动识别备用组，否则用 `--reserve 2` 指定。

用它做三件事：**给结论提供证据**（别口算 P 值）、**审计 Rust 输出**（`verify --compare` 会直接列出精确值与 Rust 近似判定不同的比较对）、**改完统计代码后当回归基线**。

## 排障

**"No valid grouping found"** —— 按这个顺序排除，多数情况停在前两条：

1. 配置本身不可行：配额之和与性别数不匹配，或实验组只有 1 只。先跑 `group`，Python 会一次列出所有不匹配项，比 Rust 逐条报错快。
2. 指标间存在天然强差异：某指标个体差异极大（体重跨度大、CHOl 之类离散指标），任何划分都无法平衡。用 `group --mode optimized` 或减少指标定位是哪一个——`group` 输出里 min(P) 对应的指标就是瓶颈。
3. α 设得过大（如 0.1）：α 越大越难通过（要求 P > α）。
4. 组数 ≥3 时事后比较也要全部过关（`is_valid` 同时要求主检验和每一对都 P > α）。整体 ANOVA 通过、但某一对组间差异显著，同样会被判为不合格——`group` 输出里逐对列出的事后 P 值能直接指出是哪一对。

**其他已知边界**：所有指标都被跳过时，Rust 的 `min_p_value` 会保持 `f64::MAX`（约 1.8e308）而 `mean_p_value` 为 0，摘要失去意义；Top-N 里可能出现同一划分的不同组标号（配额相同的组可互换，统计量完全一样），Python 的 `--dedup` 可以消除，Rust 目前不做去重。

## 被问到"这份结果能不能交付"时

先分清楚哪部分可信，再回答，别一句"可以/不可以"：

- **动物分配本身**：由 Levene + 主检验决定排序，Rust 与精确实现完全一致，选出来的划分可信。
- **要写进报告的数字**：Levene P、主检验 P、事后比较 P 现在都走精确分布，可以直接用 Rust 输出（e2e 数据上 210 个两两比较与 Python 参考实现最大偏差 1.8e-11）。要对外交付（报告、审计、监管材料）时仍建议跑一次 `verify --compare` 留证据，但预期是 0 处不一致。
- **交付文件的暴露面**：导出 xlsx 包含 `分组结果` / `统计结果` / `事后比较` / `汇总信息` 四张表。「统计结果」是每指标一行的五列宽表；「事后比较」是每行一个「指标 × 组对」的长表（两组设计没有事后阶段，该表不生成）。
- **通过率的写法**：报"68/68 通过（所选 71 个中 3 个非数值列未参与）"，别报"71 个全部通过"。
- **历史包袱**：2026-08 之前的版本里 Tukey / Dunnett's T3 是近似实现，事后 P 值不可信（详见 `references/statistics.md` 的「曾经的偏差」）。如果手上是那之前导出的旧文件，事后数字要重算。

## 改代码时

修改分组或统计逻辑，务必两侧同步：Rust 改了行为，Python 参考实现要跟上，否则 oracle 失效；`references/rust-map.md` 有文件位置、扩展步骤和测试命令，`references/statistics.md` 有每个检验的公式与已知偏差。评估路径是 `rayon` 并行的纯函数，加状态会破坏可复现性。
