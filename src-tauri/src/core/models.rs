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
            _ => Err(format!("Invalid sex value: {}", s)),
        }
    }

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
    pub p_value: f64,
    pub test_method: String,
    pub is_valid: bool,
}

#[derive(Debug, Clone, Serialize, Deserialize)]
pub struct ResultSummary {
    pub min_p_value: f64,
    pub mean_p_value: f64,
    pub num_invalid_indicators: usize,
    pub meets_criteria: bool,
}

/// Internal: Candidate grouping (indices only)
#[derive(Debug, Clone)]
pub struct CandidateGrouping {
    pub groups: Vec<Vec<usize>>, // group_id -> [animal_indices]
}
