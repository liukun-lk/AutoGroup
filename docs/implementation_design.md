# AutoGroup Implementation Design

> Based on confirmed tech stack: Tauri + React + TypeScript + Rust
> UI: shadcn/ui, Table: TanStack Table, Charts: ECharts, State: Jotai
> Backend: Pure Rust statistics, SQLite persistence

---

## 1. Test Data Analysis

### 1.1 Actual Data Format
From test file `通用动物实验自动分组软件_测试用数据.xlsx`:

```
Row 1: [None, None, 'kg', '℃', 'ALT', 'AST', 'TP', ...]  # Indicator names
Row 2: ['动物编号', '性别', '体重', '肛温', 'U/L', 'U/L', ...]  # Chinese headers + units
Row 3+: ['XHP2601001', 'F', 31.85, 38.5, 58.8, 23.1, ...]  # Data rows
```

**Key observations:**
- 11 rows total (1 header + 1 label row + 1 unit row + 10 animals)
- 75 columns (2 ID columns + 73 indicators)
- Header is split across 2 rows (row 1: English names, row 2: Chinese + units)
- First column: AnimalID (e.g., `XHP2601001`)
- Second column: Sex (`F` or `M`)
- Remaining columns: Numeric indicators

**Data validation requirements:**
- Need to handle multi-row headers
- Need to skip unit row (row 2)
- Need to parse numeric values correctly (handle empty cells)
- Need to validate Sex column contains only 'M' or 'F'

---

## 2. Backend Architecture (Rust)

### 2.1 Project Structure

```
src-tauri/
├── Cargo.toml
├── src/
│   ├── main.rs
│   ├── lib.rs
│   ├── commands/           # Tauri command handlers
│   │   ├── mod.rs
│   │   ├── import.rs       # Excel import command
│   │   ├── grouping.rs     # Grouping computation command
│   │   ├── config.rs       # Config template commands
│   │   ├── history.rs      # History management commands
│   │   └── export.rs       # Export commands
│   ├── core/               # Core business logic
│   │   ├── mod.rs
│   │   ├── models.rs       # Data models
│   │   ├── parser.rs       # Excel parser
│   │   ├── validator.rs    # Data validation
│   │   ├── grouping/       # Grouping algorithms
│   │   │   ├── mod.rs
│   │   │   ├── enumerator.rs    # Exhaustive enumeration
│   │   │   ├── monte_carlo.rs   # Monte Carlo sampling
│   │   │   └── evaluator.rs     # Grouping evaluation
│   │   └── stats/          # Statistics engine
│   │       ├── mod.rs
│   │       ├── levene.rs   # Levene test
│   │       ├── ttest.rs    # Student & Welch t-test
│   │       ├── anova.rs    # One-way ANOVA
│   │       ├── tukey.rs    # Tukey HSD post-hoc
│   │       └── dunnett.rs  # Dunnett's T3 post-hoc
│   ├── persistence/        # Data persistence
│   │   ├── mod.rs
│   │   ├── db.rs           # SQLite connection
│   │   ├── config_repo.rs  # Config CRUD
│   │   └── history_repo.rs # History CRUD
│   └── utils/
│       ├── mod.rs
│       └── error.rs        # Error types
```

### 2.2 Dependencies (Cargo.toml additions)

```toml
[dependencies]
# Existing
tauri = { version = "2", features = [] }
tauri-plugin-opener = "2"
serde = { version = "1", features = ["derive"] }
serde_json = "1"

# Excel parsing
calamine = "0.27"

# Statistics
statrs = "0.18"
special = "0.11"  # For advanced math functions (erf, gamma, etc.)

# Database
rusqlite = { version = "0.32", features = ["bundled"] }

# Parallel computing
rayon = "1.10"

# Error handling
anyhow = "1.0"
thiserror = "2.0"

# Utilities
chrono = { version = "0.4", features = ["serde"] }
uuid = { version = "1.11", features = ["v4", "serde"] }

# Excel export
rust_xlsxwriter = "0.83"
```

### 2.3 Core Data Models

