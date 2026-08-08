//! Sequential covariate-adaptive minimization (Pocock-Simon).
//!
//! Animals enter one at a time in a seeded order. For each one the engine asks, for every
//! group it could still go to, "how imbalanced would the covariate tables be if it went
//! here", and sends it to a group that minimizes that — with probability `p`. With the
//! remaining `1 - p` it goes to one of the *other* groups, which is what keeps the method
//! on the randomization side of the line: the allocation is never fully predictable from
//! the data alone.
//!
//! Three properties are load-bearing and each has a test:
//!
//! * **Covariates are binned inside each sex stratum.** Binning globally would put every
//!   male in the top tertile and every female in the bottom one for any indicator where
//!   the sexes differ, and since the eligible groups are already filtered by sex, the
//!   covariate would then carry no information at all — the method would silently
//!   degrade to complete randomization.
//! * **The imbalance measure is normalized by each group's quota.** Raw counts assume
//!   equal allocation; with a 20/10 split they would hold both groups level until the
//!   small one fills and then dump the whole tail into the large one.
//! * **Exactly two uniforms are consumed per animal, whatever the branch.** The stream
//!   position is part of the reproduction contract, so skipping a draw when the outcome
//!   is forced would shift every later animal.

use super::{enumerator, evaluator, randomizer};
use crate::core::models::*;
use anyhow::{bail, Result};
use rand::seq::SliceRandom;
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;
use std::cmp::Ordering;
use std::collections::HashMap;

/// Identifier of the imbalance measure, written into every record so an archived run
/// still says which formula produced it after a later version adds another.
pub const IMBALANCE_MEASURE: &str = "quota-normalized-range";

/// Identifier of the allocation rule: the `1 - p` probability mass goes to the
/// non-minimizers, which is what makes the recorded `p` the actual probability of
/// landing on a minimizer rather than `p + (1 - p) / k`.
pub const ALLOCATION_RULE: &str = "minimizer-or-uniform-over-others";

/// Scores within this distance count as tied.
///
/// The measure is a sum of small rationals, so exact equality would mostly work — but
/// "mostly" is not good enough here: an undetected tie collapses the uniform pick onto
/// the lowest-numbered group, which would quietly bias which group becomes the control.
const SCORE_EPSILON: f64 = 1e-9;

const SEXES: [Sex; 2] = [Sex::Male, Sex::Female];

fn sex_index(sex: Sex) -> usize {
    match sex {
        Sex::Male => 0,
        Sex::Female => 1,
    }
}

