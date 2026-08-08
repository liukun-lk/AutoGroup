use crate::core::{models::*, stats};
use anyhow::Result;

/// Ranking-relevant summary of one candidate grouping.
///
/// Deliberately allocation-free: the engine computes one of these for every candidate
/// (up to hundreds of thousands per run) and only materializes the full
/// [`GroupingResult`] for the handful of candidates that actually win.
#[derive(Debug, Clone, Copy)]
pub struct CandidateScore {
    pub min_p_value: f64,
    pub mean_p_value: f64,
    pub num_invalid_indicators: usize,
    pub total_indicators: usize,
}

impl CandidateScore {
    pub fn meets_criteria(&self, mode: OptimizationMode) -> bool {
        match mode {
            OptimizationMode::Strict => self.num_invalid_indicators == 0,
            OptimizationMode::Optimized => self.num_invalid_indicators <= 1,
        }
    }
}

/// Scratch buffers reused across candidates to keep the hot loop allocation-free.
#[derive(Default)]
pub struct EvalScratch {
    groups: Vec<Vec<f64>>,
    posthoc: Vec<(usize, usize, f64)>,
}

/// What to do with an indicator whose test cannot be computed on this particular split —
/// a group with zero variance leaves Welch's ANOVA undefined, for instance.
///
/// Optimization can afford [`Untestable::Abort`]: it has hundreds of thousands of other
/// candidates and simply drops this one. Randomization cannot — the draw is the
/// allocation, made without consulting that indicator in the first place — so it skips
/// the indicator exactly like one with insufficient data. Either way `total_indicators`
/// counts what was actually tested, and a caller reporting a pass rate has to compare it
/// against the number of indicators the user selected.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum Untestable {
    Abort,
    Skip,
}

/// Score a candidate without building any per-indicator detail.
pub fn score_candidate(
    candidate: &CandidateGrouping,
    dataset: &Dataset,
    stat_config: &StatConfig,
    group_constraints: Option<&[SexConstraint]>,
    scratch: &mut EvalScratch,
    untestable: Untestable,
) -> Result<CandidateScore> {
    run_indicator_tests(
        candidate,
        dataset,
        stat_config,
        group_constraints,
        scratch,
        None,
        untestable,
    )
}

/// Evaluate a candidate grouping by computing statistical tests
pub fn evaluate_grouping(
    candidate: &CandidateGrouping,
    dataset: &Dataset,
    stat_config: &StatConfig,
) -> Result<GroupingResult> {
    evaluate_grouping_with_constraints(candidate, dataset, stat_config, None, Untestable::Abort)
}

/// Evaluate a candidate grouping with group constraints
/// If group_constraints is provided, reserve groups are excluded from statistics
pub fn evaluate_grouping_with_constraints(
    candidate: &CandidateGrouping,
    dataset: &Dataset,
    stat_config: &StatConfig,
    group_constraints: Option<&[SexConstraint]>,
    untestable: Untestable,
) -> Result<GroupingResult> {
    let mut scratch = EvalScratch::default();
    let mut statistics = Vec::with_capacity(stat_config.selected_indicators.len());

    let score = run_indicator_tests(
        candidate,
        dataset,
        stat_config,
        group_constraints,
        &mut scratch,
        Some(&mut statistics),
        untestable,
    )?;

    // Convert candidate to assignments
    let mut assignments = Vec::new();
    for (group_id, animal_indices) in candidate.groups.iter().enumerate() {
        for &idx in animal_indices {
            let animal = &dataset.animals[idx];
            assignments.push(GroupAssignment {
                animal_id: animal.id.clone(),
                sex: animal.sex,
                group_id,
                // Filled in by the path that produced the candidate; the evaluator has no
                // per-animal provenance of its own to report.
                random_number: None,
                block_index: None,
                entry_index: None,
            });
        }
    }

    let num_experimental_groups = match group_constraints {
        Some(constraints) => constraints
            .iter()
            .filter(|c| c.group_type == GroupType::Experimental)
            .count(),
        None => candidate.groups.len(),
    };

    Ok(GroupingResult {
        assignments,
        statistics,
        summary: ResultSummary {
            min_p_value: score.min_p_value,
            mean_p_value: score.mean_p_value,
            num_invalid_indicators: score.num_invalid_indicators,
            meets_criteria: score.meets_criteria(stat_config.mode),
            total_animals: dataset.animals.len(),
            num_groups: num_experimental_groups,
            passed_indicators: score.total_indicators - score.num_invalid_indicators,
            total_indicators: score.total_indicators,
        },
        computation_time_ms: 0, // Will be set by caller
        // Both are set by the caller that knows how the candidate was produced; the
        // evaluator itself is method-agnostic.
        method: GroupingMethod::default(),
        randomization: None,
    })
}