```rust
// src/core/models.rs

use serde::{Deserialize, Serialize};
use std::collections::HashMap;

/// Raw animal data from Excel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Animal {
    pub id: String,
    pub sex: Sex,
    pub indicators: HashMap<String, f64>,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum Sex {
    Male,
    Female,
}

impl Sex {
    pub fn from_str(s: &str) -> Result<Self, String> {
        match s.trim().to_uppercase().as_str() {
            "M" | "MALE" => Ok(Sex::Male),
            "F" | "FEMALE" => Ok(Sex::Female),
            _ => Err(format!("Invalid sex value: {}", s)),
        }
    }

    pub fn to_char(&self) -> char {
        match self {
            Sex::Male => 'M',
            Sex::Female => 'F',
        }
    }
}

/// Dataset imported from Excel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub animals: Vec<Animal>,
    pub indicator_names: Vec<String>,
    pub metadata: DatasetMetadata,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMetadata {
    pub total_animals: usize,
    pub male_count: usize,
    pub female_count: usize,
    pub indicator_count: usize,
}

/// Grouping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupConfig {
    pub num_groups: usize,
    pub animals_per_group: GroupSize,
    pub sex_constraints: Vec<SexConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub enum GroupSize {
    Uniform(usize),  // All groups same size
    Custom(Vec<usize>),  // Specify each group size
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SexConstraint {
    pub group_index: usize,
    pub male_count: usize,
    pub female_count: usize,
}

/// Statistical configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatConfig {
    pub selected_indicators: Vec<String>,
    pub alpha: f64,  // Significance level (default: 0.05)
    pub mode: OptimizationMode,
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationMode {
    Strict,      // All P > alpha
    Optimized,   // Allow at most 1 indicator with P <= alpha
}

/// Grouping result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupingResult {
    pub assignments: Vec<GroupAssignment>,  // Animal -> Group mapping
    pub statistics: Vec<IndicatorStats>,
    pub summary: ResultSummary,
    pub computation_time_ms: u64,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupAssignment {
    pub animal_id: String,
    pub sex: Sex,
    pub group_id: usize,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorStats {
    pub indicator_name: String,
    pub p_value: f64,
    pub test_method: String,  // e.g., "Student t-test", "Welch ANOVA + Dunnett's T3"
    pub is_valid: bool,  // P > alpha
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultSummary {
    pub min_p_value: f64,
    pub mean_p_value: f64,
    pub num_invalid_indicators: usize,
    pub meets_criteria: bool,
}
```

### 2.4 Tauri Commands Interface

```rust
// src/commands/import.rs

use crate::core::{parser, validator, models::Dataset};
use tauri::State;
use anyhow::Result;

#[tauri::command]
pub async fn parse_excel(
    file_path: String,
) -> Result<Dataset, String> {
    let dataset = parser::parse_excel_file(&file_path)
        .map_err(|e| format!("Failed to parse Excel: {}", e))?;

    validator::validate_dataset(&dataset)
        .map_err(|e| format!("Data validation failed: {}", e))?;

    Ok(dataset)
}

// src/commands/grouping.rs

use crate::core::{
    models::{Dataset, GroupConfig, StatConfig, GroupingResult},
    grouping,
};

#[tauri::command]
pub async fn compute_grouping(
    dataset: Dataset,
    group_config: GroupConfig,
    stat_config: StatConfig,
) -> Result<GroupingResult, String> {
    grouping::compute_optimal_grouping(dataset, group_config, stat_config)
        .map_err(|e| format!("Grouping computation failed: {}", e))
}

// src/commands/export.rs

#[tauri::command]
pub async fn export_result(
    result: GroupingResult,
    format: ExportFormat,
    output_path: String,
) -> Result<(), String> {
    // TODO: Implement Excel/CSV export
    Ok(())
}

#[derive(Debug, Deserialize)]
pub enum ExportFormat {
    Excel,
    Csv,
}
```

### 2.5 Excel Parser Implementation

