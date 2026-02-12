# AutoGroup 技术方案设计文档

## 一、需求理解与整理

### 1.1 核心目标
开发一款通用动物实验自动分组软件，根据术前多指标数据实现统计学平衡的自动分组，适用于科研及注册前动物实验场景。

### 1.2 关键功能需求

#### 数据输入
- 支持 Excel (.xlsx) 文件导入
- 数据格式：`AnimalID | Sex | Indicator1 | Indicator2 | ... | IndicatorN`
- 约束：
  - AnimalID 为唯一标识
  - Sex 为分类变量（初始为 M/F，可扩展）
  - 其余列为连续型数值指标

#### 分组配置
- **分组数量**：≥2 组
- **每组动物数**：统一设定或逐组指定
- **性别组成**：每组性别比例（如 1F+2M）

#### 统计配置
- **指标选择**：勾选参与统计平衡计算的指标
- **显著性水平**：α 值（默认 0.05）
- **统计方法自动选择**：
  - 2组：独立样本 t 检验
    - Levene 方差齐性检验 → 齐性用 Student t-test，不齐用 Welch t-test
  - ≥3组：单因素方差分析
    - Levene 方差齐性检验 → 齐性用 One-way ANOVA + Tukey HSD，不齐用 Welch ANOVA + Dunnett's T3

#### 优化策略
- **严格模式**：所有选中指标的 P > α
- **优化模式**：允许最多 1 个指标 P ≤ α
  - 主评分：max(min(P)) — 最大化最小 P 值
  - 次级评分：max(mean(P)) — 当主评分相同时，最大化平均 P 值

#### 输出结果
- 分组结果表：`AnimalID | Sex | Group`
- 各指标统计结果（P 值表）
- 汇总信息：最小 P、平均 P、达标状态
- 支持导出 Excel / CSV

#### 扩展需求（加分项）
- 多分类变量支持
- 指标权重配置
- 项目配置模板保存/加载
- 分组历史记录
- P 值分布可视化图表

---

## 二、技术栈选择与理由

### 2.1 当前项目状态
- 框架：**Tauri 2.x** + **React 19** + **TypeScript**
- 后端：**Rust** (src-tauri)
- 前端：**React + Vite**
- 包管理：**bun**（从 bun.lock 判断）

### 2.2 技术栈决策

#### 前端技术栈
- **UI 框架**：React 19 + TypeScript（已确定）
- **UI 组件库**：需决策（见"决策点 1"）
- **表格组件**：需决策（见"决策点 2"）
- **图表库**：需决策（见"决策点 3"）
- **状态管理**：需决策（见"决策点 4"）

#### 后端技术栈（Rust）
- **Excel 解析**：`calamine` crate（纯 Rust，高性能）
- **数据结构**：`polars` 或 `ndarray` + 自定义结构（需决策，见"决策点 5"）
- **统计计算**：需决策（见"决策点 6"）
  - 方案 A：调用 Python scipy（通过 PyO3 或子进程）
  - 方案 B：纯 Rust 统计库（statrs + welch-ttest 等）
  - 方案 C：外接 R 脚本
- **分组算法**：纯 Rust 实现
  - 小规模（< 20 动物）：组合枚举
  - 中大规模：Monte Carlo 采样
  - 优化策略：迭代过滤 + 评分排序
- **持久化**：
  - 配置模板：JSON（serde_json）
  - 历史记录：SQLite（rusqlite）或 JSON 文件

---

## 三、架构设计

### 3.1 整体架构

