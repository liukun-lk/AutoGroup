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
            "M" | "MALE" | "雄性" => Ok(Sex::Male),
            "F" | "FEMALE" | "雌性" => Ok(Sex::Female),
            _ => Err(format!("Invalid sex value: {s}")),
        }
    }

    // Currently unused - kept for potential future use
    #[allow(dead_code)]
    pub fn to_char(&self) -> char {
        match self {
            Sex::Male => 'M',
            Sex::Female => 'F',
        }
    }

    pub fn to_chinese(&self) -> &'static str {
        match self {
            Sex::Male => "雄性",
            Sex::Female => "雌性",
        }
    }
}

/// Metadata for a single indicator
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorMetadata {
    /// Primary key for lookups (usually English name from Row 1, or Chinese name if Row 1 is empty)
    pub key: String,
    /// Display name (Chinese name from Row 2, or English name from Row 1)
    pub display_name: String,
    /// Unit string (extracted from Row 1 if present, otherwise from Row 2)
    pub unit: String,
}

impl IndicatorMetadata {
    pub fn new(key: String, display_name: String, unit: String) -> Self {
        Self {
            key,
            display_name,
            unit,
        }
    }
}

/// Dataset imported from Excel
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct Dataset {
    pub animals: Vec<Animal>,
    pub indicator_names: Vec<String>,
    pub indicator_metadata: Vec<IndicatorMetadata>,
    pub metadata: DatasetMetadata,
}

impl Dataset {
    /// Get metadata for a given indicator key
    pub fn get_indicator_metadata(&self, key: &str) -> Option<&IndicatorMetadata> {
        self.indicator_metadata.iter().find(|m| m.key == key)
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct DatasetMetadata {
    pub total_animals: usize,
    pub male_count: usize,
    pub female_count: usize,
    pub indicator_count: usize,
}

/// Study scenario declared by the user before anything else. Drives which methods are
/// offered, which one is preselected, and how the run is labelled on export.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum StudyScenario {
    /// GLP regulatory submission: randomization only, `Optimized` is rejected.
    GlpSubmission,
    /// Confirmatory trial with a small sample and many covariates.
    ConfirmatoryTrial,
    /// Exploratory / non-GLP work. Anything goes.
    #[default]
    Exploratory,
}

impl StudyScenario {
    pub fn to_chinese(&self) -> &'static str {
        match self {
            StudyScenario::GlpSubmission => "GLP 申报实验",
            StudyScenario::ConfirmatoryTrial => "确证性临床试验",
            StudyScenario::Exploratory => "探索性 / 非 GLP 实验",
        }
    }

    /// Scenario x method matrix. The only hard exclusion is optimization under a GLP
    /// submission: it ranks candidates by P value, which is not randomization, and
    /// writing it up as such would not survive review.
    pub fn allows(&self, method: GroupingMethod) -> bool {
        !matches!(
            (self, method),
            (StudyScenario::GlpSubmission, GroupingMethod::Optimized)
        )
    }