/// Dispatch target for `Minimization`.
pub fn compute_minimization_grouping(
    dataset: Dataset,
    group_config: GroupConfig,
    stat_config: StatConfig,
) -> Result<MultiGroupingResult> {
    let start_time = std::time::Instant::now();

    let rand_config = group_config.randomization.clone().unwrap_or_default();
    enumerator::validate_config(&dataset.animals, &group_config)?;
    randomizer::validate_randomization(&dataset, &group_config, &rand_config)?;

    let min_config = rand_config
        .minimization
        .as_ref()
        .expect("validated above: Minimization carries its parameters");

    let order = randomizer::normalized_order(&dataset.animals);
    let input_fingerprint = randomizer::dataset_fingerprint(&dataset.animals);

    let base_seed = rand_config
        .seed
        .unwrap_or_else(|| rand::thread_rng().gen_range(0..=randomizer::MAX_SEED));
    let seed = randomizer::derive_draw_seed(base_seed, rand_config.draw_index);
    let mut rng = ChaCha12Rng::seed_from_u64(seed);

    let covariates: Vec<BinnedCovariate> = min_config
        .covariates
        .iter()
        .map(|key| BinnedCovariate::build(key, &dataset.animals))
        .collect::<Result<_>>()?;

    // The shuffle comes off the same stream as the allocation draws, and it starts from
    // the normalized order rather than the Excel row order — otherwise re-sorting the
    // input file would move the result under a fixed seed.
    let mut entry_order = order;
    entry_order.shuffle(&mut rng);

    let allocation = allocate(
        &dataset,
        &group_config,
        &covariates,
        &entry_order,
        min_config.allocation_probability,
        &mut rng,
    )?;

    let mut result = evaluator::evaluate_grouping_with_constraints(
        &allocation.candidate,
        &dataset,
        &stat_config,
        Some(&group_config.sex_constraints),
        evaluator::Untestable::Skip,
    )?;

    let index_of: HashMap<&str, usize> = dataset
        .animals
        .iter()
        .enumerate()
        .map(|(idx, animal)| (animal.id.as_str(), idx))
        .collect();

    for assignment in &mut result.assignments {
        if let Some(&idx) = index_of.get(assignment.animal_id.as_str()) {
            assignment.entry_index = Some(allocation.entry_index_of[idx]);
        }
    }

    result.computation_time_ms = start_time.elapsed().as_millis() as u64;
    result.method = group_config.method;
    result.randomization = Some(RandomizationRecord {
        seed,
        base_seed,
        draw_index: rand_config.draw_index,
        rng_algorithm: randomizer::RNG_ALGORITHM.to_string(),
        input_fingerprint,
        engine_version: randomizer::engine_version(),
        // Minimization allocates in a single pass; there is nothing to reject and retry.
        attempts: 1,
        acceptance: None,
        primary_indicator: None,
        block_size: None,
        incomplete_last_block: false,
        calibrated_threshold: None,
        calibration_draws: None,
        minimization: Some(MinimizationRecord {
            covariates: min_config.covariates.clone(),
            binning: min_config.binning.as_str().to_string(),
            bins: bins_record(&covariates, &dataset.animals),
            allocation_probability: min_config.allocation_probability,
            imbalance_measure: IMBALANCE_MEASURE.to_string(),
            allocation_rule: ALLOCATION_RULE.to_string(),
            decisions: allocation.decisions,
        }),
    });

    let meets_criteria = result.summary.meets_criteria;

    Ok(MultiGroupingResult {
        candidates: vec![result],
        // One allocation, not a search space: minimization neither enumerates nor
        // resamples, so these counts describe a single pass.
        total_evaluated: 1,
        total_valid: usize::from(meets_criteria),
        computation_time_ms: start_time.elapsed().as_millis() as u64,
    })
}

/// One covariate turned into levels, plus the cut points that produced them.
struct BinnedCovariate {
    key: String,
    /// Level index per dataset animal index, within that animal's own sex stratum.
    level_of: Vec<usize>,
    /// Level count per sex index; 0 for a sex with no animals.
    levels_per_sex: [usize; 2],
    cuts_per_sex: [Vec<f64>; 2],
}

impl BinnedCovariate {
    fn build(key: &str, animals: &[Animal]) -> Result<Self> {
        let mut level_of = vec![0usize; animals.len()];
        let mut levels_per_sex = [0usize; 2];
        let mut cuts_per_sex = [Vec::new(), Vec::new()];

        for sex in SEXES {
            let si = sex_index(sex);
            let mut values: Vec<f64> = Vec::new();
            for animal in animals.iter().filter(|a| a.sex == sex) {
                match animal.indicators.get(key) {
                    Some(value) if value.is_finite() => values.push(*value),
                    // Both cases are rejected by `validate_randomization` before this
                    // point; reaching here means the two drifted apart.
                    _ => bail!("协变量「{key}」在动物「{}」上没有可用数值。", animal.id),
                }
            }

            if values.is_empty() {
                continue;
            }

            values.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));
            let cuts = tertile_cuts(&values);

            for (idx, animal) in animals.iter().enumerate() {
                if animal.sex != sex {
                    continue;
                }
                let value = animal.indicators[key];
                level_of[idx] = cuts.iter().filter(|&&cut| value > cut).count();
            }

            levels_per_sex[si] = cuts.len() + 1;
            cuts_per_sex[si] = cuts;
        }

        Ok(Self {
            key: key.to_string(),
            level_of,
            levels_per_sex,
            cuts_per_sex,
        })
    }

    /// Width of the per-sex level block inside the flattened cell table.
    fn stride(&self) -> usize {
        self.levels_per_sex
            .iter()
            .copied()
            .max()
            .unwrap_or(1)
            .max(1)
    }

    fn cells(&self) -> usize {
        SEXES.len() * self.stride()
    }

    /// Cell key: levels are per stratum, so the sex is part of the identity. Balancing
    /// "light males" is a different job from balancing "light females".
    fn cell(&self, sex: Sex, animal_idx: usize) -> usize {
        sex_index(sex) * self.stride() + self.level_of[animal_idx]
    }
}