```
┌─────────────────────────────────────────────┐
│              Frontend (React)                │
│  ┌─────────────┐  ┌──────────────────────┐  │
│  │  UI 界面    │  │  状态管理 & 数据流   │  │
│  │  - 数据导入 │  │  - 分组配置         │  │
│  │  - 参数配置 │  │  - 计算状态         │  │
│  │  - 结果展示 │  │  - 结果缓存         │  │
│  │  - 导出功能 │  │                      │  │
│  └─────────────┘  └──────────────────────┘  │
│           ↕ Tauri IPC (invoke commands)      │
└─────────────────────────────────────────────┘
                      ↕
┌─────────────────────────────────────────────┐
│           Backend (Rust / Tauri)             │
│  ┌────────────────────────────────────────┐ │
│  │  Tauri Commands (API Layer)            │ │
│  └────────────────────────────────────────┘ │
│  ┌────────────┬───────────────┬──────────┐  │
│  │ Data Layer │ Algorithm Eng │ Export   │  │
│  │ - Excel IO │ - Enumeration │ - Excel  │  │
│  │ - Validate │ - Monte Carlo │ - CSV    │  │
│  │            │ - Statistics  │          │  │
│  └────────────┴───────────────┴──────────┘  │
│  ┌────────────────────────────────────────┐ │
│  │  Persistence Layer                     │ │
│  │  - Config Templates (JSON)             │ │
│  │  - History (SQLite / JSON)             │ │
│  └────────────────────────────────────────┘ │
└─────────────────────────────────────────────┘
```

### 3.2 模块划分

#### Frontend Modules
```typescript
src/
├── components/
│   ├── DataImport/         # 数据导入组件
│   ├── GroupConfig/        # 分组配置组件
│   ├── StatConfig/         # 统计配置组件
│   ├── ResultDisplay/      # 结果展示组件
│   └── ExportPanel/        # 导出面板组件
├── hooks/                  # 自定义 Hooks
├── types/                  # TypeScript 类型定义
├── utils/                  # 工具函数
└── store/                  # 状态管理（如使用 Zustand）
```

#### Backend Modules (Rust)
```
src-tauri/src/
├── commands/               # Tauri 命令接口
│   ├── data_import.rs
│   ├── grouping.rs
│   ├── statistics.rs
│   └── export.rs
├── core/
│   ├── models.rs           # 数据模型
│   ├── parser.rs           # Excel 解析
│   ├── validator.rs        # 数据验证
│   ├── grouping_algo.rs    # 分组算法引擎
│   └── stats_engine.rs     # 统计计算引擎
├── persistence/
│   ├── config.rs           # 配置模板管理
│   └── history.rs          # 历史记录管理
└── lib.rs
```

---

## 四、核心算法设计

### 4.1 分组算法流程

```rust
// Pseudo-code
fn generate_optimal_grouping(
    animals: Vec<Animal>,
    config: GroupConfig,
    stat_config: StatConfig,
) -> Result<GroupingResult> {
    // Step 1: 数据验证
    validate_animals(&animals, &config)?;

    // Step 2: 根据性别约束预分组
    let sex_groups = partition_by_sex(&animals, &config.sex_constraints);

    // Step 3: 生成候选分组方案
    let candidates = match animals.len() {
        n if n <= 20 => enumerate_all_groupings(sex_groups, &config),
        _ => monte_carlo_sampling(sex_groups, &config, SAMPLE_SIZE),
    };

    // Step 4: 评估每个候选方案
    let evaluated = candidates
        .par_iter()  // 并行计算
        .map(|candidate| {
            let p_values = compute_all_p_values(candidate, &stat_config);
            let score = evaluate_grouping(&p_values, stat_config.alpha, stat_config.mode);
            (candidate, p_values, score)
        })
        .collect::<Vec<_>>();

    // Step 5: 根据优化模式过滤和排序
    let best = match stat_config.mode {
        Mode::Strict => {
            evaluated.iter()
                .filter(|(_, p_vals, _)| all_p_greater_than(p_vals, stat_config.alpha))
                .max_by(|a, b| a.2.cmp(&b.2))
        },
        Mode::Optimized => {
            evaluated.iter()
                .filter(|(_, p_vals, _)| count_bad_p(p_vals, stat_config.alpha) <= 1)
                .max_by(|a, b| {
                    // Primary: max(min(P))
                    // Secondary: max(mean(P))
                    a.2.cmp(&b.2)
                })
        }
    };

    // Step 6: 返回结果
    best.ok_or(Error::NoValidGrouping)
        .map(|(grouping, p_values, score)| GroupingResult { ... })
}
```

### 4.2 统计计算接口设计