    /// Preselected method for this scenario.
    pub fn default_method(&self) -> GroupingMethod {
        match self {
            StudyScenario::GlpSubmission => GroupingMethod::BlockedRandom,
            StudyScenario::ConfirmatoryTrial => GroupingMethod::Optimized,
            StudyScenario::Exploratory => GroupingMethod::Random,
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum GroupingMethod {
    /// Existing deterministic optimization: enumerate/sample candidates, rank by P values.
    #[default]
    Optimized,
    /// Complete randomization, stratified by sex when more than one stratum exists.
    Random,
    /// Complete randomization plus an acceptance criterion (seeded rejection sampling).
    ConstrainedRandom,
    /// Blocked randomization on a primary indicator, optionally plus the acceptance
    /// criterion for the remaining indicators.
    BlockedRandom,
    /// Sequential covariate-adaptive minimization (Pocock-Simon).
    Minimization,
}

impl GroupingMethod {
    /// The pure randomization family. Kept deliberately narrow because it drives the
    /// wording on export: minimization has a random component but allocates by an
    /// imbalance rule, and calling it "randomized" in a submission would overstate it.
    pub fn is_randomized(&self) -> bool {
        matches!(
            self,
            GroupingMethod::Random
                | GroupingMethod::ConstrainedRandom
                | GroupingMethod::BlockedRandom
        )
    }

    /// Whether this method draws from the seeded RNG, and therefore needs a
    /// [`RandomizationConfig`] and produces a [`RandomizationRecord`].
    ///
    /// This is the predicate to gate configuration on; `is_randomized` answers a
    /// different question and using it here would leave minimization without a seed.
    pub fn uses_random_source(&self) -> bool {
        !matches!(self, GroupingMethod::Optimized)
    }

    pub fn to_chinese(&self) -> &'static str {
        match self {
            GroupingMethod::Optimized => "统计均衡优化",
            GroupingMethod::Random => "完全随机",
            GroupingMethod::ConstrainedRandom => "受限随机化",
            GroupingMethod::BlockedRandom => "按主指标分层随机",
            GroupingMethod::Minimization => "最小化法",
        }
    }
}

/// Acceptance rule applied to each rejection-sampling draw. Both variants are declared
/// before any draw happens and executed by the machine, which is what keeps them inside
/// the restricted-randomization boundary; neither ranks candidates to pick a winner.
#[derive(Debug, Clone, Copy, PartialEq, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum AcceptanceCriterion {
    /// Every tested indicator must clear alpha. Rejects only draws with a detectable
    /// difference (~10% of them); balance is otherwise ordinary-random.
    AlphaLine,
    /// Accept only draws in the most-balanced `target_rate` fraction, ranked by min(P)
    /// over the tested indicators. The cutoff is calibrated per dataset by a seeded
    /// simulation, because min(P)'s scale collapses as the indicator count grows.
    TopFraction { target_rate: f64 },
}

/// Randomization parameters. Absent for `Optimized`.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomizationConfig {
    /// None -> generated by the backend and echoed back to the user.
    #[serde(default)]
    pub seed: Option<u64>,
    /// Indicator key used to build blocks. Required for `BlockedRandom`.
    #[serde(default)]
    pub primary_indicator: Option<String>,
    /// Acceptance rule for rejection sampling. None means every draw is accepted.
    #[serde(default)]
    pub acceptance: Option<AcceptanceCriterion>,
    /// Upper bound on rejection-sampling draws.
    #[serde(default = "default_max_attempts")]
    pub max_attempts: usize,
    /// 1-based draw number within a run. Draw 1 uses the base seed verbatim, so a
    /// protocol-declared seed replays the allocation with no derivation knowledge;
    /// later draws (exploratory only) derive their seed from (base_seed, draw_index).
    #[serde(default = "default_draw_index")]
    pub draw_index: usize,
    /// Minimization-specific parameters. Required when `method == Minimization`, and
    /// rejected for every other method.
    #[serde(default)]
    pub minimization: Option<MinimizationConfig>,
}

/// Parameters of a sequential covariate-adaptive minimization (Pocock-Simon) run.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationConfig {
    /// Indicator keys used as balancing covariates. Numeric, complete, deduplicated,
    /// at least one. Deliberately separate from `StatConfig.selected_indicators`: what a
    /// run balances on and what it is later tested on are two different declarations.
    pub covariates: Vec<String>,
    /// Probability of allocating to a minimizer, in the open interval (0, 1).
    ///
    /// p = 1 is rejected rather than merely discouraged: it removes the random component
    /// entirely, at which point the method is a deterministic search and no longer
    /// belongs on the randomization side of the export wording.
    #[serde(default = "default_allocation_probability")]
    pub allocation_probability: f64,
    /// How continuous covariates are turned into levels.
    #[serde(default)]
    pub binning: CovariateBinning,
}

fn default_allocation_probability() -> f64 {
    0.8
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Default, Serialize, Deserialize)]
pub enum CovariateBinning {
    /// Tertiles cut inside each sex stratum, on values rather than ranks, ties kept in
    /// the same level.
    #[default]
    Tertiles,
}

impl CovariateBinning {
    /// Identifier written into the record, so an archived run says which scheme produced
    /// its levels even after a later version adds more schemes.
    pub fn as_str(&self) -> &'static str {
        match self {
            CovariateBinning::Tertiles => "tertiles-within-sex",
        }
    }

    pub fn to_chinese(&self) -> &'static str {
        match self {
            CovariateBinning::Tertiles => "性别层内三分位",
        }
    }
}

