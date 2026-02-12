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
        let (levene_p_value, diff_p_value, test_method, posthoc_results) = stats::compute_p_value(&groups, stat_config.alpha)?;

        // For multi-group (≥3), check post-hoc results
        let mut is_valid = diff_p_value > stat_config.alpha;
        let mut posthoc_comparisons = None;

        if let Some(posthoc) = posthoc_results {
            // Convert to PostHocComparison and check if all pairwise comparisons pass
            let comparisons: Vec<PostHocComparison> = posthoc
                .iter()
                .map(|(g1, g2, p)| PostHocComparison {
                    group1_id: *g1,
                    group2_id: *g2,
                    p_value: *p,
                    is_valid: *p > stat_config.alpha,
                })
                .collect();

            // Strict criterion: ALL pairwise comparisons must have P > α
            let all_posthoc_valid = comparisons.iter().all(|c| c.is_valid);
            is_valid = is_valid && all_posthoc_valid;

            posthoc_comparisons = Some(comparisons);
        }

        if !is_valid {
            num_invalid += 1;
        }

        if diff_p_value < min_p {
            min_p = diff_p_value;
        }
        sum_p += diff_p_value;

        statistics.push(IndicatorStats {
            indicator_name: indicator_name.clone(),
            levene_p_value,
            diff_p_value,
            test_method,
            is_valid,
            posthoc_results: posthoc_comparisons,
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

    let total_indicators = statistics.len();
    let passed_indicators = total_indicators - num_invalid;

    Ok(GroupingResult {
        assignments,
        statistics,
        summary: ResultSummary {
            min_p_value: min_p,
            mean_p_value: mean_p,
            num_invalid_indicators: num_invalid,
            meets_criteria,
            total_animals: dataset.animals.len(),
            num_groups: candidate.groups.len(),
            passed_indicators,
            total_indicators,
        },
        computation_time_ms: 0, // Will be set by caller
    })
}
