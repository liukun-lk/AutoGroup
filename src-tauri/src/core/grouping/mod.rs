pub mod enumerator;
pub mod evaluator;
pub mod randomizer;

#[cfg(test)]
mod tests;

#[cfg(test)]
mod real_data_test;

#[cfg(test)]
mod perf_repro;

use crate::core::models::*;
use anyhow::{bail, Result};
use evaluator::{CandidateScore, EvalScratch};
use rand::SeedableRng;
use rand_chacha::ChaCha12Rng;
use rayon::prelude::*;

/// Seed for the Monte Carlo path of the optimization mode.
///
/// Optimization takes no seed from the user — it is not a randomized method — but its
/// candidate *sampling* above 500 000 combinations still has to be reproducible, or the
/// exported result cannot be recomputed. A fixed constant makes the whole mode a pure
/// function of its input again.
const OPTIMIZED_SAMPLING_SEED: u64 = 0x4155_546F_4772_7000; // "AUToGrp\0"

/// Entry point for every grouping run: validates the scenario/method combination, then
/// dispatches to the optimization or the randomization path.
pub fn compute_grouping(
    dataset: Dataset,
    group_config: GroupConfig,
    stat_config: StatConfig,
) -> Result<MultiGroupingResult> {
    validate_method_selection(&group_config)?;

    match group_config.method {
        GroupingMethod::Optimized => compute_optimal_grouping(dataset, group_config, stat_config),
        GroupingMethod::Minimization => {
            bail!("最小化法（序贯协变量自适应随机化）暂未实现，请选择其他分组方式。")
        }
        _ => randomizer::compute_random_grouping(dataset, group_config, stat_config),
    }
}

/// Enforce the scenario x method matrix in the core rather than only in the UI: this is
/// an IPC boundary, and a greyed-out button does not stop a hand-built request. A GLP
/// submission produced by ranking candidates on their P values is the one combination
/// that must never be reachable.
fn validate_method_selection(group_config: &GroupConfig) -> Result<()> {
    if !group_config.scenario.allows(group_config.method) {
        bail!(
            "「{}」场景下不能使用「{}」：它按 P 值择优挑选分组，不属于随机化，\
             写进申报材料与实际方法不符。请选择随机化类方法。",
            group_config.scenario.to_chinese(),
            group_config.method.to_chinese()
        );
    }

    if group_config.method == GroupingMethod::Optimized && group_config.randomization.is_some() {
        bail!("统计均衡优化不接受随机化参数（种子 / 主指标 / 接受准则）。");
    }

    if group_config.method.is_randomized() && group_config.randomization.is_none() {
        bail!("随机化方法必须提供随机化参数。");
    }

    Ok(())
}

pub fn compute_optimal_grouping(
    dataset: Dataset,
    group_config: GroupConfig,
    stat_config: StatConfig,
) -> Result<MultiGroupingResult> {
    let start_time = std::time::Instant::now();

    // Generate all candidate groupings (enumeration for ≤50 animals)
    let mut rng = ChaCha12Rng::seed_from_u64(OPTIMIZED_SAMPLING_SEED);
    let candidates = enumerator::enumerate_all(&dataset.animals, &group_config, &mut rng)?;
    let total_evaluated = candidates.len();

    // Pass 1: score every candidate in parallel.
    //
    // Only the ranking numbers are kept here. Materializing a full GroupingResult per
    // candidate (per-indicator statistics + post-hoc comparisons + assignments) costs
    // roughly 13 KB each, which for the 100k–500k candidates that ≥3 groups produce
    // means gigabytes of live heap — the machine starts swapping and the run never
    // appears to finish.
    let mut scored: Vec<(usize, CandidateScore)> = candidates
        .par_iter()
        .enumerate()
        .map_init(EvalScratch::default, |scratch, (idx, candidate)| {
            let score = evaluator::score_candidate(
                candidate,
                &dataset,
                &stat_config,
                Some(&group_config.sex_constraints),
                scratch,
                evaluator::Untestable::Abort,
            )
            .ok()?;

            score
                .meets_criteria(stat_config.mode)
                .then_some((idx, score))
        })
        .flatten()
        .collect();

    let total_valid = scored.len();

    if scored.is_empty() {
        return Err(anyhow::anyhow!("No valid grouping found"));
    }

    // Sort by quality (descending): primary by min_p_value, secondary by mean_p_value.
    // Equally-scoring candidates are extremely common (swapping two same-sized groups
    // leaves every statistic untouched), so ties fall back to enumeration order to keep
    // the reported Top-N reproducible across runs.
    let max_candidates = stat_config.max_candidates.max(1);
    let top_n = max_candidates.min(scored.len());
    scored.select_nth_unstable_by(top_n - 1, compare_entry);
    scored.truncate(top_n);
    scored.sort_unstable_by(compare_entry);

    // Pass 2: build the full result only for the winners.
    let top_candidates: Vec<GroupingResult> = scored
        .into_iter()
        .map(|(idx, _)| {
            evaluator::evaluate_grouping_with_constraints(
                &candidates[idx],
                &dataset,
                &stat_config,
                Some(&group_config.sex_constraints),
                evaluator::Untestable::Abort,
            )
            .map(|mut result| {
                result.computation_time_ms = start_time.elapsed().as_millis() as u64;
                result
            })
        })
        .collect::<Result<_>>()?;

    let computation_time_ms = start_time.elapsed().as_millis() as u64;

    Ok(MultiGroupingResult {
        candidates: top_candidates,
        total_evaluated,
        total_valid,
        computation_time_ms,
    })
}

/// Ranking order over `(candidate_index, score)` entries: higher `min_p_value` first,
/// then higher `mean_p_value`, then enumeration order.
fn compare_entry(a: &(usize, CandidateScore), b: &(usize, CandidateScore)) -> std::cmp::Ordering {
    descending(a.1.min_p_value, b.1.min_p_value)
        .then_with(|| descending(a.1.mean_p_value, b.1.mean_p_value))
        .then_with(|| a.0.cmp(&b.0))
}

/// Descending comparison that stays a total order: NaN (degenerate indicators, e.g. a
/// zero-variance column in Optimized mode) always ranks last instead of poisoning the sort.
fn descending(a: f64, b: f64) -> std::cmp::Ordering {
    use std::cmp::Ordering;
    match (a.is_nan(), b.is_nan()) {
        (true, true) => Ordering::Equal,
        (true, false) => Ordering::Greater,
        (false, true) => Ordering::Less,
        (false, false) => b.partial_cmp(&a).unwrap_or(Ordering::Equal),
    }
}
