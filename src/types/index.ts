/**
 * TypeScript types matching Rust backend models
 * Auto-generated types should match src-tauri/src/core/models.rs
 */

export type Sex = "Male" | "Female";

export type GroupType = "Experimental" | "Reserve";

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

export interface SexConstraint {
  group_index: number;
  male_count: number;
  female_count: number;
  group_type?: GroupType; // defaults to "Experimental"
  custom_name?: string; // e.g., "备用动物" for reserve group
}

export type GroupSize =
  | { type: "Uniform"; value: number }
  | { type: "Custom"; values: number[] };

export interface GroupConfig {
  num_groups: number;
  animals_per_group: GroupSize;
  sex_constraints: SexConstraint[];
}

export type OptimizationMode = "Strict" | "Optimized";

export interface StatConfig {
  selected_indicators: string[];
  alpha: number;
  mode: OptimizationMode;
}

export interface GroupAssignment {
  animal_id: string;
  sex: Sex;
  group_id: number;
}

/** One pairwise post-hoc comparison. Only produced for designs with >= 3 groups. */
export interface PostHocComparison {
  /** Group ids as used by `GroupAssignment.group_id`, not compacted indices. */
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
  /** Main test P > alpha AND every pairwise post-hoc comparison P > alpha. */
  is_valid: boolean;
  /** Absent for two-group designs, which have no post-hoc stage. */
  posthoc_results?: PostHocComparison[] | null;
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

// UI-specific types

export interface ExportConfig {
  selected_indicators: string[];
  include_statistics: boolean;
  include_summary: boolean;
}

export type AppStep = "upload" | "configure" | "compute" | "results";

export interface AppState {
  currentStep: AppStep;
  dataset: Dataset | null;
  groupConfig: GroupConfig | null;
  statConfig: StatConfig | null;
  result: GroupingResult | null;
  isLoading: boolean;
  error: string | null;
}