```rust
// src/core/parser.rs

use calamine::{Reader, Xlsx, open_workbook};
use anyhow::{Result, anyhow};
use std::collections::HashMap;
use crate::core::models::{Animal, Dataset, DatasetMetadata, Sex};

pub fn parse_excel_file(path: &str) -> Result<Dataset> {
    let mut workbook: Xlsx<_> = open_workbook(path)?;
    let sheet_name = workbook.sheet_names()[0].clone();
    let range = workbook.worksheet_range(&sheet_name)?;

    // Row 0: Indicator names (skip first 2 columns)
    let indicator_row = range.rows().next()
        .ok_or_else(|| anyhow!("Empty Excel file"))?;

    let mut indicator_names: Vec<String> = indicator_row
        .iter()
        .skip(2)
        .filter_map(|cell| {
            cell.get_string()
                .filter(|s| !s.is_empty())
                .map(|s| s.to_string())
        })
        .collect();

    // Row 1: Chinese headers + units (skip)
    // Row 2: Unit row (skip)

    // Parse animal data starting from row 2
    let mut animals = Vec::new();
    for (row_idx, row) in range.rows().enumerate().skip(2) {
        if row.is_empty() {
            continue;
        }

        // Column 0: AnimalID
        let animal_id = row.get(0)
            .and_then(|c| c.get_string())
            .ok_or_else(|| anyhow!("Missing AnimalID at row {}", row_idx + 1))?
            .to_string();

        // Column 1: Sex
        let sex_str = row.get(1)
            .and_then(|c| c.get_string())
            .ok_or_else(|| anyhow!("Missing Sex at row {}", row_idx + 1))?;
        let sex = Sex::from_str(sex_str)
            .map_err(|e| anyhow!("Invalid sex at row {}: {}", row_idx + 1, e))?;

        // Remaining columns: Indicators
        let mut indicators = HashMap::new();
        for (col_idx, cell) in row.iter().skip(2).enumerate() {
            if col_idx >= indicator_names.len() {
                break;
            }

            if let Some(value) = cell.get_float() {
                indicators.insert(indicator_names[col_idx].clone(), value);
            }
        }

        animals.push(Animal {
            id: animal_id,
            sex,
            indicators,
        });
    }

    let male_count = animals.iter().filter(|a| a.sex == Sex::Male).count();
    let female_count = animals.iter().filter(|a| a.sex == Sex::Female).count();

    Ok(Dataset {
        animals,
        indicator_names,
        metadata: DatasetMetadata {
            total_animals: animals.len(),
            male_count,
            female_count,
            indicator_count: indicator_names.len(),
        },
    })
}
```

### 2.6 Statistics Engine - Levene Test

```rust
// src/core/stats/levene.rs

use anyhow::Result;
use crate::core::stats::anova::one_way_anova;

/// Levene's test for homogeneity of variance
/// Returns P-value (null hypothesis: variances are equal)
pub fn levene_test(groups: &[Vec<f64>]) -> Result<f64> {
    // Transform data: |x_ij - median_i|
    let transformed_groups: Vec<Vec<f64>> = groups
        .iter()
        .map(|group| {
            let median = compute_median(group);
            group.iter().map(|&x| (x - median).abs()).collect()
        })
        .collect();

    // Run ANOVA on transformed data
    one_way_anova(&transformed_groups)
}

fn compute_median(data: &[f64]) -> f64 {
    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());
    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
}
```

### 2.7 Statistics Engine - t-test

```rust
// src/core/stats/ttest.rs

use statrs::distribution::{StudentsT, ContinuousCDF};
use anyhow::Result;

/// Student's t-test (equal variance assumed)
pub fn student_ttest(group1: &[f64], group2: &[f64]) -> Result<f64> {
    let n1 = group1.len() as f64;
    let n2 = group2.len() as f64;

    let mean1 = group1.iter().sum::<f64>() / n1;
    let mean2 = group2.iter().sum::<f64>() / n2;

    let var1 = group1.iter().map(|x| (x - mean1).powi(2)).sum::<f64>() / (n1 - 1.0);
    let var2 = group2.iter().map(|x| (x - mean2).powi(2)).sum::<f64>() / (n2 - 1.0);

    // Pooled variance
    let sp_squared = ((n1 - 1.0) * var1 + (n2 - 1.0) * var2) / (n1 + n2 - 2.0);
    let se = (sp_squared * (1.0 / n1 + 1.0 / n2)).sqrt();

    let t_stat = (mean1 - mean2) / se;
    let df = n1 + n2 - 2.0;

    let t_dist = StudentsT::new(0.0, 1.0, df)?;
    let p_value = 2.0 * (1.0 - t_dist.cdf(t_stat.abs()));

    Ok(p_value)
}

/// Welch's t-test (unequal variance)
pub fn welch_ttest(group1: &[f64], group2: &[f64]) -> Result<f64> {
    let n1 = group1.len() as f64;
    let n2 = group2.len() as f64;

    let mean1 = group1.iter().sum::<f64>() / n1;
    let mean2 = group2.iter().sum::<f64>() / n2;

    let var1 = group1.iter().map(|x| (x - mean1).powi(2)).sum::<f64>() / (n1 - 1.0);
    let var2 = group2.iter().map(|x| (x - mean2).powi(2)).sum::<f64>() / (n2 - 1.0);

    let se = (var1 / n1 + var2 / n2).sqrt();
    let t_stat = (mean1 - mean2) / se;

    // Welch-Satterthwaite degrees of freedom
    let df = (var1 / n1 + var2 / n2).powi(2)
        / ((var1 / n1).powi(2) / (n1 - 1.0) + (var2 / n2).powi(2) / (n2 - 1.0));

    let t_dist = StudentsT::new(0.0, 1.0, df)?;
    let p_value = 2.0 * (1.0 - t_dist.cdf(t_stat.abs()));

    Ok(p_value)
}
```

