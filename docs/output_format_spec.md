# 需求澄清与目标输出格式

## 核心需求理解

用户提供的 Excel 文件包含 **2 个 sheet**：

### Sheet 1: `原始数据` (Input)
分组前的各动物指标数据

**格式：**
```
Row 1: [None, None, 'kg', '℃', 'ALT', 'AST', ...]  # 指标英文名
Row 2: ['动物编号', '性别', '体重', '肛温', 'U/L', ...]  # 中文名 + 单位
Row 3+: ['XHP2601001', 'F', 31.85, 38.5, 58.8, ...]  # 实际数据
```

- 10 只动物（6 雄 4 雌）
- 73 个指标
- 性别标记：`F` (雌性), `M` (雄性)

### Sheet 2: `动物分组` (Expected Output Format)
分组后的结果样式

**格式：**
```
Row 1: [None, None, None, 'kg', '℃', '(10^9/L)', ...]  # 单位
Row 2: ['组别', '动物编号', '性别', '体重', '肛温', 'WBC', ...]  # 列标题
Row 3: [1, 'XHP2601001', '雌性', 31.85, 38.5, 14.08, ...]  # 组1动物1
Row 4: [1, 'XHP2601004', '雄性', 30.3, 38.1, 13.33, ...]  # 组1动物2
Row 5: [1, 'XHP2601008', '雄性', 30.6, 38.3, 15.25, ...]  # 组1动物3
...
Row N: [2, 'XHP2601002', '雌性', ...]  # 组2动物1
```

**关键差异：**
1. 新增 `组别` 列（第1列）：数字 1, 2, 3...
2. `性别` 列值改为中文：`雌性` / `雄性`（原始数据中为 `F` / `M`）
3. 可能包含不同的指标列（`动物分组` sheet 中显示 WBC, RBC, HGB 等，与原始数据的 ALT, AST 等不完全相同）

---

## 软件目标

**核心功能：**
> 将 `原始数据` sheet 快速转换为 `动物分组` sheet 格式

**具体流程：**
1. 读取 `原始数据` sheet 的动物列表和指标
2. 根据用户配置（分组数、每组动物数、性别比例、统计参数）自动计算最优分组
3. 输出分组结果，格式与 `动物分组` sheet 一致
4. 支持导出为新的 Excel 文件（包含统计分析结果）

---

## 输出格式详细规范

### 3.1 必需列

| 列名 | 数据类型 | 说明 | 示例 |
|------|---------|------|------|
| 组别 | Integer | 分组编号（1-based） | 1, 2, 3 |
| 动物编号 | String | 唯一标识符 | XHP2601001 |
| 性别 | String | 中文性别（雌性/雄性） | 雌性, 雄性 |

### 3.2 指标列

**用户可选择：**
- 选项 A：导出所有原始指标（kg, ℃, ALT, AST, ...）
- 选项 B：仅导出用户勾选的参与统计的指标
- 选项 C：导出常用指标子集（用户可预定义模板）

**列顺序：**
```
组别 | 动物编号 | 性别 | [选定的指标列...]
```

### 3.3 行顺序

**建议排序规则：**
1. 先按组别排序（1 → 2 → 3 → ...）
2. 组内按性别排序（雌性在前，雄性在后）
3. 同性别内按动物编号排序

示例：
```
组别 | 动物编号    | 性别
-----|------------|------
1    | XHP2601001 | 雌性
1    | XHP2601003 | 雌性
1    | XHP2601004 | 雄性
1    | XHP2601005 | 雄性
1    | XHP2601006 | 雄性
2    | XHP2601002 | 雌性
2    | XHP2601007 | 雌性
2    | XHP2601008 | 雄性
2    | XHP2601009 | 雄性
2    | XHP2601010 | 雄性
```

---

## 输出文件结构

### 方案 A：单 Sheet 输出（推荐）

**Sheet: `分组结果`**
- 包含上述格式的分组表格
- 适合直接复制到注册文档

### 方案 B：多 Sheet 输出（完整版）

**Sheet 1: `分组结果`**
- 同方案 A

**Sheet 2: `统计结果`**

每个指标一行。Levene 只决定走哪条检验分支，不是达标条件本身。

| 指标名称 | Levene P 值 | 差异检验 P 值 | 检验方法 | 是否达标 |
|---------|------------|--------------|---------|---------|
| kg | 0.812 | 0.523 | Student t-test | ✓ |
| ALT | 0.031 | 0.087 | Welch t-test | ✓ |
| AST | 0.024 | 0.042 | Welch t-test | ✗ |
| ... | ... | ... | ... | ... |

**Sheet 3: `事后比较`**（仅 ≥3 组时生成）

