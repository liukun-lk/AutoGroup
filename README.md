# AutoGroup

通用动物实验自动分组软件 —— 基于 Tauri 2 的桌面应用。导入动物实验的 Excel 原始数据后，通过统计学平衡算法自动将动物分配到实验组，保证各组在所选生物学指标上保持统计均衡（所有指标 P > α），并输出符合注册要求的 Excel 结果。

## 特性

- **Excel 导入**：支持 `.xlsx` / `.xls`，可通过文件选择、拖拽或剪贴板粘贴；自动解析多行表头（指标英文名 + 中文名/单位）并校验数据完整性
- **分组配置**：组数、每组动物数、性别比例约束、预留动物数量
- **统计配置**：筛选参与平衡的指标、显著性水平 α、严格 / 优化两种模式
- **纯 Rust 统计引擎**：
  - 2 组：Levene 方差齐性检验 → Student t / Welch t
  - ≥3 组：Levene 方差齐性检验 → One-way ANOVA + Tukey HSD / Welch ANOVA + Dunnett's T3
- **最优分组搜索**：枚举候选方案（大规模数据自动切换 Monte Carlo 采样），rayon 并行评估，实时展示进度
- **结果展示**：分组明细表、各指标 P 值、汇总统计、P 值分布图（ECharts）
- **Excel 导出**：双行表头格式（组别 + 动物编号 + 性别 + 指标值），支持导出多个分组结果
- **本地持久化**：SQLite 保存配置模板与分组历史
- **自动更新**：启动时通过 GitHub Releases 检查新版本

## 技术栈

| 层 | 技术 |
| --- | --- |
| 前端 | React 19 · TypeScript · Vite 7 · Tailwind CSS · shadcn/ui · TanStack Table · ECharts · Jotai |
| 后端 | Rust · Tauri 2.x |
| 数据处理 | calamine（Excel 解析）· rusqlite（SQLite）· rayon（并行计算）· rust_xlsxwriter（Excel 导出） |
| 统计 | statrs（概率分布）· 自实现 Levene / t / ANOVA / Tukey / Dunnett |
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

### 后端测试与检查

```bash
cd src-tauri
cargo test
cargo clippy -- -D warnings
```

测试数据位于 `docs/通用动物实验自动分组软件_测试用数据.xlsx`。

## 使用流程

1. **上传数据**：选择 / 拖拽 Excel 文件，预览解析结果
2. **配置参数**：设置分组与性别约束，选择统计指标与 α 值
3. **计算分组**：算法枚举候选并并行评估，展示实时进度
4. **查看结果**：核对分组明细与统计结果，导出 Excel

## 项目结构

```
AutoGroup/
├── src/                    # React 前端
│   ├── components/         # 页面与 shadcn/ui 组件
│   ├── stores/             # Jotai 状态
│   ├── lib/                # Tauri API 封装、更新检查
│   └── types/              # TypeScript 类型
├── src-tauri/              # Rust 后端
│   └── src/
│       ├── commands/       # Tauri IPC 命令
│       ├── core/           # 解析、分组算法、统计引擎、导出
│       ├── persistence/    # SQLite 仓储
│       └── utils/
├── docs/                   # 技术文档
├── .github/workflows/      # 发布流水线
└── package.json
```

## 文档

- [docs/README.md](docs/README.md) — 项目技术方案总览
- [docs/technical_specification.md](docs/technical_specification.md) — 需求与技术选型
- [docs/implementation_design.md](docs/implementation_design.md) — 代码结构与接口设计
- [docs/data_format_spec.md](docs/data_format_spec.md) — Excel 数据格式规范
- [docs/output_format_spec.md](docs/output_format_spec.md) — 导出格式规范

## 发布

推送 `v*` 标签后，GitHub Actions 会自动构建 macOS（Apple Silicon / Intel）与 Windows 安装包并生成 draft Release；应用内更新通过 Releases 中的 `latest.json` 完成。

## 许可证

本项目基于 [MIT License](LICENSE) 发布。Copyright (c) 2026 Kun Liu.