/// Sized for the worst case in the design doc: a 70-indicator dataset under Strict has
/// an acceptance rate around 1.3%, i.e. ~80 draws expected.
fn default_max_attempts() -> usize {
    10_000
}

fn default_draw_index() -> usize {
    1
}

impl Default for RandomizationConfig {
    fn default() -> Self {
        Self {
            seed: None,
            primary_indicator: None,
            acceptance: None,
            max_attempts: default_max_attempts(),
            draw_index: default_draw_index(),
            minimization: None,
        }
    }
}

/// Grouping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupConfig {
    pub num_groups: usize,
    pub animals_per_group: GroupSize,
    pub sex_constraints: Vec<SexConstraint>,
    /// Declared study scenario. Absent in configs saved before scenarios existed, which
    /// land on `Exploratory` — the scenario that permits everything, so replaying an old
    /// config behaves exactly as it did.
    #[serde(default)]
    pub scenario: StudyScenario,
    #[serde(default)]
    pub method: GroupingMethod,
    #[serde(default)]
    pub randomization: Option<RandomizationConfig>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GroupSize {
    Uniform { value: usize },
    Custom { values: Vec<usize> },
}

/// Group type classification
#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum GroupType {
    /// Normal experimental group (participates in statistical tests)
    Experimental,
    /// Reserve animals group (excluded from statistical tests)
    Reserve,
}

impl Default for GroupType {
    fn default() -> Self {
        GroupType::Experimental
    }
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct SexConstraint {
    pub group_index: usize,
    pub male_count: usize,
    pub female_count: usize,
    /// Group type (defaults to Experimental for backward compatibility)
    #[serde(default)]
    pub group_type: GroupType,
    /// Custom name for the group (e.g., "备用动物" for reserve group)
    #[serde(default)]
    pub custom_name: Option<String>,
}

/// Statistical configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct StatConfig {
    pub selected_indicators: Vec<String>,
    pub alpha: f64,
    pub mode: OptimizationMode,
    #[serde(default = "default_max_candidates")]
    pub max_candidates: usize,
}

fn default_max_candidates() -> usize {
    10
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, Serialize, Deserialize)]
pub enum OptimizationMode {
    Strict,
    Optimized,
}

/// Grouping result
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupingResult {
    pub assignments: Vec<GroupAssignment>,
    pub statistics: Vec<IndicatorStats>,
    pub summary: ResultSummary,
    pub computation_time_ms: u64,
    /// How this grouping was produced.
    #[serde(default)]
    pub method: GroupingMethod,
    /// Present for the randomized methods, always None for `Optimized`.
    ///
    /// Strictly this describes the whole run rather than one candidate, but
    /// `export_result` only ever sees a single `GroupingResult`, so it rides along here.
    #[serde(default)]
    pub randomization: Option<RandomizationRecord>,
}

/// Everything needed to reproduce a randomized allocation years later.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct RandomizationRecord {
    pub seed: u64,
    /// The seed the user supplied (or the backend generated). Equal to `seed` at draw 1.
    pub base_seed: u64,
    /// Which draw of the run this is.
    pub draw_index: usize,
    /// e.g. "chacha12". A seed alone does not pin a sequence.
    pub rng_algorithm: String,
    pub input_fingerprint: String,
    pub engine_version: String,
    /// Draws consumed before acceptance.
    pub attempts: usize,
    /// The acceptance rule that was in force. Part of the method description on export.
    pub acceptance: Option<AcceptanceCriterion>,
    pub primary_indicator: Option<String>,
    pub block_size: Option<usize>,
    pub incomplete_last_block: bool,
    /// Present only for `TopFraction`: the calibrated min(P) cutoff a draw had to clear.
    pub calibrated_threshold: Option<f64>,
    /// How many seeded simulation draws produced the cutoff.
    pub calibration_draws: Option<usize>,
    /// Present only for `Minimization`.
    #[serde(default)]
    pub minimization: Option<MinimizationRecord>,
}