/// Shared test cascade driver for both the scoring pass and the full evaluation.
///
/// Keeping a single implementation guarantees that the numbers used for ranking are
/// exactly the numbers later reported for the winning candidates.
fn run_indicator_tests(
    candidate: &CandidateGrouping,
    dataset: &Dataset,
    stat_config: &StatConfig,
    group_constraints: Option<&[SexConstraint]>,
    scratch: &mut EvalScratch,
    mut collect: Option<&mut Vec<IndicatorStats>>,
    untestable: Untestable,
) -> Result<CandidateScore> {
    let mut min_p = f64::MAX;
    let mut sum_p = 0.0;
    let mut num_invalid = 0;
    let mut total_indicators = 0;

    // Groups that participate in the statistics (reserve groups are excluded).
    // Groups without a matching constraint default to experimental.
    //
    // The original `group_id` is kept alongside each group: the statistics see a compacted
    // list, so a post-hoc comparison between experimental groups 0 and 1 may refer to
    // `group_id` 0 and 2 once a reserve group sits between them. Reporting the compacted
    // index would mislabel every comparison in that layout.
    let experimental: Vec<(usize, &Vec<usize>)> = candidate
        .groups
        .iter()
        .enumerate()
        .filter(|(group_idx, _)| match group_constraints {
            Some(constraints) => constraints
                .iter()
                .find(|c| c.group_index == *group_idx)
                .map(|c| c.group_type == GroupType::Experimental)
                .unwrap_or(true),
            None => true,
        })
        .collect();

    // One value buffer per experimental group, reused across indicators and candidates so
    // the hot loop stays allocation-free after the first candidate on each worker thread.
    if scratch.groups.len() != experimental.len() {
        scratch.groups.resize_with(experimental.len(), Vec::new);
    }

    // Exact post-hoc p-values are only needed for candidates whose detail is actually
    // reported; the scoring pass just needs to know whether they all clear alpha.
    let detail = if collect.is_some() {
        stats::PostHocDetail::Exact
    } else {
        stats::PostHocDetail::ValidityOnly
    };

    // For each selected indicator, compute P-value
    for indicator_name in &stat_config.selected_indicators {
        // Extract indicator values for each experimental group
        for (buffer, (_, animal_indices)) in scratch.groups.iter_mut().zip(&experimental) {
            buffer.clear();
            buffer.extend(
                animal_indices.iter().filter_map(|&idx| {
                    dataset.animals[idx].indicators.get(indicator_name).copied()
                }),
            );
        }

        // Skip if any group has insufficient data
        if scratch.groups.iter().any(|g| g.len() < 2) {
            continue;
        }

        let test = match stats::compute_indicator_test(
            &scratch.groups,
            stat_config.alpha,
            detail,
            &mut scratch.posthoc,
        ) {
            Ok(test) => test,
            Err(_) if untestable == Untestable::Skip => continue,
            Err(e) => return Err(e),
        };

        // Strict criterion: main test AND every pairwise post-hoc comparison must have P > alpha
        let is_valid = test.diff_p_value > stat_config.alpha && test.posthoc_all_valid;

        if !is_valid {
            num_invalid += 1;
        }

        if test.diff_p_value < min_p {
            min_p = test.diff_p_value;
        }
        sum_p += test.diff_p_value;
        total_indicators += 1;

        if let Some(statistics) = collect.as_deref_mut() {
            let posthoc_results = if scratch.posthoc.is_empty() {
                None
            } else {
                Some(
                    scratch
                        .posthoc
                        .iter()
                        .map(|&(g1, g2, p)| PostHocComparison {
                            group1_id: experimental[g1].0,
                            group2_id: experimental[g2].0,
                            p_value: p,
                            is_valid: p > stat_config.alpha,
                        })
                        .collect(),
                )
            };

            statistics.push(IndicatorStats {
                indicator_name: indicator_name.clone(),
                levene_p_value: test.levene_p_value,
                diff_p_value: test.diff_p_value,
                test_method: test.method.as_str().to_string(),
                is_valid,
                posthoc_results,
            });
        }
    }

    let mean_p = if total_indicators > 0 {
        sum_p / total_indicators as f64
    } else {
        0.0
    };

    Ok(CandidateScore {
        min_p_value: min_p,
        mean_p_value: mean_p,
        num_invalid_indicators: num_invalid,
        total_indicators,
    })
}