### 2.8 Grouping Algorithm Core

```rust
// src/core/grouping/mod.rs

use crate::core::models::*;
use anyhow::Result;
use rayon::prelude::*;

pub fn compute_optimal_grouping(
    dataset: Dataset,
    group_config: GroupConfig,
    stat_config: StatConfig,
) -> Result<GroupingResult> {
    let start_time = std::time::Instant::now();

    // Step 1: Generate candidate groupings
    let candidates = if dataset.animals.len() <= 20 {
        enumerator::enumerate_all(&dataset.animals, &group_config)?
    } else {
        monte_carlo::sample_groupings(&dataset.animals, &group_config, 100_000)?
    };

    // Step 2: Evaluate candidates in parallel
    let evaluated: Vec<_> = candidates
        .par_iter()
        .filter_map(|candidate| {
            evaluator::evaluate_grouping(candidate, &dataset, &stat_config).ok()
        })
        .collect();

    // Step 3: Select best grouping
    let best = evaluated
        .into_iter()
        .filter(|result| {
            match stat_config.mode {
                OptimizationMode::Strict => result.summary.num_invalid_indicators == 0,
                OptimizationMode::Optimized => result.summary.num_invalid_indicators <= 1,
            }
        })
        .max_by(|a, b| {
            // Primary: max(min_p_value)
            let cmp = a.summary.min_p_value.partial_cmp(&b.summary.min_p_value).unwrap();
            if cmp == std::cmp::Ordering::Equal {
                // Secondary: max(mean_p_value)
                a.summary.mean_p_value.partial_cmp(&b.summary.mean_p_value).unwrap()
            } else {
                cmp
            }
        })
        .ok_or_else(|| anyhow::anyhow!("No valid grouping found"))?;

    let computation_time_ms = start_time.elapsed().as_millis() as u64;

    Ok(GroupingResult {
        computation_time_ms,
        ..best
    })
}
```

---

## 3. Frontend Architecture (React + TypeScript)

### 3.1 Project Structure

```
src/
├── App.tsx
├── main.tsx
├── components/
│   ├── ui/                 # shadcn/ui components
│   │   ├── button.tsx
│   │   ├── input.tsx
│   │   ├── table.tsx
│   │   ├── dialog.tsx
│   │   ├── tabs.tsx
│   │   └── ...
│   ├── layout/
│   │   ├── MainLayout.tsx
│   │   └── Sidebar.tsx
│   ├── data-import/
│   │   ├── FileUploader.tsx
│   │   └── DataPreview.tsx
│   ├── configuration/
│   │   ├── GroupConfigPanel.tsx
│   │   ├── StatConfigPanel.tsx
│   │   └── OptimizationModeSelector.tsx
│   ├── results/
│   │   ├── GroupingResultTable.tsx
│   │   ├── StatisticsTable.tsx
│   │   ├── SummaryCard.tsx
│   │   └── PValueChart.tsx
│   └── history/
│       └── HistoryList.tsx
├── hooks/
│   ├── useGrouping.ts
│   ├── useDataset.ts
│   └── useConfig.ts
├── lib/
│   ├── tauri.ts            # Tauri command wrappers
│   └── utils.ts
├── store/                  # Jotai atoms
│   ├── dataset.ts
│   ├── config.ts
│   └── result.ts
└── types/
    └── index.ts            # TypeScript type definitions
```

