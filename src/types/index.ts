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

/** Study scenario the user declares before anything else; it decides which methods are
 * offered and is recorded alongside the result. */
export type StudyScenario = "GlpSubmission" | "ConfirmatoryTrial" | "Exploratory";

/**
 * How the allocation was produced. The line that matters for a submission runs between
 * the randomized methods and `Optimized`, which ranks candidates by their P values and
 * is therefore not randomization at all.
 */
export type GroupingMethod =
  | "Optimized"
  | "Random"
  | "ConstrainedRandom"
  | "BlockedRandom"
  /** Sequential covariate-adaptive minimization (Pocock-Simon). */
  | "Minimization";

/**
 * Pre-declared acceptance rule for rejection sampling. Mirrors the Rust enum's
 * internally-tagged serde shape.
 */
export type AcceptanceCriterion =
  | { type: "AlphaLine" }
  | { type: "TopFraction"; target_rate: number };

/** Randomization parameters. Absent for `Optimized`. */
export interface RandomizationConfig {
  /** Null lets the backend generate one and echo it back in the result. */
  seed?: number | null;
  /** Indicator key used to build blocks. Required for `BlockedRandom`. */
  primary_indicator?: string | null;
  /** Null means every draw is accepted (pure randomization). */
  acceptance: AcceptanceCriterion | null;
  max_attempts: number;
  /** 1-based draw number within a run. Always 1 when computed from the configure page. */
  draw_index: number;
  /** Required for `Minimization`, rejected for every other method. */
  minimization?: MinimizationConfig | null;
}

/** How continuous covariates are turned into levels. v1 offers tertiles only. */
export type CovariateBinning = "Tertiles";

/** Parameters of a sequential covariate-adaptive minimization run. */
export interface MinimizationConfig {
  /**
   * Indicator keys balanced on. Deliberately separate from the tested indicators: what a
   * run balances on and what it is later tested on are two different declarations.
   */
  covariates: string[];
  /** Probability of allocating to a minimizer. Open interval (0, 1); default 0.8. */
  allocation_probability: number;
  binning: CovariateBinning;
}

export interface GroupConfig {
  num_groups: number;
  animals_per_group: GroupSize;
  sex_constraints: SexConstraint[];
  scenario: StudyScenario;
  method: GroupingMethod;
  randomization?: RandomizationConfig | null;
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
  /**
   * The draw this animal received, for the pure randomization methods only. There the
   * allocation *is* "sort by it inside the block, then deal each group its quota", so it
   * is exported as an audit column a reviewer can re-sort by hand. Minimization leaves it
   * null: its allocation is a decision chain, not a sort.
   */
  random_number?: number | null;
  /** 1-based block, for blocked randomization only. The draw is sorted within a block. */
  block_index?: number | null;
  /** 1-based position in the seeded entry order, for minimization only. */
  entry_index?: number | null;
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

/** Everything needed to reproduce a randomized allocation years later. */
export interface RandomizationRecord {
  seed: number;
  /** e.g. "chacha12". A seed alone does not pin a sequence. */
  rng_algorithm: string;
  input_fingerprint: string;
  engine_version: string;
  /** Draws consumed before acceptance. */
  attempts: number;
  acceptance: AcceptanceCriterion | null;
  /** The seed the user supplied or the backend generated. Equal to `seed` at draw 1. */
  base_seed: number;
  draw_index: number;
  /** Present only for TopFraction: the calibrated min(P) cutoff. */
  calibrated_threshold?: number | null;
  calibration_draws?: number | null;
  primary_indicator?: string | null;
  block_size?: number | null;
  incomplete_last_block: boolean;
  /** Present only for `Minimization`. */
  minimization?: MinimizationRecord | null;
}

/**
 * What a minimization run actually did. The parameters say what rule was declared; the
 * decision log says whether it was followed, animal by animal.
 */
export interface MinimizationRecord {
  covariates: string[];
  /** Binning scheme identifier, e.g. "tertiles-within-sex". */
  binning: string;
  bins: CovariateBins[];
  allocation_probability: number;
  /** Imbalance measure identifier, e.g. "quota-normalized-range". */
  imbalance_measure: string;
  /** Allocation rule identifier: where the 1 - p probability mass went. */
  allocation_rule: string;
  decisions: MinimizationDecision[];
}

export interface CovariateBins {
  covariate: string;
  strata: CovariateStratumBins[];
}

export interface CovariateStratumBins {
  sex: Sex;
  /** Boundaries between adjacent levels; `levels === cut_points.length + 1`. */
  cut_points: number[];
  levels: number;
}

export interface MinimizationDecision {
  entry_index: number;
  animal_id: string;
  /** Level index per covariate, aligned with `MinimizationRecord.covariates`. */
  levels: number[];
  /** Imbalance per group id; null where the group was not a candidate at that step. */
  scores: (number | null)[];
  took_minimizer: boolean;
  group_id: number;
}

export interface GroupingResult {
  assignments: GroupAssignment[];
  statistics: IndicatorStats[];
  summary: ResultSummary;
  computation_time_ms: number;
  method: GroupingMethod;
  /** Present for the randomized methods, always null for `Optimized`. */
  randomization?: RandomizationRecord | null;
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
