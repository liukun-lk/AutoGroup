# AutoGroup 项目技术方案总览

> 通用动物实验自动分组软件 - 完整技术方案
>
> 文档版本: v1.0
> 创建日期: 2026-02-12

---

## 📋 文档索引

1. [技术规格说明](./technical_specification.md) - 整体架构、技术栈选型、模块设计
2. [实施设计方案](./implementation_design.md) - 详细代码结构、接口定义、开发路线图
3. [数据格式规范](./data_format_spec.md) - Excel 解析规则、测试数据分析
4. [输出格式规范](./output_format_spec.md) - 分组结果导出格式、UI 工作流

---

## 🎯 项目目标

开发一款 **桌面应用**（Tauri），实现：
1. 导入动物实验原始数据（Excel）
2. 根据用户配置的分组规则和统计参数自动计算最优分组
3. 输出符合注册要求的分组结果表格（Excel/CSV）

**核心价值：**
- 替代手工分组，节省 80% 时间
- 确保统计学平衡（所有指标 P > α）
- 可重复、可追溯的分组结果

---

## 🏗️ 技术架构

### 技术栈（已确认）

**前端:**
- React 19 + TypeScript
- UI 组件: **shadcn/ui** (基于 Radix UI + Tailwind CSS)
- 表格: **TanStack Table** (虚拟滚动)
- 图表: **ECharts** (P 值分布可视化)
- 状态管理: **Jotai** (轻量级 atom 状态)