每行一个「指标 × 组对」。整体 ANOVA 只说明各组之间是否存在差异，评审还要看任意两组之间
都无差异——这也正是达标规则的要求（主检验 P > α **且** 每一对事后比较 P > α）。
两组设计没有事后阶段，该 sheet 不会出现。

| 指标名称 | 比较对 | 检验方法 | 事后比较 P 值 | 是否达标 |
|---------|-------|---------|--------------|---------|
| kg | 组1 vs. 组2 | One-way ANOVA + Tukey HSD | 0.2864 | ✓ |
| kg | 组1 vs. 组3 | One-way ANOVA + Tukey HSD | 0.4963 | ✓ |
| kg | 组2 vs. 组3 | One-way ANOVA + Tukey HSD | 0.8816 | ✓ |
| ... | ... | ... | ... | ... |

组名取自分组配置里的 `custom_name`，未设置时回落为 `组N`，与 `分组结果` sheet 一致。

**Sheet 4: `汇总信息`**
```
分组配置
- 分组数量: 2
- 每组动物数: 5
- 性别约束: 3雄 + 2雌

统计配置
- 显著性水平 α: 0.05
- 优化模式: 严格模式
- 参与统计指标: 73 个

结果摘要
- 最小 P 值: 0.042
- 平均 P 值: 0.315
- 不达标指标数: 1
- 计算耗时: 125 ms
```

---

## 导入流程优化

### 当前问题
原始数据 sheet 的指标列名不统一：
- Row 1: 英文缩写（如 `ALT`, `AST`）
- Row 2: 中文全称 + 单位（如 `U/L`, `g/L`）

### 解决方案

**指标名称展示：**
```
界面中显示组合格式：
- "ALT (U/L)" — 英文名 + 单位
- "AST (U/L)"
- "体重 (kg)" — 如果没有英文名，使用中文

内部存储：
- 使用 Row 1 的英文名作为主键（唯一标识）
- 如果 Row 1 为空，则使用 Row 2 的中文名
```

**用户界面：**
```
[ ] ALT (U/L)          ✓ 已选择
[ ] AST (U/L)          ✓ 已选择
[ ] TP (g/L)           ✓ 已选择
[ ] 体重 (kg)          ✓ 已选择
...

[全选] [取消全选] [选择常用20项]
```

---

## 性别字段转换规则

**输入 → 输出映射：**

| 原始数据 (Sex) | 输出格式 (性别) |
|---------------|---------------|
| F, f, Female  | 雌性          |
| M, m, Male    | 雄性          |

**实现：**
```rust
impl Sex {
    pub fn to_chinese(&self) -> &'static str {
        match self {
            Sex::Female => "雌性",
            Sex::Male => "雄性",
        }
    }
}
```

---

## UI 工作流示例

### Step 1: 导入数据
```
┌─────────────────────────────────────┐
│ 📁 导入 Excel 文件                   │
│                                     │
│ [选择文件]                           │
│                                     │
│ ✓ 已导入: 10 只动物, 73 个指标       │
│   - 雄性: 6 (60%)                   │
│   - 雌性: 4 (40%)                   │
└─────────────────────────────────────┘
```

### Step 2: 配置分组
```
┌─────────────────────────────────────┐
│ ⚙️ 分组配置                          │
│                                     │
│ 分组数量: [2] ▼                      │
│ 每组动物数: [5] ▼                    │
│                                     │
│ 性别约束:                            │
│   组1: [3]雄 + [2]雌                 │
│   组2: [3]雄 + [2]雌                 │
└─────────────────────────────────────┘
```

### Step 3: 选择指标
```
┌─────────────────────────────────────┐
│ 📊 选择参与统计的指标                 │
│                                     │
│ [全选] [取消] [常用20项]              │
│                                     │
│ ☑ 体重 (kg)                         │
│ ☑ 肛温 (℃)                          │
│ ☑ ALT (U/L)                         │
│ ☑ AST (U/L)                         │
│ ... (已选 73 项)                     │
└─────────────────────────────────────┘
```

### Step 4: 统计配置
```
┌─────────────────────────────────────┐
│ 📈 统计参数                          │
│                                     │
│ 显著性水平 α: [0.05]                 │
│                                     │
│ 优化模式:                            │
│   ◉ 严格模式 (所有 P > α)            │
│   ○ 优化模式 (允许1个 P ≤ α)          │
│                                     │
│ [开始计算分组]                        │
└─────────────────────────────────────┘
```

### Step 5: 查看结果
```
┌─────────────────────────────────────┐
│ ✅ 分组完成 (耗时 125ms)              │
│                                     │
│ 摘要:                                │
│ - 最小 P 值: 0.042                   │
│ - 平均 P 值: 0.315                   │
│ - 达标状态: ⚠️ 1个指标不达标          │
│                                     │
│ [查看分组表] [查看统计详情] [导出]     │
└─────────────────────────────────────┘
```

