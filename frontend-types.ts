/**
 * TypeScript type definitions for AutoGroup Tauri commands
 * Mirror of Rust types in src-tauri/src/core/models.rs
 */

export type Sex = "Male" | "Female";

export interface Animal {
  id: string;
  sex: Sex;
  indicators: Record<string, number>;
}

export interface IndicatorMetadata {
  key: string;
  display_name: string;
  unit: string;
}

export interface DatasetMetadata {
  total_animals: number;
  male_count: number;
  female_count: number;
  indicator_count: number;
}

export interface Dataset {
  animals: Animal[];
  indicator_names: string[];
  indicator_metadata: IndicatorMetadata[];
  metadata: DatasetMetadata;
}

export interface GroupConfig {
  num_groups: number;
  animals_per_group: GroupSize;
  sex_constraints: SexConstraint[];
}

export type GroupSize =
  | { type: "Uniform"; value: number }
  | { type: "Custom"; values: number[] };

export interface SexConstraint {
  group_index: number;
  male_count: number;
  female_count: number;
}

export interface StatConfig {
  selected_indicators: string[];
  alpha: number;
  mode: OptimizationMode;
  max_candidates?: number; // Defaults to 10 if not provided
}

export type OptimizationMode = "Strict" | "Optimized";

export interface GroupAssignment {
  animal_id: string;
  sex: Sex;
  group_id: number;
}

export interface PostHocComparison {
  group1_id: number;
  group2_id: number;
  p_value: number;
  is_valid: boolean;
}

export interface IndicatorStats {
  indicator_name: string;
  levene_p_value: number;
  diff_p_value: number;
  test_method: string;
  is_valid: boolean;
  posthoc_results?: PostHocComparison[];
}

export interface ResultSummary {
  min_p_value: number;
  mean_p_value: number;
  num_invalid_indicators: number;
  meets_criteria: boolean;
  total_animals: number;
  num_groups: number;
  passed_indicators: number;
  total_indicators: number;
}

export interface GroupingResult {
  assignments: GroupAssignment[];
  statistics: IndicatorStats[];
  summary: ResultSummary;
  computation_time_ms: number;
}

export interface MultiGroupingResult {
  candidates: GroupingResult[];
  total_evaluated: number;
  total_valid: number;
  computation_time_ms: number;
}

export interface ExportConfig {
  mode: ExportMode;
  output_path: string;
}

export type ExportMode =
  | { type: "Single"; candidate_index: number }
  | { type: "MultiSheet" };

/**
 * Tauri Command Interfaces
 */

// Import command
export async function parseExcel(filePath: string): Promise<Dataset> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<Dataset>("parse_excel", { filePath });
}

// Grouping command - now returns multiple candidates
export async function computeGrouping(
  dataset: Dataset,
  groupConfig: GroupConfig,
  statConfig: StatConfig
): Promise<MultiGroupingResult> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<MultiGroupingResult>("compute_grouping", {
    dataset,
    groupConfig,
    statConfig,
  });
}

// Export single result
export async function exportResult(
  result: GroupingResult,
  dataset: Dataset,
  selectedIndicators: string[],
  outputPath: string
): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("export_result", {
    result,
    dataset,
    selectedIndicators,
    outputPath,
  });
}

// Export multiple results to multi-sheet Excel
export async function exportMultipleResults(
  multiResult: MultiGroupingResult,
  dataset: Dataset,
  selectedIndicators: string[],
  outputPath: string
): Promise<void> {
  const { invoke } = await import("@tauri-apps/api/core");
  return invoke<void>("export_multiple_results", {
    multiResult,
    dataset,
    selectedIndicators,
    outputPath,
  });
}