### 3.2 TypeScript Type Definitions

```typescript
// src/types/index.ts

export type Sex = 'Male' | 'Female';

export interface Animal {
  id: string;
  sex: Sex;
  indicators: Record<string, number>;
}

export interface Dataset {
  animals: Animal[];
  indicator_names: string[];
  metadata: DatasetMetadata;
}

export interface DatasetMetadata {
  total_animals: number;
  male_count: number;
  female_count: number;
  indicator_count: number;
}

export interface GroupConfig {
  num_groups: number;
  animals_per_group: GroupSize;
  sex_constraints: SexConstraint[];
}

export type GroupSize =
  | { type: 'Uniform'; value: number }
  | { type: 'Custom'; values: number[] };

export interface SexConstraint {
  group_index: number;
  male_count: number;
  female_count: number;
}

export interface StatConfig {
  selected_indicators: string[];
  alpha: number;
  mode: OptimizationMode;
}

export type OptimizationMode = 'Strict' | 'Optimized';

export interface GroupingResult {
  assignments: GroupAssignment[];
  statistics: IndicatorStats[];
  summary: ResultSummary;
  computation_time_ms: number;
}

export interface GroupAssignment {
  animal_id: string;
  sex: Sex;
  group_id: number;
}

export interface IndicatorStats {
  indicator_name: string;
  p_value: number;
  test_method: string;
  is_valid: boolean;
}

export interface ResultSummary {
  min_p_value: number;
  mean_p_value: number;
  num_invalid_indicators: number;
  meets_criteria: boolean;
}
```

### 3.3 Jotai Store Setup

```typescript
// src/store/dataset.ts
import { atom } from 'jotai';
import type { Dataset } from '@/types';

export const datasetAtom = atom<Dataset | null>(null);
export const isDataLoadedAtom = atom((get) => get(datasetAtom) !== null);

// src/store/config.ts
import { atom } from 'jotai';
import type { GroupConfig, StatConfig } from '@/types';

export const groupConfigAtom = atom<GroupConfig>({
  num_groups: 2,
  animals_per_group: { type: 'Uniform', value: 5 },
  sex_constraints: [],
});

export const statConfigAtom = atom<StatConfig>({
  selected_indicators: [],
  alpha: 0.05,
  mode: 'Strict',
});

// src/store/result.ts
import { atom } from 'jotai';
import type { GroupingResult } from '@/types';

export const groupingResultAtom = atom<GroupingResult | null>(null);
export const isComputingAtom = atom(false);
```

### 3.4 Tauri Command Wrappers

```typescript
// src/lib/tauri.ts
import { invoke } from '@tauri-apps/api/core';
import type { Dataset, GroupConfig, StatConfig, GroupingResult } from '@/types';

export async function parseExcel(filePath: string): Promise<Dataset> {
  return invoke<Dataset>('parse_excel', { filePath });
}

export async function computeGrouping(
  dataset: Dataset,
  groupConfig: GroupConfig,
  statConfig: StatConfig
): Promise<GroupingResult> {
  return invoke<GroupingResult>('compute_grouping', {
    dataset,
    groupConfig,
    statConfig,
  });
}

export async function exportResult(
  result: GroupingResult,
  format: 'Excel' | 'Csv',
  outputPath: string
): Promise<void> {
  return invoke('export_result', { result, format, outputPath });
}
```

### 3.5 Main UI Components

