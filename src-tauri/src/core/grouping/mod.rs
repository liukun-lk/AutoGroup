pub mod enumerator;
pub mod evaluator;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod real_data_test;

use crate::core::models::*;
use anyhow::Result;
use rayon::prelude::*;

pub fn compute_optimal_grouping(
    dataset: Dataset,
    group_config: GroupConfig,
    stat_config: StatConfig,
) -> Result<MultiGroupingResult> {
    let start_time = std::time::Instant::now();

    // Generate all candidate groupings (enumeration for ≤50 animals)
    let candidates = enumerator::enumerate_all(&dataset.animals, &group_config)?;
    let total_evaluated = candidates.len();

    // Evaluate candidates in parallel
    let evaluated: Vec<_> = candidates
        .par_iter()
        .filter_map(|candidate| {
            evaluator::evaluate_grouping(candidate, &dataset, &stat_config).ok()
        })
        .collect();

    // Filter valid candidates based on mode
    let mut valid_candidates: Vec<GroupingResult> = evaluated
        .into_iter()
        .filter(|result| match stat_config.mode {
            OptimizationMode::Strict => result.summary.num_invalid_indicators == 0,
            OptimizationMode::Optimized => result.summary.num_invalid_indicators <= 1,
        })
        .collect();

    let total_valid = valid_candidates.len();

    if valid_candidates.is_empty() {
        return Err(anyhow::anyhow!("No valid grouping found"));
    }

    // Sort by quality (descending): primary by min_p_value, secondary by mean_p_value
    valid_candidates.sort_by(|a, b| {
        let cmp = b
            .summary
            .min_p_value
            .partial_cmp(&a.summary.min_p_value)
            .unwrap();
        if cmp == std::cmp::Ordering::Equal {
            b.summary
                .mean_p_value
                .partial_cmp(&a.summary.mean_p_value)
                .unwrap()
        } else {
            cmp
        }
    });

    // Take top N candidates
    let max_candidates = stat_config.max_candidates;
    let top_candidates: Vec<GroupingResult> = valid_candidates
        .into_iter()
        .take(max_candidates)
        .map(|mut result| {
            // Set computation_time_ms for each candidate
            result.computation_time_ms = start_time.elapsed().as_millis() as u64;
            result
        })
        .collect();

    let computation_time_ms = start_time.elapsed().as_millis() as u64;

    Ok(MultiGroupingResult {
        candidates: top_candidates,
        total_evaluated,
        total_valid,
        computation_time_ms,
    })
}
