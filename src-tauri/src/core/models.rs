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

/// Grouping configuration
#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct GroupConfig {
    pub num_groups: usize,
    pub animals_per_group: GroupSize,
    pub sex_constraints: Vec<SexConstraint>,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
#[serde(tag = "type")]
pub enum GroupSize {
    Uniform { value: usize },
    Custom { values: Vec<usize> },
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
