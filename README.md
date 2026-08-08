# AutoGroup

通用动物实验自动分组软件，基于 Tauri 2 的桌面应用。导入动物实验的 Excel 原始数据后，可以用种子化随机分组（完全随机、分层区组随机、受限随机化）得到可审计、可复现的分配，也可以用统计均衡优化搜索让各组在所选指标上统计均衡（所有指标 P > α）的划分，最后导出带统计结果和抽签记录的 Excel 报告。

## 特性

### 数据导入

- 支持 `.xlsx`，旧版 `.xls` 会被拒绝并给出中文提示；可通过文件选择、拖拽或剪贴板粘贴导入
- 自动从数据内容识别表头行，解析多行表头（指标英文名 + 中文名/单位），并校验数据完整性
- 上传页提供最近导入列表和格式示例

### 分组方法

先声明研究场景（GLP 申报、确证性临床试验、探索性实验），软件据此收窄可用方法并给出推荐：

- 按主指标分层随机：按性别与主指标分层，层内种子化洗牌后按配额发牌，主指标均衡由构造保证。GLP 场景的默认方法
- 完全随机：种子化洗牌后按配额分配，不读取任何指标值
- 受限随机化：完全随机加预先声明的接受准则，不达标自动废签重抽。准则分两档：
  - 基础档：任一指标 P ≤ α 即废签
  - 增强档：先在本数据上做 1000 次种子化模拟定标，只接受最均衡的前 X%
- 统计均衡优化：枚举候选划分（超过 50 万组合自动切换种子化 Monte Carlo 采样），rayon 并行评估，按 min(P) / mean(P) 择优，分严格、优化两种判定模式。该方法不属于随机化，GLP 申报场景禁用，导出文件会如实标注分组原理
- 最小化法（协变量自适应随机化）：规划中，尚未实现

### 可复现性与审计

- 每一签由主种子和抽签序号唯一决定，随时可以从种子复现；抽过的签全部保留，可在候选方案间切换
- GLP 场景执行分配隐藏：一次抽签即为最终分配，不提供看到结果后重抽或挑选的入口
- 结果页展示复现卡片（种子、方法、接受准则、复现步骤），同样的内容写入导出文件的「汇总信息」
- 随机化方法导出附带「随机数」「区组」审计列，审阅者在 Excel 里重新排序即可手工复算组别

### 统计引擎

纯 Rust 实现，不依赖 Python 或 R：

- 2 组：Levene 方差齐性检验 → Student t / Welch t
- ≥3 组：Levene → One-way ANOVA + Tukey HSD / Welch ANOVA + Dunnett's T3
- 事后比较 P 值是精确值：自实现 studentized range 和 studentized maximum modulus 分布，与 Python 参考实现的偏差在 1e-11 量级
- 零方差输入短路处理，不会让分布函数收到 NaN 而崩溃

### 结果与导出

- 分组明细表、各指标 P 值、事后比较矩阵（按结论折叠展示）、汇总统计
- Excel 导出四张表：「分组结果」（双行表头，每组附均值±标准差行）、「统计结果」、「事后比较」（≥3 组时逐对一行）、「汇总信息」（含接受准则与抽签记录）
- 可一次导出 Top-N 多个候选方案

### 其他

- 启动时通过 GitHub Releases 检查新版本，应用内更新

## 技术栈

| 层 | 技术 |
| --- | --- |
| 前端 | React 19 · TypeScript · Vite 7 · Tailwind CSS · shadcn/ui · TanStack Table · Jotai |
| 后端 | Rust · Tauri 2.x |
| 数据处理 | calamine（Excel 解析）· rayon（并行计算）· rust_xlsxwriter（Excel 导出）· rand_chacha（种子化随机） |
| 统计 | statrs（概率分布）· 自实现 Levene / t / ANOVA / Tukey / Dunnett / studentized range & maximum modulus |
| 构建 | Bun · Cargo |

## 快速开始

### 环境要求

- Node.js >= 20.19（Vite 7 要求）
- Rust stable
- Bun
- 对应平台的 Tauri 2 系统依赖

### 开发

```bash
bun install
bun run tauri dev
```

### 构建发布版

```bash
bun run tauri build
```

### 测试与检查

```bash
cd src-tauri
cargo fmt
cargo test --release            # includes the end-to-end golden test
cargo clippy --all-targets
cd .. && bun run build          # tsc + vite build
```

改动涉及分组算法、统计引擎、解析或导出时，还要跑慢速套件：

```bash
cd src-tauri && cargo test --release -- --ignored   # real-data + performance harnesses
```

端到端黄金测试（`src-tauri/tests/e2e_grouping_test.rs`）用真实脱敏数据走完解析、分组、导出全流程，和人工验收过的导出文件逐单元格比对，是最重要的回归门禁。历史样例数据在 `docs/通用动物实验自动分组软件_测试用数据.xlsx`。

## 使用流程

1. 上传数据：选择或拖拽 Excel 文件，预览解析结果
2. 配置参数：声明研究场景，选择分组方法与接受准则，设置分组与性别约束、统计指标与 α 值
3. 计算分组：随机化方法即时抽签；优化方法枚举候选并并行评估，显示实时进度
4. 查看结果：核对分组明细、统计结果与复现卡片，导出 Excel。非 GLP 场景可以再抽一签或在候选间切换

## 项目结构

```
AutoGroup/
├── src/                    # React frontend
│   ├── components/
│   │   ├── features/       # Upload / Configure / Compute / Results pages
│   │   └── ui/             # shadcn/ui components
│   ├── stores/             # Jotai state
│   ├── lib/                # Tauri API wrappers, method metadata, updater
│   └── types/              # TypeScript types mirroring Rust models
├── src-tauri/              # Rust backend
│   ├── src/
│   │   ├── commands/       # Tauri IPC commands
│   │   ├── core/
│   │   │   ├── grouping/   # enumerator, evaluator, seeded randomizer
│   │   │   ├── stats/      # Levene, t, ANOVA, Tukey, Dunnett, distributions
│   │   │   ├── parser.rs   # Excel parsing (multi-row headers)
│   │   │   └── exporter.rs # Excel export
│   │   └── utils/
│   └── tests/              # end-to-end golden test + anonymized fixtures
├── docs/                   # design docs
├── .github/workflows/      # release pipeline
└── package.json
```

## 文档

- [docs/README.md](docs/README.md) — 项目技术方案总览
- [docs/technical_specification.md](docs/technical_specification.md) — 需求与技术选型
- [docs/implementation_design.md](docs/implementation_design.md) — 代码结构与接口设计
- [docs/data_format_spec.md](docs/data_format_spec.md) — Excel 数据格式规范
- [docs/output_format_spec.md](docs/output_format_spec.md) — 导出格式规范
- [docs/randomization_design.md](docs/randomization_design.md) — 随机化方法设计
- [docs/randomization_interaction_design.md](docs/randomization_interaction_design.md) — 随机化交互设计

## 发布

推送 `v*` 标签后，GitHub Actions 自动构建 macOS（Apple Silicon / Intel）与 Windows 安装包并生成 draft Release；应用内更新读取 Releases 中的 `latest.json`。

## 规划中

- 最小化法（序贯协变量自适应随机化）
- SQLite 本地持久化：配置模板与分组历史，仓储层已有雏形，尚未接入
- P 值分布可视化

## 许可证

本项目基于 [MIT License](LICENSE) 发布。Copyright (c) 2026 Kun Liu.