/// Cut points for the tertiles of an ascending `sorted`, snapped to positions where the
/// value actually changes so that equal measurements always share a level.
///
/// Cutting on ranks instead would split two animals with identical body weight into
/// different strata, which is indefensible for a covariate and outright common for
/// discrete indicators (scores, counts, coarse scales).
fn tertile_cuts(sorted: &[f64]) -> Vec<f64> {
    let n = sorted.len();
    let mut cuts: Vec<f64> = Vec::new();

    for third in 1..=2 {
        let target = ((third * n) as f64 / 3.0).round() as usize;
        if let Some(cut) = boundary_near(sorted, target) {
            // Repeated values can push both tertile boundaries onto the same split. The
            // level count then drops to 2 (or 1), which is the honest answer for that
            // stratum rather than something to paper over.
            if !cuts.contains(&cut) {
                cuts.push(cut);
            }
        }
    }

    cuts
}

/// Midpoint of the split position closest to `target` at which the value changes.
fn boundary_near(sorted: &[f64], target: usize) -> Option<f64> {
    let n = sorted.len();
    if n < 2 {
        return None;
    }
    let target = target.clamp(1, n - 1);

    (1..n)
        .filter(|&split| sorted[split - 1] < sorted[split])
        .min_by_key(|&split| split.abs_diff(target))
        .map(|split| (sorted[split - 1] + sorted[split]) / 2.0)
}

fn bins_record(covariates: &[BinnedCovariate], animals: &[Animal]) -> Vec<CovariateBins> {
    let present = |sex: Sex| animals.iter().any(|a| a.sex == sex);

    covariates
        .iter()
        .map(|covariate| CovariateBins {
            covariate: covariate.key.clone(),
            strata: SEXES
                .into_iter()
                .filter(|&sex| present(sex))
                .map(|sex| {
                    let si = sex_index(sex);
                    CovariateStratumBins {
                        sex,
                        cut_points: covariate.cuts_per_sex[si].clone(),
                        levels: covariate.levels_per_sex[si],
                    }
                })
                .collect(),
        })
        .collect()
}

struct Allocation {
    candidate: CandidateGrouping,
    /// 1-based entry position per dataset animal index.
    entry_index_of: Vec<usize>,
    decisions: Vec<MinimizationDecision>,
}

/// The sequential pass itself.
fn allocate(
    dataset: &Dataset,
    group_config: &GroupConfig,
    covariates: &[BinnedCovariate],
    entry_order: &[usize],
    p: f64,
    rng: &mut ChaCha12Rng,
) -> Result<Allocation> {
    let num_groups = group_config.sex_constraints.len();

    let quotas: Vec<[usize; 2]> = group_config
        .sex_constraints
        .iter()
        .map(|c| [c.male_count, c.female_count])
        .collect();
    let is_experimental: Vec<bool> = group_config
        .sex_constraints
        .iter()
        .map(|c| c.group_type == GroupType::Experimental)
        .collect();

    let mut remaining = quotas.clone();
    let mut counts: Vec<Vec<Vec<usize>>> = covariates
        .iter()
        .map(|covariate| vec![vec![0usize; num_groups]; covariate.cells()])
        .collect();

    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); num_groups];
    let mut entry_index_of = vec![0usize; dataset.animals.len()];
    let mut decisions = Vec::with_capacity(entry_order.len());

    for (position, &animal_idx) in entry_order.iter().enumerate() {
        let sex = dataset.animals[animal_idx].sex;
        let si = sex_index(sex);

        // Experimental groups take precedence: a reserve group is where the overflow
        // goes once the study is fully staffed, not a competitor for the animals.
        let mut eligible: Vec<usize> = (0..num_groups).filter(|&g| remaining[g][si] > 0).collect();
        if eligible.iter().any(|&g| is_experimental[g]) {
            eligible.retain(|&g| is_experimental[g]);
        }
        if eligible.is_empty() {
            // Unreachable: quotas sum to the dataset per sex, checked by
            // `enumerator::validate_config` before the pass starts.
            bail!(
                "内部错误：动物「{}」没有可分配的组，配额与数据集不一致。",
                dataset.animals[animal_idx].id
            );
        }

        // The measure ranges over every experimental group that takes this sex at all,
        // not just the ones still open: a group that filled up early still carries the
        // imbalance it accumulated, and dropping it would hide exactly what the later
        // animals are there to correct.
        let measured: Vec<usize> = (0..num_groups)
            .filter(|&g| is_experimental[g] && quotas[g][si] > 0)
            .collect();

        let mut scores: Vec<Option<f64>> = vec![None; num_groups];
        for &g in &eligible {
            scores[g] = Some(imbalance_score(
                covariates, &counts, &quotas, &measured, sex, animal_idx, g,
            ));
        }

        let best = eligible
            .iter()
            .map(|&g| scores[g].expect("scored above"))
            .fold(f64::INFINITY, f64::min);
        let (minimizers, others): (Vec<usize>, Vec<usize>) = eligible
            .iter()
            .copied()
            .partition(|&g| scores[g].expect("scored above") <= best + SCORE_EPSILON);

        // Both draws are taken unconditionally. Consuming a variable number of uniforms
        // would make the stream position depend on the branch taken, and every animal
        // after the first forced choice would land somewhere else.
        let coin: f64 = rng.gen();
        let pick: f64 = rng.gen();

        let (group_id, took_minimizer) = choose_group(&minimizers, &others, p, coin, pick);

        for (covariate, cov_counts) in covariates.iter().zip(counts.iter_mut()) {
            cov_counts[covariate.cell(sex, animal_idx)][group_id] += 1;
        }
        remaining[group_id][si] -= 1;
        groups[group_id].push(animal_idx);
        entry_index_of[animal_idx] = position + 1;

        decisions.push(MinimizationDecision {
            entry_index: position + 1,
            animal_id: dataset.animals[animal_idx].id.clone(),
            levels: covariates
                .iter()
                .map(|covariate| covariate.level_of[animal_idx])
                .collect(),
            scores,
            took_minimizer,
            group_id,
        });
    }

    Ok(Allocation {
        candidate: CandidateGrouping { groups },
        entry_index_of,
        decisions,
    })
}

