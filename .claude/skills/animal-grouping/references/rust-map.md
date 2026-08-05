# Rust 代码地图与改动流程

分组相关代码的位置、数据流、扩展点与测试命令。所有 cargo 命令都必须在 `src-tauri/` 下执行。

## 文件职责

| 路径 | 职责 | 改这里要注意 |
|---|---|---|
| `src/core/models.rs` | `Animal` / `Dataset` / `GroupConfig` / `SexConstraint` / `StatConfig` / `GroupingResult` | 结构体经 serde 直达前端，字段改名等于破坏 IPC 契约，需同步 `src/types/` |
| `src/core/parser.rs` | xlsx 解析，双行表头 → 指标 key/display_name/unit | `parse_dual_row_header` 决定指标 key；改它会让所有已保存配置里的指标名失效 |
| `src/core/validator.rs` | 数据集级校验（编号唯一、完整性） | |
| `src/core/grouping/enumerator.rs` | 配置前置校验、性别分层候选枚举、Monte Carlo 退化 | 枚举顺序影响 Top-N 中同分方案的先后 |
| `src/core/grouping/evaluator.rs` | 单候选评估：逐指标检验、备用组排除、汇总 | 纯函数，被 `rayon` 并行调用，禁止引入可变共享状态 |
| `src/core/grouping/mod.rs` | 编排：枚举 → 并行评估 → 过滤 → 排序 → Top-N | 排序键在这里（min_p 降序，mean_p 次之） |
| `src/core/stats/*` | Levene / t / ANOVA / Tukey / Dunnett | 见 `statistics.md` |
| `src/core/exporter.rs` | 双行表头 Excel 导出 | 表头格式是交付要求，不要退回单行 |
| `src/commands/grouping.rs` | Tauri 命令入口 | 新命令要在 `lib.rs` 的 `invoke_handler!` 注册 |

数据流：`parse_excel` → `Dataset` →（前端配置）→ `compute_grouping` →
`enumerate_all` → `par_iter(evaluate)` → 过滤/排序 → `MultiGroupingResult` → `export_result`。

## 三个常见改动的落点

**改分组语义（配额、备用组、约束）** —— `enumerator.rs::validate_config` 加前置校验，
`enumerate_recursive` 改枚举，`evaluator.rs` 里 `experimental_groups` 那段决定谁参与统计。
新语义必须同时反映到 `scripts/grouping_engine.py` 的 `GroupSpec` / `enumerate_candidates`，
否则复核工具会和生产引擎讲不同的规则。

**改排序或筛选** —— 只在 `grouping/mod.rs`。注意 `OptimizationMode::Optimized` 的语义是
"最多允许 1 个指标不通过"，不是"按加权分数排序"；如果要改成打分制，
`ResultSummary.meets_criteria` 的含义也要跟着改，前端有依赖。

**改统计** —— 见 `statistics.md` §5。

## 测试矩阵

```bash
cd src-tauri

cargo test                      # 全量
cargo test stats                # 统计内核
cargo test grouping             # 枚举 + 评估单测（含组合数断言 120 / 540）
cargo test exporter             # 导出格式
cargo fmt && cargo clippy -- -D warnings

# 真实数据端到端（默认 #[ignore]，必须显式打开）
cargo test --lib real_data_test -- --ignored --nocapture
```

`real_data_test.rs` 里的 xlsx 路径是硬编码绝对路径，换机器要改。它同时覆盖 2 组和 3 组两个
场景，输出完整的 P 值表——这是与 Python 参考实现对账的最佳素材：

两个测试的配置不同，对账时必须逐个对齐（配额、指标集、模式都要一致，否则算的不是同一个问题）：

| 测试 | 配额 | 指标 | 模式 |
|---|---|---|---|
| `test_with_real_excel_data` | 3M+2F, 3M+1F | kg ℃ ALT AST TP ALB GLU UREA CREA CHOl（10 个） | Strict |
| `test_three_groups_real_data` | 2M+1F ×3 | kg ℃ ALT AST TP ALB GLU（7 个） | Optimized |

```bash
X="docs/通用动物实验自动分组软件_测试用数据.xlsx"
S=.claude/skills/animal-grouping/scripts/grouping_engine.py

# Rust 侧（两个测试一起跑）
cd src-tauri && cargo test --lib real_data_test -- --ignored --nocapture

# Python 侧：2 组
python3 $S group --excel "$X" --groups "3M+2F,3M+1F" \
  --indicators "kg,℃,ALT,AST,TP,ALB,GLU,UREA,CREA,CHOl" --mode strict --top 1

# Python 侧：3 组（k>=3 才有事后检验偏差，这个才是最需要对账的场景）
python3 $S group --excel "$X" --groups "2M+1F,2M+1F,2M+1F" \
  --indicators "kg,℃,ALT,AST,TP,ALB,GLU" --mode optimized --top 1

# 拿 Rust 获胜划分做逐对对账：把它的 assignments 写成 JSON 后
python3 $S verify --excel "$X" --assignments rust_winner.json \
  --indicators "kg,℃,ALT,AST,TP,ALB,GLU" --mode optimized --compare
```

已核对基线（2026-08 复核）：2 组场景两侧的 Levene P、主检验 P、min(P) = 0.362080、
mean(P) = 0.670030、最佳划分与候选数 60 全部一致；3 组场景 Levene P 与 ANOVA P 也逐项一致
（min(P) = 0.624988），候选 540、合格 534 两侧相同。差异只在事后检验，见 `statistics.md` §3。

## 规模与性能

穷举适用于 ≤ 50 只动物；组合数超过 500 000 时 Rust 切换为 100 000 次 Monte Carlo 抽样
（`rand::thread_rng`，不可复现——这会让同一份数据两次运行给出不同结果，若要可复现必须
换成固定种子的 `StdRng`）。评估阶段是 CPU 密集且线程安全的，`rayon` 已并行。
Python 参考实现默认在候选数超过 `--max-enumerate`（20 万）时用固定种子抽样，
适合复核而非生产规模计算。