**后端 (Rust):**
- 框架: **Tauri 2.x**
- Excel 解析: **calamine**
- 统计计算: **纯 Rust 实现** (statrs + 自实现 ANOVA/Tukey/Dunnett's T3)
- 数据库: **SQLite** (rusqlite)
- 并行计算: **rayon**
- Excel 导出: **rust_xlsxwriter**

### 架构图

```
┌──────────────────────────────────────────────┐
│          Frontend (React + shadcn/ui)        │
│  ┌──────────────┬───────────────────────┐    │
│  │ 数据导入     │ Jotai Store (原子状态) │    │
│  │ 参数配置     │ - datasetAtom         │    │
│  │ 结果展示     │ - configAtom          │    │
│  │ 导出管理     │ - resultAtom          │    │
│  └──────────────┴───────────────────────┘    │
│            ↕ Tauri IPC Commands              │
└──────────────────────────────────────────────┘
                    ↕
┌──────────────────────────────────────────────┐
│         Backend (Rust / Tauri)               │
│  ┌─────────────────────────────────────────┐ │
│  │ Commands Layer                          │ │
│  │ - parse_excel                           │ │
│  │ - compute_grouping                      │ │
│  │ - export_result                         │ │
│  └─────────────────────────────────────────┘ │
│  ┌──────────┬──────────────┬──────────────┐  │
│  │ Parser   │ Algorithm    │ Stats Engine │  │
│  │ (Excel)  │ (Grouping)   │ (Pure Rust)  │  │
│  └──────────┴──────────────┴──────────────┘  │
│  ┌─────────────────────────────────────────┐ │
│  │ Persistence (SQLite)                    │ │
│  │ - Config Templates                      │ │
│  │ - Grouping History                      │ │
│  └─────────────────────────────────────────┘ │
└──────────────────────────────────────────────┘
```

---

## 📊 数据流程

### 1. 导入阶段
```
用户上传 Excel
  ↓
Frontend: 调用 Tauri file dialog 获取路径
  ↓
Backend: parse_excel(file_path)
  ↓ calamine 解析
  ↓ 数据验证（AnimalID 唯一性、Sex 有效性、指标完整性）
  ↓
返回 Dataset { animals, indicator_names, metadata }
  ↓
Frontend: 更新 datasetAtom，显示数据预览
```

### 2. 配置阶段
```
用户配置参数
  ↓
Frontend 组件:
  - GroupConfigPanel: 分组数、每组动物数、性别约束
  - StatConfigPanel: 选择指标、α 值、优化模式
  ↓
更新 Jotai atoms:
  - groupConfigAtom
  - statConfigAtom
```

### 3. 计算阶段
```
用户点击 "开始分组"
  ↓
Backend: compute_grouping(dataset, groupConfig, statConfig)
  ↓
算法流程:
  1. 生成候选分组 (枚举 or Monte Carlo)
  2. 并行评估每个候选 (rayon)
     - 对每个指标计算 P 值
     - Levene 检验 → 选择 t-test/ANOVA 方法
  3. 按优化模式过滤并排序
  4. 返回最佳分组
  ↓
返回 GroupingResult { assignments, statistics, summary }
  ↓
Frontend: 更新 resultAtom，显示结果
```

### 4. 导出阶段
```
用户点击 "导出"
  ↓
Backend: export_result(result, format, output_path)
  ↓
生成 Excel 文件:
  - Sheet 1: 分组结果表 (组别 | 动物编号 | 性别 | 指标...)
  - Sheet 2: 统计结果表 (指标 | P值 | 检验方法 | 达标状态)
  - Sheet 3: 汇总信息
  ↓
保存到用户选择的路径
```

---

## 🔬 核心算法

### 分组生成策略

| 数据规模 | 算法 | 候选数量 | 预估耗时 |
|---------|------|---------|---------|
| < 20 动物 | 完全枚举 | C(n, k) | < 1s |
| 20-50 动物 | Monte Carlo | 10,000 | 1-5s |
| > 50 动物 | Monte Carlo + 启发式剪枝 | 100,000 | 5-30s |

### 统计检验流程

```rust
for each indicator:
    // Step 1: Variance homogeneity test
    p_levene = levene_test(groups)

    // Step 2: Select appropriate test
    if num_groups == 2:
        if p_levene > α:
            p = student_ttest(group1, group2)
        else:
            p = welch_ttest(group1, group2)
    else:  // >= 3 groups
        if p_levene > α:
            p = anova_oneway(groups)  // + Tukey HSD
        else:
            p = welch_anova(groups)   // + Dunnett's T3

    // Step 3: Store result
    results.push(IndicatorStats { p, method, is_valid: p > α })
```

### 评分函数

**严格模式：**
```rust
valid_candidates = candidates.filter(|c| c.num_bad_p == 0)
best = valid_candidates.max_by(|c| c.min_p_value)
```

**优化模式：**
```rust
valid_candidates = candidates.filter(|c| c.num_bad_p <= 1)
best = valid_candidates.max_by(|c| {
    // Primary: max(min(P))
    c.min_p_value.cmp(&other.min_p_value)
        // Secondary: max(mean(P))
        .then_with(|| c.mean_p_value.cmp(&other.mean_p_value))
})
```

---

## 📁 项目结构

```
AutoGroup/
├── docs/                           # 文档目录（当前）
│   ├── README.md                   # 总览（本文件）
│   ├── technical_specification.md
│   ├── implementation_design.md
│   ├── data_format_spec.md
│   └── output_format_spec.md
├── src/                            # React 前端源码
│   ├── components/
│   │   ├── ui/                     # shadcn/ui 组件
│   │   ├── data-import/
│   │   ├── configuration/
│   │   ├── results/
│   │   └── history/
│   ├── hooks/
│   ├── store/                      # Jotai atoms
│   ├── lib/
│   │   └── tauri.ts                # Tauri 命令封装
│   └── types/
│       └── index.ts                # TypeScript 类型定义
├── src-tauri/                      # Rust 后端源码
│   ├── src/
│   │   ├── commands/               # Tauri 命令处理
│   │   ├── core/
│   │   │   ├── models.rs
│   │   │   ├── parser.rs
│   │   │   ├── grouping/
│   │   │   └── stats/
│   │   ├── persistence/
│   │   └── lib.rs
│   └── Cargo.toml
├── package.json
└── README.md
```

---

## 🧪 测试数据

**文件:** `通用动物实验自动分组软件_测试用数据.xlsx`

**Sheet 1: 原始数据**
- 10 只动物（6 雄性，4 雌性）
- 73 个指标
- 格式：多行表头 + 数据行

**Sheet 2: 动物分组（期望输出格式）**
- 列结构：组别 | 动物编号 | 性别 | 指标...
- 性别格式：雌性 / 雄性（中文）
- 按组别、性别、动物编号排序

**测试场景:**
```
输入: 10 只动物，73 个指标
配置: 2 组，每组 5 只，3雄+2雌
统计: 所有指标参与，α=0.05，严格模式
预期: 找到所有 P > 0.05 的分组方案
```

---

## 📅 开发路线图

### Phase 1: 核心基础（Week 1-2）
- [x] 项目架构设计
- [x] 技术选型确认
- [ ] shadcn/ui 组件安装与配置
- [ ] Rust 依赖项添加
- [ ] 数据模型定义（Rust + TypeScript）
- [ ] Excel 解析器实现
- [ ] 基础 UI 框架搭建

### Phase 2: 统计引擎（Week 3-4）
- [ ] Levene 方差齐性检验
- [ ] Student & Welch t 检验
- [ ] One-way ANOVA
- [ ] Tukey HSD 事后检验
- [ ] Welch ANOVA + Dunnett's T3
- [ ] 单元测试（与 scipy 对比验证）

### Phase 3: 分组算法（Week 5）
- [ ] 完全枚举算法（小规模）
- [ ] Monte Carlo 采样（中大规模）
- [ ] 分组评估器
- [ ] 并行计算优化（rayon）
- [ ] 进度报告机制

### Phase 4: 前端界面（Week 6-7）
- [ ] 文件导入组件 + 数据预览
- [ ] 分组配置面板
- [ ] 统计配置面板
- [ ] 计算进度指示器
- [ ] 结果展示（TanStack Table）
- [ ] P 值分布图（ECharts）
- [ ] 导出功能

### Phase 5: 持久化（Week 8）
- [ ] SQLite 数据库设计
- [ ] 配置模板 CRUD
- [ ] 分组历史 CRUD
- [ ] 模板管理界面
- [ ] 历史记录查看器

### Phase 6: 测试与优化（Week 9-10）
- [ ] 端到端测试（使用真实测试数据）
- [ ] 性能优化（大规模数据场景）
- [ ] 错误处理完善
- [ ] UI/UX 改进
- [ ] 用户手册编写
- [ ] Tauri 打包配置

---

## 🚀 即刻启动

### 环境要求
- Node.js >= 18
- Rust >= 1.75
- Bun (已安装)
- VS Code + Tauri 插件

### 快速开始

**1. 安装前端依赖**
```bash
bun install
bun add jotai @tanstack/react-table echarts
npx shadcn@latest init
```

**2. 安装 shadcn/ui 组件**
```bash
npx shadcn@latest add button input table dialog tabs card
```

**3. 添加 Rust 依赖**
编辑 `src-tauri/Cargo.toml`，添加：
```toml
calamine = "0.27"
statrs = "0.18"
rusqlite = { version = "0.32", features = ["bundled"] }
rayon = "1.10"
rust_xlsxwriter = "0.83"
anyhow = "1.0"
chrono = { version = "0.4", features = ["serde"] }
```

**4. 运行开发服务器**
```bash
bun run tauri dev
```

---

## ❓ 待确认决策

在开始实施前，请确认以下问题：

### 1. 输出格式
- ✅ 必须包含 `组别` 列（第一列）
- ✅ 性别必须转换为中文（雌性/雄性）
- ❓ 导出所有原始指标，还是仅导出参与统计的指标？

### 2. 多 Sheet 输出
- ❓ 需要 3 个 sheet（分组结果 + 统计结果 + 汇总），还是仅 1 个 sheet？

### 3. 实施优先级
- ❓ 先实现核心算法（能跑通），再完善 UI？
- ❓ 还是先完成标准格式导出，再优化算法性能？

### 4. 性能目标
- ❓ 预期最大动物数量？（决定算法选择）
  - 50 以内：可以用完全枚举
  - 100+：必须用 Monte Carlo + 启发式剪枝

---

## 📞 联系与支持

**文档维护者：** Claude (AI Assistant)
**项目发起人：** Kun
**创建日期：** 2026-02-12

如有疑问或需要调整方案，请随时反馈！

---

## 📝 版本历史

- **v1.0** (2026-02-12): 初始版本，完成整体架构设计和技术选型