#### File Uploader
```typescript
// src/components/data-import/FileUploader.tsx
import { useState } from 'react';
import { useSetAtom } from 'jotai';
import { open } from '@tauri-apps/plugin-dialog';
import { parseExcel } from '@/lib/tauri';
import { datasetAtom } from '@/store/dataset';
import { Button } from '@/components/ui/button';

export function FileUploader() {
  const [isLoading, setIsLoading] = useState(false);
  const setDataset = useSetAtom(datasetAtom);

  const handleUpload = async () => {
    setIsLoading(true);
    try {
      const filePath = await open({
        filters: [{ name: 'Excel', extensions: ['xlsx', 'xls'] }],
      });

      if (filePath) {
        const dataset = await parseExcel(filePath as string);
        setDataset(dataset);
      }
    } catch (error) {
      console.error('Failed to parse Excel:', error);
      alert(`Error: ${error}`);
    } finally {
      setIsLoading(false);
    }
  };

  return (
    <div className="space-y-4">
      <Button onClick={handleUpload} disabled={isLoading}>
        {isLoading ? 'Loading...' : 'Import Excel File'}
      </Button>
    </div>
  );
}
```

#### TanStack Table for Results
```typescript
// src/components/results/GroupingResultTable.tsx
import { useMemo } from 'react';
import { useAtomValue } from 'jotai';
import {
  useReactTable,
  getCoreRowModel,
  getSortedRowModel,
  flexRender,
  createColumnHelper,
} from '@tanstack/react-table';
import { groupingResultAtom } from '@/store/result';
import type { GroupAssignment } from '@/types';

const columnHelper = createColumnHelper<GroupAssignment>();

export function GroupingResultTable() {
  const result = useAtomValue(groupingResultAtom);

  const columns = useMemo(
    () => [
      columnHelper.accessor('animal_id', {
        header: 'Animal ID',
        cell: (info) => info.getValue(),
      }),
      columnHelper.accessor('sex', {
        header: 'Sex',
        cell: (info) => info.getValue(),
      }),
      columnHelper.accessor('group_id', {
        header: 'Group',
        cell: (info) => `Group ${info.getValue()}`,
      }),
    ],
    []
  );

  const table = useReactTable({
    data: result?.assignments ?? [],
    columns,
    getCoreRowModel: getCoreRowModel(),
    getSortedRowModel: getSortedRowModel(),
  });

  if (!result) {
    return <div>No result to display</div>;
  }

  return (
    <div className="rounded-md border">
      <table className="w-full">
        <thead>
          {table.getHeaderGroups().map((headerGroup) => (
            <tr key={headerGroup.id}>
              {headerGroup.headers.map((header) => (
                <th key={header.id} className="border-b p-2 text-left">
                  {flexRender(header.column.columnDef.header, header.getContext())}
                </th>
              ))}
            </tr>
          ))}
        </thead>
        <tbody>
          {table.getRowModel().rows.map((row) => (
            <tr key={row.id}>
              {row.getVisibleCells().map((cell) => (
                <td key={cell.id} className="border-b p-2">
                  {flexRender(cell.column.columnDef.cell, cell.getContext())}
                </td>
              ))}
            </tr>
          ))}
        </tbody>
      </table>
    </div>
  );
}
```

#### ECharts P-value Distribution
```typescript
// src/components/results/PValueChart.tsx
import { useEffect, useRef } from 'react';
import { useAtomValue } from 'jotai';
import * as echarts from 'echarts';
import { groupingResultAtom } from '@/store/result';

export function PValueChart() {
  const result = useAtomValue(groupingResultAtom);
  const chartRef = useRef<HTMLDivElement>(null);

  useEffect(() => {
    if (!result || !chartRef.current) return;

    const chart = echarts.init(chartRef.current);

    const option: echarts.EChartsOption = {
      title: {
        text: 'P-value Distribution',
      },
      tooltip: {
        trigger: 'axis',
        axisPointer: { type: 'shadow' },
      },
      xAxis: {
        type: 'category',
        data: result.statistics.map((s) => s.indicator_name),
        axisLabel: { rotate: 45 },
      },
      yAxis: {
        type: 'value',
        name: 'P-value',
      },
      series: [
        {
          name: 'P-value',
          type: 'bar',
          data: result.statistics.map((s) => ({
            value: s.p_value,
            itemStyle: {
              color: s.is_valid ? '#10b981' : '#ef4444',
            },
          })),
        },
      ],
    };

    chart.setOption(option);

    return () => {
      chart.dispose();
    };
  }, [result]);

  return <div ref={chartRef} className="h-96 w-full" />;
}
```

---

## 4. Database Schema (SQLite)