/// What a minimization run actually did.
///
/// Sized to answer the two questions a reviewer asks of an adaptive allocation: what was
/// the declared rule, and was it followed for every animal. The parameters answer the
/// first, `decisions` answers the second.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationRecord {
    pub covariates: Vec<String>,
    /// Binning scheme identifier, e.g. "tertiles-within-sex".
    pub binning: String,
    /// The cut points binning actually produced, one entry per covariate. Without these
    /// the method description says "tertiles" and nothing can check what that meant.
    pub bins: Vec<CovariateBins>,
    pub allocation_probability: f64,
    /// Imbalance measure identifier. A later engine reading an archived record has to be
    /// able to tell which formula produced it.
    pub imbalance_measure: String,
    /// Allocation rule identifier: where the 1 - p probability mass went.
    pub allocation_rule: String,
    /// Per-animal decision log, in entry order.
    pub decisions: Vec<MinimizationDecision>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CovariateBins {
    pub covariate: String,
    /// One entry per sex stratum present in the dataset.
    pub strata: Vec<CovariateStratumBins>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct CovariateStratumBins {
    pub sex: Sex,
    /// Boundaries between adjacent levels; `levels == cut_points.len() + 1`.
    pub cut_points: Vec<f64>,
    pub levels: usize,
}

/// One step of the sequential allocation.
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MinimizationDecision {
    /// 1-based position in the seeded entry order.
    pub entry_index: usize,
    pub animal_id: String,
    /// Level index per covariate, aligned with `MinimizationRecord.covariates`.
    pub levels: Vec<usize>,
    /// Imbalance score per group id; groups that were not eligible carry `None`.
    pub scores: Vec<Option<f64>>,
    /// True when the animal went to a minimizer, false when the 1 - p branch fired.
    pub took_minimizer: bool,
    pub group_id: usize,
}

/// `Eq` is deliberately not derived: `random_number` is a float. Assignments are compared
/// for equality in tests, which `PartialEq` covers.
#[derive(Debug, Clone, PartialEq, Serialize, Deserialize)]
pub struct GroupAssignment {
    pub animal_id: String,
    pub sex: Sex,
    pub group_id: usize,
    /// The draw this animal received, for the pure randomization methods only.
    ///
    /// This is not a decorative number: for those methods the allocation *is* "sort by it
    /// inside the block, then deal each group its quota in turn", so exporting it lets a
    /// reviewer re-sort the sheet and confirm every animal's group by hand — the same
    /// check the lab used to do with an Excel `RAND()` column, except this one is
    /// reproducible from a seed. Minimization leaves it `None`: its allocation is a
    /// sequential decision chain, and publishing a per-animal draw there would advertise
    /// a hand check that does not exist.
    #[serde(default)]
    pub random_number: Option<f64>,
    /// 1-based block this animal fell in, for blocked randomization only. The draw is
    /// sorted *within* a block, so the block is needed alongside the number to redo it.
    #[serde(default)]
    pub block_index: Option<usize>,
    /// 1-based position in the seeded entry order, for minimization only. The order the
    /// animals entered is what the decision log is indexed by, so it is the field that
    /// ties the exported grouping back to that log.
    #[serde(default)]
    pub entry_index: Option<usize>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct IndicatorStats {
    pub indicator_name: String,
    pub levene_p_value: f64,
    pub diff_p_value: f64,
    pub test_method: String,
    pub is_valid: bool,
    pub posthoc_results: Option<Vec<PostHocComparison>>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct PostHocComparison {
    pub group1_id: usize,
    pub group2_id: usize,
    pub p_value: f64,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultSummary {
    pub min_p_value: f64,
    pub mean_p_value: f64,
    pub num_invalid_indicators: usize,
    pub meets_criteria: bool,
    pub total_animals: usize,
    pub num_groups: usize,
    pub passed_indicators: usize,
    pub total_indicators: usize,
}

/// Internal: Candidate grouping (indices only)
#[derive(Debug, Clone)]
pub struct CandidateGrouping {
    pub groups: Vec<Vec<usize>>, // group_id -> [animal_indices]
}

/// Multi-candidate grouping result with Top-N solutions
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct MultiGroupingResult {
    pub candidates: Vec<GroupingResult>,
    pub total_evaluated: usize,
    pub total_valid: usize,
    pub computation_time_ms: u64,
}

/// Export configuration for result export
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ExportConfig {
    pub mode: ExportMode,
    pub output_path: String,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum ExportMode {
    /// Export a single selected candidate
    Single { candidate_index: usize },
    /// Export all candidates to separate worksheets
    MultiSheet,
}
