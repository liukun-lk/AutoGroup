# AutoGroup 项目实施进度报告

> 生成时间: 2026-02-12
> 状态: Phase 1 进行中

---

## ✅ 已完成的工作

### 1. 项目结构搭建 ✓
- [x] 创建完整的模块目录结构
- [x] 添加所有 Rust 依赖到 Cargo.toml
- [x] 配置 Tauri 插件（dialog, opener）

### 2. 核心数据模型 ✓
**文件:** `src-tauri/src/core/models.rs`

实现的结构体：
- `Animal`, `Sex`, `Dataset`, `DatasetMetadata`
- `GroupConfig`, `GroupSize`, `SexConstraint`
- `StatConfig`, `OptimizationMode`
- `GroupingResult`, `GroupAssignment`, `IndicatorStats`, `ResultSummary`
- `CandidateGrouping`（内部使用）

**特性：**
- 完整的 Serialize/Deserialize 支持
- Sex 枚举支持中英文转换（`to_chinese()` 方法）
- 类型定义与前端 TypeScript 完全对应

### 3. Excel 解析器 ✓
**文件:** `src-tauri/src/core/parser.rs`

**功能：**
- 解析测试数据格式（多行表头）
- Row 1: 英文指标名 / Row 2: 中文名+单位
- 自动识别数据起始行（跳过单位行）
- 提取 AnimalID, Sex, 指标值
- 处理空值和缺失数据

**文件:** `src-tauri/src/core/validator.rs`

**验证规则：**
- 最少 4 只动物
- AnimalID 唯一性检查
- 性别有效性检查
- 数据完整性检查（每只动物至少50%指标有值）

### 4. Tauri Commands ✓
**文件:** `src-tauri/src/commands/import.rs`

- `parse_excel(file_path)` - 解析 Excel 并返回 Dataset
- 集成了 parser + validator
- 错误处理：转换为用户友好的错误消息

**文件:** `src-tauri/src/commands/grouping.rs`

- `compute_grouping(dataset, config, stat_config)` - 占位实现

**文件:** `src-tauri/src/commands/export.rs`

- `export_result(result, dataset, format, path)` - 占位实现

---

## 🚧 进行中的工作

### 任务状态

| 任务ID | 任务名称 | 状态 | 优先级 |
|--------|---------|------|-------|
| #1 | Setup Rust dependencies and project structure | ✅ 完成 | 高 |
| #2 | Implement core data models (Rust) | ✅ 完成 | 高 |
| #3 | Implement Excel parser | ✅ 完成 | 高 |
| #4 | Implement Levene test | 📝 待实施 | 高 |
| #5 | Implement t-tests (Student & Welch) | 📝 待实施 | 高 |
| #6 | Implement ANOVA and post-hoc tests | 📝 待实施 | 中 |
| #7 | Implement grouping enumeration algorithm | 📝 待实施 | 高 |
| #8 | Implement grouping evaluator | 📝 待实施 | 高 |
| #9 | Setup frontend TypeScript types and Jotai store | 📝 待实施 | 中 |
| #10 | Setup shadcn/ui and create basic UI components | 📝 待实施 | 中 |
| #11 | Implement Excel export with 3 sheets | 📝 待实施 | 低 |

---

## 📝 待创建的文件清单

### 统计引擎模块（核心优先）

```
src-tauri/src/core/stats/
├── levene.rs      # Levene 方差齐性检验
├── ttest.rs       # Student & Welch t检验
├── anova.rs       # One-way ANOVA & Welch ANOVA
├── tukey.rs       # Tukey HSD 事后检验
└── dunnett.rs     # Dunnett's T3 事后检验
```

### 分组算法模块

```
src-tauri/src/core/grouping/
├── enumerator.rs  # 完全枚举算法（≤50动物）
└── evaluator.rs   # 分组评估器（计算P值，评分排序）
```

### 持久化模块（后期）