```rust
trait StatisticalTest {
    fn test(&self, groups: &[Vec<f64>]) -> Result<PValue>;
}

// 2 groups: t-test
struct TTest {
    use_welch: bool,  // Auto-determined by Levene test
}

// >=3 groups: ANOVA
struct OneWayANOVA {
    post_hoc: PostHocMethod,  // Tukey or Dunnett's T3
}

fn select_test(groups: &[Vec<f64>], alpha: f64) -> Box<dyn StatisticalTest> {
    match groups.len() {
        2 => {
            let homogeneous = levene_test(groups, alpha);
            Box::new(TTest { use_welch: !homogeneous })
        },
        _ => {
            let homogeneous = levene_test(groups, alpha);
            let post_hoc = if homogeneous {
                PostHocMethod::Tukey
            } else {
                PostHocMethod::DunnettT3
            };
            Box::new(OneWayANOVA { post_hoc })
        }
    }
}
```

### 4.3 评分函数

```rust
#[derive(Debug)]
struct GroupingScore {
    min_p: f64,
    mean_p: f64,
}

impl Ord for GroupingScore {
    fn cmp(&self, other: &Self) -> Ordering {
        // Primary: max(min(P))
        match self.min_p.partial_cmp(&other.min_p) {
            Some(Ordering::Equal) => {
                // Secondary: max(mean(P))
                self.mean_p.partial_cmp(&other.mean_p).unwrap()
            },
            other => other.unwrap(),
        }
    }
}
```

---

## 五、数据流设计

### 5.1 导入流程
```
User uploads Excel
    ↓
Frontend reads file as ArrayBuffer
    ↓
invoke("parse_excel", { data: ArrayBuffer })
    ↓
Backend: calamine parses → validate → AnimalData[]
    ↓
Return: { animals, indicators, summary }
    ↓
Frontend stores in state & displays preview
```

### 5.2 分组计算流程
```
User configures parameters & clicks "Start Grouping"
    ↓
invoke("compute_grouping", { animals, config, statConfig })
    ↓
Backend:
  - Generate candidates
  - Compute statistics (parallel)
  - Evaluate & rank
    ↓
Return: { best_grouping, p_values, score, runtime }
    ↓
Frontend displays results
```

### 5.3 导出流程
```
User clicks "Export"
    ↓
invoke("export_result", { grouping, format })
    ↓
Backend: Generate Excel/CSV file
    ↓
Return: file_path
    ↓
Frontend triggers download via Tauri dialog
```

---

## 六、性能优化策略

### 6.1 算法层面
- **组合爆炸处理**：
  - 小规模（< 20）：完全枚举
  - 中规模（20-50）：Monte Carlo 采样（10,000 - 100,000 次）
  - 大规模（> 50）：启发式剪枝 + 遗传算法（可选）

- **并行计算**：
  - 使用 `rayon` 并行评估候选方案
  - 统计计算独立，天然适合并行化

### 6.2 数据结构优化
- 使用 `Vec<Animal>` + 索引映射，避免克隆
- 分组方案用索引数组表示：`Vec<Vec<usize>>`

### 6.3 缓存策略
- Frontend 缓存导入数据，避免重复解析
- Backend 可选缓存 Levene 检验结果（方差齐性状态）

---

## 七、需要决策的关键技术点

### 决策点 1：UI 组件库选择
**选项：**
- A. **Ant Design** (antd)
  - 优点：组件丰富、文档完善、社区活跃、中文友好
  - 缺点：包体积较大
- B. **Material-UI (MUI)**
  - 优点：设计规范、国际化、生态成熟
  - 缺点：定制成本高、包体积大
- C. **Radix UI + Tailwind CSS**
  - 优点：轻量、可定制、现代化
  - 缺点：需要手动组装、开发速度慢
- D. **shadcn/ui**
  - 优点：基于 Radix + Tailwind、可复制组件代码、灵活
  - 缺点：需要逐个安装组件

**建议：** Ant Design（快速开发、组件齐全、适合科研工具场景）

---

### 决策点 2：表格组件选择
**选项：**
- A. **Ant Design Table**（如果选择 antd）
- B. **TanStack Table (React Table v8)**
  - 优点：轻量、headless、高性能、支持虚拟滚动
  - 缺点：需要自行实现 UI
- C. **AG Grid**
  - 优点：功能最强大（排序、筛选、编辑）
  - 缺点：免费版功能受限、复杂

**建议：** TanStack Table（数据量可能较大，需要虚拟滚动优化）

---

