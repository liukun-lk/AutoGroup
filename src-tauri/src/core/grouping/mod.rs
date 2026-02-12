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
) -> Result<GroupingResult> {
    let start_time = std::time::Instant::now();

    // Generate all candidate groupings (enumeration for ≤50 animals)
    let candidates = enumerator::enumerate_all(&dataset.animals, &group_config)?;

    // Evaluate candidates in parallel
    let evaluated: Vec<_> = candidates
        .par_iter()
        .filter_map(|candidate| {
            evaluator::evaluate_grouping(candidate, &dataset, &stat_config).ok()
        })
        .collect();

    // Select best grouping based on mode
    let best = evaluated
        .into_iter()
        .filter(|result| match stat_config.mode {
            OptimizationMode::Strict => result.summary.num_invalid_indicators == 0,
            OptimizationMode::Optimized => result.summary.num_invalid_indicators <= 1,
        })
        .max_by(|a, b| {
            // Primary: max(min_p_value)
            let cmp = a
                .summary
                .min_p_value
                .partial_cmp(&b.summary.min_p_value)
                .unwrap();
            if cmp == std::cmp::Ordering::Equal {
                // Secondary: max(mean_p_value)
                a.summary
                    .mean_p_value
                    .partial_cmp(&b.summary.mean_p_value)
                    .unwrap()
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