### Step 6: 导出
```
┌─────────────────────────────────────┐
│ 💾 导出结果                          │
│                                     │
│ 导出格式:                            │
│   ◉ Excel (.xlsx) - 完整版           │
│   ○ Excel (.xlsx) - 仅分组表         │
│   ○ CSV (.csv)                      │
│                                     │
│ 包含内容:                            │
│   ☑ 分组结果表                       │
│   ☑ 统计分析表                       │
│   ☑ 汇总信息                         │
│                                     │
│ [选择保存位置]                        │
└─────────────────────────────────────┘
```

---

## 技术实现要点

### 前端 (React)

**状态管理 (Jotai):**
```typescript
// 新增 atom
export const outputFormatAtom = atom<'chinese' | 'english'>('chinese');
export const exportColumnsAtom = atom<string[]>([]);  // 选择导出的指标列
```

**导出配置组件:**
```typescript
interface ExportConfig {
  format: 'excel' | 'csv';
  includeStatistics: boolean;
  includeSummary: boolean;
  sexFormat: 'chinese' | 'english';  // 雌性/雄性 vs F/M
  selectedColumns: string[];
}
```

### 后端 (Rust)

**新增数据模型:**
```rust
#[derive(Debug, Clone, Serialize)]
pub struct GroupingExportRow {
    pub group_id: usize,
    pub animal_id: String,
    pub sex_chinese: String,  // 雌性 / 雄性
    pub indicators: HashMap<String, f64>,
}

impl GroupingExportRow {
    pub fn from_assignment(
        assignment: &GroupAssignment,
        animal: &Animal,
    ) -> Self {
        Self {
            group_id: assignment.group_id,
            animal_id: assignment.animal_id.clone(),
            sex_chinese: assignment.sex.to_chinese().to_string(),
            indicators: animal.indicators.clone(),
        }
    }
}
```

**Excel 导出函数:**
```rust
use rust_xlsxwriter::*;

pub fn export_grouping_result(
    result: &GroupingResult,
    dataset: &Dataset,
    config: &ExportConfig,
    output_path: &str,
) -> Result<()> {
    let mut workbook = Workbook::new();

    // Sheet 1: 分组结果
    let sheet1 = workbook.add_worksheet();
    sheet1.set_name("分组结果")?;

    // Headers
    sheet1.write_string(0, 0, "组别")?;
    sheet1.write_string(0, 1, "动物编号")?;
    sheet1.write_string(0, 2, "性别")?;

    for (col_idx, indicator_name) in config.selected_columns.iter().enumerate() {
        sheet1.write_string(0, col_idx + 3, indicator_name)?;
    }

    // Data rows (sorted by group, then sex, then animal_id)
    let mut export_rows = result.assignments.iter()
        .map(|a| {
            let animal = dataset.animals.iter()
                .find(|an| an.id == a.animal_id)
                .unwrap();
            GroupingExportRow::from_assignment(a, animal)
        })
        .collect::<Vec<_>>();

    export_rows.sort_by(|a, b| {
        a.group_id.cmp(&b.group_id)
            .then_with(|| b.sex_chinese.cmp(&a.sex_chinese))  // 雌 before 雄
            .then_with(|| a.animal_id.cmp(&b.animal_id))
    });

    for (row_idx, row) in export_rows.iter().enumerate() {
        let excel_row = row_idx + 1;
        sheet1.write_number(excel_row, 0, row.group_id as f64)?;
        sheet1.write_string(excel_row, 1, &row.animal_id)?;
        sheet1.write_string(excel_row, 2, &row.sex_chinese)?;

        for (col_idx, indicator_name) in config.selected_columns.iter().enumerate() {
            if let Some(&value) = row.indicators.get(indicator_name) {
                sheet1.write_number(excel_row, col_idx + 3, value)?;
            }
        }
    }

    // Sheet 2: 统计结果 (if enabled)
    if config.include_statistics {
        let sheet2 = workbook.add_worksheet();
        sheet2.set_name("统计结果")?;
        // ... implementation
    }

    workbook.save(output_path)?;
    Ok(())
}
```

---

## 下一步行动

请确认：

1. **输出格式确认**
   - 是否需要 `组别` 列在第一列？
   - 性别是否必须转换为中文（雌性/雄性）？
   - 是否需要保留所有原始指标列，还是仅导出参与统计的列？

2. **多 sheet 输出**
   - 是否需要输出 3 个 sheet（分组结果 + 统计结果 + 汇总）？
   - 还是仅需要单个 `分组结果` sheet？

3. **优先级**
   - 是先实现核心分组算法，还是先实现标准格式导出？

确认后，我将立即开始：
1. 更新后端导出模块代码
2. 添加前端导出配置界面
3. 实现完整的端到端测试流程