### 决策点 3：图表库选择
**用途：** P 值分布可视化、分组结果对比

**选项：**
- A. **ECharts**
  - 优点：功能强大、中文文档、适合科研可视化
  - 缺点：包体积较大
- B. **Recharts**
  - 优点：React 原生、简洁、易用
  - 缺点：定制能力有限
- C. **D3.js**
  - 优点：最灵活、功能最强
  - 缺点：学习曲线陡峭

**建议：** ECharts（科研场景、需要专业统计图表）

---

### 决策点 4：状态管理方案
**选项：**
- A. **无状态管理**（仅用 React useState/useContext）
  - 适用场景：应用简单、状态局部
- B. **Zustand**
  - 优点：轻量（~1KB）、API 简洁、TypeScript 友好
  - 缺点：生态较小
- C. **Redux Toolkit**
  - 优点：成熟、DevTools 强大、生态丰富
  - 缺点：样板代码多、学习成本高

**建议：** Zustand（轻量且足够、适合中小型应用）

---

### 决策点 5：后端数据处理库
**用途：** 存储和操作动物数据、指标矩阵

**选项：**
- A. **自定义结构**（`Vec<Animal>` + HashMap）
  - 优点：简单、灵活、无额外依赖
  - 缺点：手动处理数据操作
- B. **polars-rs**
  - 优点：类似 pandas、高性能、适合数据分析
  - 缺点：学习曲线、包体积大
- C. **ndarray**
  - 优点：科学计算标准库、轻量
  - 缺点：需要手动管理列名、索引

**建议：** 自定义结构（数据规模不大、操作简单）

---

### 决策点 6：统计计算方案（最关键）
**选项：**

#### A. **调用 Python scipy**
- 实现方式：
  - 通过 `PyO3` 嵌入 Python 解释器
  - 或通过子进程调用 Python 脚本
- 优点：
  - scipy 统计功能完善、经过验证
  - 实现快速（直接调用现成函数）
- 缺点：
  - 需要用户安装 Python 环境（打包复杂）
  - 跨语言调用性能损耗
  - 依赖管理复杂

#### B. **纯 Rust 统计库**
- 相关 crate：
  - `statrs`：基础统计函数、分布
  - `welch-ttest`：Welch t 检验
  - 需自行实现：Levene 检验、ANOVA、Tukey HSD、Dunnett's T3
- 优点：
  - 无外部依赖、打包简单
  - 性能最佳（编译时优化）
  - 完全可控
- 缺点：
  - 需要自行实现部分高级统计方法
  - 开发成本高
  - 需要验证正确性（与 scipy 对比）

#### C. **调用 R 脚本**
- 实现方式：子进程调用 Rscript
- 优点：R 统计功能最专业
- 缺点：需要用户安装 R 环境、依赖管理更复杂

**推荐：** 方案 B（纯 Rust）
- 理由：
  1. Tauri 应用强调单文件分发、无外部依赖
  2. 统计计算性能敏感（候选方案可能达数万次）
  3. Rust 生态已有基础统计库，缺失部分可自行实现
  4. 长期维护成本低

**实施策略：**
- 阶段 1：使用 `statrs` + `welch-ttest` 实现 t 检验
- 阶段 2：自行实现 Levene 检验（算法简单）
- 阶段 3：实现 One-way ANOVA + Tukey HSD
- 阶段 4：实现 Welch ANOVA + Dunnett's T3（可选延后）
- **验证方式**：与 scipy 结果对比（单元测试）

---

### 决策点 7：持久化方案
**用途：** 配置模板、历史记录

**选项：**
- A. **JSON 文件**
  - 优点：简单、人类可读、易调试
  - 缺点：查询效率低、不适合大量历史记录
- B. **SQLite**
  - 优点：查询高效、支持索引、适合历史记录
  - 缺点：需要 schema 设计、复杂度增加
- C. **混合方案**
  - 配置模板用 JSON
  - 历史记录用 SQLite

**建议：** 混合方案（兼顾简单性和查询效率）

---

### 决策点 8：Excel 导出方案
**选项：**
- A. **rust_xlsxwriter**
  - 优点：纯 Rust、功能全面、支持格式化
  - 缺点：API 较复杂
