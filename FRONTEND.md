# AutoGroup Frontend

动物实验智能分组系统 - 前端部分

## 技术栈

- **Framework**: React 19 + TypeScript 5.8
- **Build Tool**: Vite 7
- **Desktop Runtime**: Tauri 2.x
- **UI Components**: shadcn/ui (Radix UI + Tailwind CSS)
- **State Management**: Jotai
- **Styling**: Tailwind CSS v3
- **Icons**: Lucide React

## 项目结构

```
src/
├── components/
│   ├── ui/              # shadcn/ui base components
│   │   ├── alert.tsx
│   │   ├── button.tsx
│   │   ├── card.tsx
│   │   ├── checkbox.tsx
│   │   ├── input.tsx
│   │   ├── label.tsx
│   │   ├── progress.tsx
│   │   ├── select.tsx
│   │   └── table.tsx
│   └── features/        # Business components
│       ├── UploadPage.tsx      # Step 1: File upload
│       ├── ConfigurePage.tsx   # Step 2: Grouping configuration
│       ├── ComputePage.tsx     # Step 3: Computation progress
│       └── ResultsPage.tsx     # Step 4: Results display
├── stores/
│   └── index.ts         # Jotai state atoms
├── types/
│   └── index.ts         # TypeScript type definitions
├── lib/
│   └── utils.ts         # Utility functions
├── App.tsx              # Main application
├── main.tsx             # React entry point
└── index.css            # Global styles
```

## 开发指南

### 启动开发服务器

```bash
npm run dev
```

访问 http://localhost:1420/

### 构建生产版本

```bash
npm run build
```

### 预览生产构建

```bash
npm run preview
```

## 4步向导流程

### 1. Upload (上传数据)
- 文件选择对话框
- Excel文件解析 (.xlsx/.xls)
- 数据预览 (动物数量、指标数量、性别分布)
- 调用后端 `parse_excel_file` 命令

### 2. Configure (配置参数)
- 分组配置
  - 分组数量 (2-5)
  - 每组动物数 (均匀分配)
  - 性别约束 (每组雄性/雌性数量)
- 统计参数
  - 显著性水平 α (0.01-0.1)
  - 优化模式 (严格/优化)
- 指标选择
  - 多选框选择参与统计的指标
  - 全选/清空功能
- 实时验证配置合法性

### 3. Compute (计算分组)
- 显示计算进度
- 调用后端 `compute_optimal_grouping` 命令
- 模拟进度条动画
- 显示计算参数概览
- 计算完成后自动跳转到结果页

### 4. Results (查看结果)
- 数据概览卡片
  - 总动物数
  - 分组数量
  - 合格指标数 / 总指标数
  - 计算耗时
- 分组结果展示
  - 每组动物编号列表
  - 分组卡片布局
- 统计检验结果表格
  - Levene检验 P值
  - 组间差异检验 P值 (t-test/ANOVA)
  - 通过/警告状态标识
- 导出功能
  - 保存为Excel文件
  - 调用后端 `export_result` 命令
- 重新开始按钮

## 状态管理

使用 Jotai 管理应用状态：

```typescript
// 主要状态
- currentStepAtom: AppStep          // 当前步骤
- datasetAtom: Dataset | null       // 解析后的数据集
- groupConfigAtom: GroupConfig      // 分组配置
- statConfigAtom: StatConfig        // 统计配置
- resultAtom: GroupingResult        // 计算结果
- errorAtom: string | null          // 全局错误信息
- selectedIndicatorsAtom: string[]  // 选中的指标

// 派生状态
- hasDatasetAtom                    // 是否已加载数据
- canProceedToConfigureAtom         // 是否可以进入配置步骤

// 操作
- resetStateAtom                    // 重置所有状态
```

## 与后端通信

使用 Tauri Invoke API 调用 Rust 后端命令：

```typescript
import { invoke } from "@tauri-apps/api/core";

// 解析Excel文件
const dataset = await invoke<Dataset>("parse_excel_file", {
  path: filePath
});

// 计算最优分组
const result = await invoke<GroupingResult>("compute_optimal_grouping", {
  dataset,
  groupConfig,
  statConfig
});

// 导出结果
await invoke("export_result", {
  result,
  dataset,
  path: outputPath
});
```

## 样式系统

使用 Tailwind CSS v3 + CSS Variables 主题系统：

- Light/Dark 模式支持
- HSL 色彩系统
- 语义化设计令牌 (primary, secondary, accent, muted, destructive)
- Responsive 布局
- 平滑过渡动画

## UI/UX 特性

- 步骤指示器 (Header)
- 全局错误提示 (可关闭)
- 加载状态动画
- 表单实时验证
- 友好的错误提示
- 响应式布局
- 可访问性支持 (ARIA)

## 下一步开发

- [ ] 添加图表可视化 (ECharts)
- [ ] 数据表格分页和排序
- [ ] 更多统计方法选项
- [ ] 历史记录功能
- [ ] 批量处理模式
- [ ] 自定义配色主题
- [ ] 导出PDF报告