```sql
-- Config templates table
CREATE TABLE config_templates (
    id TEXT PRIMARY KEY,
    name TEXT NOT NULL,
    description TEXT,
    group_config TEXT NOT NULL,  -- JSON
    stat_config TEXT NOT NULL,   -- JSON
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP,
    updated_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Grouping history table
CREATE TABLE grouping_history (
    id TEXT PRIMARY KEY,
    dataset_summary TEXT NOT NULL,  -- JSON: { total_animals, indicators, etc. }
    group_config TEXT NOT NULL,     -- JSON
    stat_config TEXT NOT NULL,      -- JSON
    result TEXT NOT NULL,           -- JSON: full GroupingResult
    created_at DATETIME DEFAULT CURRENT_TIMESTAMP
);

-- Indices
CREATE INDEX idx_config_templates_created ON config_templates(created_at DESC);
CREATE INDEX idx_history_created ON grouping_history(created_at DESC);
```

---

## 5. Development Roadmap

### Phase 1: Core Infrastructure (Week 1-2)
- [x] Project structure setup
- [ ] Install dependencies (Rust + Node.js)
- [ ] Setup shadcn/ui components
- [ ] Implement data models (Rust + TypeScript)
- [ ] Excel parser (calamine)
- [ ] Basic Tauri commands
- [ ] Jotai store setup
- [ ] File upload UI

### Phase 2: Statistics Engine (Week 3-4)
- [ ] Implement Levene test
- [ ] Implement Student & Welch t-test
- [ ] Implement One-way ANOVA
- [ ] Implement Tukey HSD
- [ ] Implement Welch ANOVA + Dunnett's T3
- [ ] Unit tests (compare with Python scipy)
- [ ] Integration tests

### Phase 3: Grouping Algorithm (Week 5)
- [ ] Exhaustive enumerator (small datasets)
- [ ] Monte Carlo sampler (large datasets)
- [ ] Grouping evaluator
- [ ] Parallel computation (rayon)
- [ ] Progress reporting

### Phase 4: Frontend UI (Week 6-7)
- [ ] Group configuration panel
- [ ] Statistical configuration panel
- [ ] Compute button + progress indicator
- [ ] Results display (TanStack Table)
- [ ] P-value chart (ECharts)
- [ ] Export functionality

### Phase 5: Persistence & History (Week 8)
- [ ] SQLite integration
- [ ] Config template CRUD
- [ ] History CRUD
- [ ] Template selector UI
- [ ] History viewer UI

### Phase 6: Polish & Testing (Week 9-10)
- [ ] Error handling improvements
- [ ] UI/UX refinements
- [ ] Performance optimization
- [ ] User manual
- [ ] End-to-end testing
- [ ] Packaging (Tauri build)

---

## 6. Testing Strategy

### 6.1 Statistics Validation
**Goal:** Ensure Rust implementation matches scipy results

```rust
#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levene_vs_scipy() {
        // Known test case from scipy documentation
        let group1 = vec![1.0, 2.0, 3.0, 4.0];
        let group2 = vec![2.0, 3.0, 4.0, 5.0];
        let p = levene_test(&[group1, group2]).unwrap();

        // Expected P-value from scipy.stats.levene
        let expected = 1.0;  // (example)
        assert!((p - expected).abs() < 1e-6);
    }
}
```

### 6.2 Test Data
Use provided `通用动物实验自动分组软件_测试用数据.xlsx`:
- 10 animals (6 males, 4 females)
- 73 indicators
- Test grouping: 2 groups, 5 animals each, 3M+2F per group

**Expected workflow:**
1. Import test Excel
2. Configure: 2 groups, 5 animals/group, 3M+2F
3. Select all indicators
4. Run grouping (strict mode)
5. Verify: all P > 0.05
6. Export result

---

## 7. Next Steps

Please confirm:
1. **Statistics implementation priority**: Which test to implement first?
   - Suggestion: Levene → Student t-test → One-way ANOVA → Tukey
2. **shadcn/ui setup**: Need help with initial component setup?
3. **Test data usage**: Should I create a dedicated test suite with this data?
4. **Performance target**: Expected max animals count? (affects algorithm choice)

After confirmation, I will:
1. Generate starter code for Rust modules
2. Setup shadcn/ui boilerplate
3. Create detailed implementation guides for statistics functions
4. Write unit test scaffolds