- B. **umya-spreadsheet**
  - 优点：支持读写、功能丰富
  - 缺点：文档较少
- C. **simple_excel_writer**
  - 优点：API 简洁
  - 缺点：功能有限

**建议：** rust_xlsxwriter（功能完善、维护活跃）

---

## 八、开发阶段规划

### Phase 1: MVP 核心功能（2-3 周）
1. **数据导入与验证**
   - Excel 解析（calamine）
   - 数据验证逻辑
   - 前端数据预览界面
2. **分组配置界面**
   - 分组数、每组动物数配置
   - 性别比例配置
3. **统计配置界面**
   - 指标勾选
   - α 值设定
   - 优化模式选择
4. **核心分组算法**
   - 枚举算法（小规模）
   - 基础统计计算（t-test、Levene）
5. **结果展示**
   - 分组结果表格
   - P 值表格
6. **结果导出**
   - Excel 导出

### Phase 2: 优化与扩展（2 周）
1. **算法优化**
   - Monte Carlo 采样（中大规模）
   - 并行计算优化（rayon）
2. **统计方法完善**
   - One-way ANOVA
   - Tukey HSD
   - Welch ANOVA + Dunnett's T3
3. **UI/UX 改进**
   - 进度条显示
   - 错误提示优化
   - 响应式布局

### Phase 3: 高级功能（2 周）
1. **配置模板系统**
   - 模板保存/加载
   - 模板管理界面
2. **历史记录**
   - SQLite 集成
   - 历史查询界面
3. **数据可视化**
   - P 值分布图（ECharts）
   - 分组对比图
4. **多分类变量支持**
   - 扩展数据模型
   - UI 适配

### Phase 4: 测试与优化（1 周）
1. **单元测试**
   - 统计函数正确性验证（与 scipy 对比）
   - 分组算法边界测试
2. **性能测试**
   - 大规模数据场景（100+ 动物）
   - 性能瓶颈分析
3. **打包与分发**
   - Tauri 打包配置
   - Windows/macOS 测试

---

## 九、技术风险与缓解

### 风险 1：统计计算正确性
- **风险**：自行实现的统计方法可能与 scipy 结果不一致
- **缓解**：
  - 开发阶段对比验证（编写测试用例）
  - 参考权威算法文献（如 R/scipy 源码）
  - 可选保留 Python 调用作为 fallback

### 风险 2：大规模数据性能
- **风险**：动物数量过多导致组合爆炸
- **缓解**：
  - 实现多级算法策略（枚举 → Monte Carlo → 启发式）
  - 设置合理的采样上限
  - 提供"中断计算"功能

### 风险 3：跨平台兼容性
- **风险**：Tauri 在不同平台表现可能不一致
- **缓解**：
  - 早期在 Windows/macOS/Linux 测试
  - 使用 Tauri 官方插件（避免自定义原生调用）

---

## 十、待讨论的技术决策清单

请确认以下决策点的选择，以便我继续细化方案：

### 必须决策
1. **UI 组件库**：推荐 Ant Design，是否同意？
2. **统计计算方案**：推荐纯 Rust 实现（方案 B），是否同意？
   - 如选择 Python（方案 A），是否接受用户需安装 Python？
3. **表格组件**：推荐 TanStack Table（支持虚拟滚动），是否同意？
4. **图表库**：推荐 ECharts，是否同意？

### 可选决策
5. **状态管理**：推荐 Zustand，或者直接用 React Context？
6. **持久化方案**：推荐混合方案（JSON + SQLite），或者仅用 JSON？
7. **数据处理**：推荐自定义结构，或者使用 polars？

### 需求澄清
8. **多分类变量支持**：是否在 MVP 阶段实现？（建议 Phase 3）
9. **指标权重功能**：具体需求是什么？（如何影响评分函数？）
10. **单机应用 vs Web 应用**：确认是 Tauri 桌面应用？

---

## 十一、下一步行动

确认上述决策后，我将：
1. 细化 Rust 模块接口设计（数据模型、Tauri Commands）
2. 输出前端组件结构设计（含 TypeScript 类型定义）
3. 编写统计算法伪代码与验证测试计划
4. 提供初始项目脚手架代码

请回复您对上述决策点的选择，以及任何补充需求。
