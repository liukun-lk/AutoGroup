use crate::core::{models::*, stats};
use anyhow::Result;

/// Evaluate a candidate grouping by computing statistical tests
pub fn evaluate_grouping(
    candidate: &CandidateGrouping,
    dataset: &Dataset,
    stat_config: &StatConfig,
) -> Result<GroupingResult> {
    let mut statistics = Vec::new();
    let mut min_p = f64::MAX;
    let mut sum_p = 0.0;
    let mut num_invalid = 0;

    // For each selected indicator, compute P-value
    for indicator_name in &stat_config.selected_indicators {
        // Extract indicator values for each group
        let groups: Vec<Vec<f64>> = candidate
            .groups
            .iter()
            .map(|animal_indices| {
                animal_indices
                    .iter()
                    .filter_map(|&idx| {
                        dataset.animals[idx]
                            .indicators
                            .get(indicator_name)
                            .copied()
                    })
                    .collect()
            })
            .collect();

        // Skip if any group has insufficient data
        if groups.iter().any(|g| g.len() < 2) {
            continue;
        }

        // Compute P-value using appropriate statistical test
        let (p_value, test_method) = stats::compute_p_value(&groups, stat_config.alpha)?;

        let is_valid = p_value > stat_config.alpha;
        if !is_valid {
            num_invalid += 1;
        }

        if p_value < min_p {
            min_p = p_value;
        }
        sum_p += p_value;

        statistics.push(IndicatorStats {
            indicator_name: indicator_name.clone(),
            p_value,
            test_method,
            is_valid,
        });
    }

    let mean_p = if !statistics.is_empty() {
        sum_p / statistics.len() as f64
    } else {
        0.0
    };

    let meets_criteria = match stat_config.mode {
        OptimizationMode::Strict => num_invalid == 0,
        OptimizationMode::Optimized => num_invalid <= 1,
    };

    // Convert candidate to assignments
    let mut assignments = Vec::new();
    for (group_id, animal_indices) in candidate.groups.iter().enumerate() {
        for &idx in animal_indices {
            let animal = &dataset.animals[idx];
            assignments.push(GroupAssignment {
                animal_id: animal.id.clone(),
                sex: animal.sex,
                group_id,
            });
        }
    }

    Ok(GroupingResult {
        assignments,
        statistics,
        summary: ResultSummary {
            min_p_value: min_p,
            mean_p_value: mean_p,
            num_invalid_indicators: num_invalid,
            meets_criteria,
        },
        computation_time_ms: 0, // Will be set by caller
    })
}