/// Pick the group for one animal from two pre-computed uniforms.
///
/// Split out from the pass so the branch can be tested at its extremes (`p = 1`, `p = 0`)
/// without going through configuration validation, which rejects both on purpose.
///
/// The `1 - p` mass goes to the *non*-minimizers. Spreading it over all eligible groups
/// instead — the obvious-looking simplification — would make the true probability of
/// landing on a minimizer `p + (1 - p) / k`, so a run declared at p = 0.8 with 3 groups
/// would actually be running at 0.867 and the exported parameter would be wrong.
fn choose_group(
    minimizers: &[usize],
    others: &[usize],
    p: f64,
    coin: f64,
    pick: f64,
) -> (usize, bool) {
    // With every group tied there is no "other" to fall back to, and a uniform pick over
    // the minimizers is the correct answer anyway — that is what makes the first animal's
    // group, and therefore which group becomes the control, genuinely random.
    let took_minimizer = coin < p || others.is_empty();
    let pool = if took_minimizer { minimizers } else { others };
    let index = ((pick * pool.len() as f64) as usize).min(pool.len() - 1);
    (pool[index], took_minimizer)
}

/// Imbalance after hypothetically sending this animal to `candidate`.
///
/// Only the cells the animal itself falls into are summed. Every other cell is untouched
/// by the assignment, so its range is the same number for every candidate group and
/// cannot change which group wins — summing over the whole table, as the first draft of
/// the design did, gives identical decisions at `levels` times the cost.
fn imbalance_score(
    covariates: &[BinnedCovariate],
    counts: &[Vec<Vec<usize>>],
    quotas: &[[usize; 2]],
    measured: &[usize],
    sex: Sex,
    animal_idx: usize,
    candidate: usize,
) -> f64 {
    if measured.len() < 2 {
        return 0.0;
    }
    let si = sex_index(sex);

    let mut total = 0.0;
    for (covariate, cov_counts) in covariates.iter().zip(counts) {
        let cell = covariate.cell(sex, animal_idx);

        let mut lowest = f64::INFINITY;
        let mut highest = f64::NEG_INFINITY;
        for &g in measured {
            let bump = usize::from(g == candidate);
            // Normalized by the group's quota *in this sex*, because the cell is
            // sex-specific: a group entitled to twice as many animals should be holding
            // twice as many of every level.
            let share = (cov_counts[cell][g] + bump) as f64 / quotas[g][si] as f64;
            lowest = lowest.min(share);
            highest = highest.max(share);
        }
        total += highest - lowest;
    }

    total
}

#[cfg(test)]
mod tests;