```
src-tauri/src/persistence/
├── db.rs          # SQLite 连接管理
├── config_repo.rs # 配置模板 CRUD
└── history_repo.rs # 历史记录 CRUD
```

### 工具模块

```
src-tauri/src/utils/
└── error.rs       # 统一错误类型定义
```

---

## 🎯 下一步行动计划

### 优先级 1：核心统计引擎（Week 3-4）

**顺序：** Levene → t-test → ANOVA → Tukey → Dunnett's T3

**原因：** 这是算法的核心依赖，必须先实现且验证正确性

**验证策略：**
- 每个统计函数编写单元测试
- 与 Python scipy 结果对比（准备测试数据集）
- 误差容忍度：< 1e-6

### 优先级 2：分组算法（Week 5）

**依赖：** 统计引擎完成后

**实施顺序：**
1. `enumerator.rs` - 生成所有候选分组
2. `evaluator.rs` - 评估每个候选（调用统计引擎）
3. `grouping/mod.rs` - 组装完整流程

### 优先级 3：前端基础（Week 6）

**可并行进行**

1. 安装 shadcn/ui
2. 创建 TypeScript 类型定义
3. 设置 Jotai store
4. 实现文件上传组件 + 数据预览

### 优先级 4：Excel 导出（Week 7）

**依赖：** 分组算法完成后

3个 sheet 的导出逻辑

---

## 🔍 当前技术状态

### 依赖版本（已锁定）

```toml
calamine = "0.26.1"  # (降级，避免 yanked 版本)
statrs = "0.18.0"
rusqlite = "0.32.1"
rayon = "1.11.0"
rust_xlsxwriter = "0.83.0"
tauri = "2.x"
```

### 编译状态

**现状：** ❌ 无法编译（缺少模块文件）

**错误类型：**
- 统计模块文件缺失（levene, ttest, anova, tukey, dunnett）
- 分组模块文件缺失（enumerator, evaluator）
- 持久化模块文件缺失（db, config_repo, history_repo）
- 工具模块文件缺失（error）

**解决方案：** 创建占位实现或注释掉 `mod` 声明

---

## 💡 技术决策确认

以下决策已锁定，将按此实施：

1. **导出列选择：** 选项 A - 导出所有 73 个原始指标
2. **多 Sheet 输出：** 选项 B - 3 个 sheet（分组结果 + 统计分析 + 汇总）
3. **实施优先级：** 方案 A - 先实现核心算法，再完善 UI
4. **性能目标：** ≤ 50 动物，使用完全枚举算法

---

## 📈 下一次会话计划

**建议从以下任务开始：**

### 任务 #4: 实现 Levene 检验
- 创建 `src-tauri/src/core/stats/levene.rs`
- 算法：计算每组的中位数，转换为绝对偏差，运行 ANOVA
- 返回 P 值

### 任务 #5: 实现 t 检验
- 创建 `src-tauri/src/core/stats/ttest.rs`
- Student's t-test（等方差）
- Welch's t-test（不等方差）
- 使用 `statrs` crate 的 t 分布

### 测试准备
- 准备小规模测试数据集（2组×5只动物×3个指标）
- 用 Python scipy 计算期望结果
- 编写 Rust 单元测试对比

---

## 📚 参考文档

- **技术方案总览:** `/docs/README.md`
- **实施设计:** `/docs/implementation_design.md`
- **数据格式规范:** `/docs/data_format_spec.md`
- **输出格式规范:** `/docs/output_format_spec.md`

---

## ⏰ 预估时间线

- **Week 3-4:** 统计引擎实现 + 单元测试 ⏳ **当前阶段**
- **Week 5:** 分组算法实现
- **Week 6:** 前端 UI 基础
- **Week 7:** Excel 导出
- **Week 8:** 持久化与历史记录
- **Week 9-10:** 集成测试与优化

---

**备注：** 所有模块设计已完成，现在主要是逐个实现和测试的过程。建议保持小步快跑的节奏，每完成一个模块立即编写测试验证正确性。
