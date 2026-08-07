//! Seeded randomized allocation: complete randomization, constrained randomization
//! (rejection sampling on a baseline-balance criterion), and blocked randomization on a
//! primary indicator.
//!
//! What separates these from [`super::compute_optimal_grouping`] is that no candidate is
//! ever ranked by its P values. The primary indicator is read only to order animals into
//! blocks; the remaining indicators are read only to accept or reject a draw against a
//! rule declared before the draw happened.
//!
//! Reproducing a run needs four things, all recorded in [`RandomizationRecord`]: the
//! seed, the RNG algorithm (a seed alone pins nothing), the deterministic animal order
//! the shuffle started from, and a fingerprint of the input it ran on.

use super::{enumerator, evaluator};
use crate::core::models::*;
use anyhow::{anyhow, bail, Result};
use rand::{Rng, SeedableRng};
use rand_chacha::ChaCha12Rng;
use std::cmp::Ordering;

/// Identifier written into every record. `rand`'s own `StdRng` is documented as *not*
/// reproducible across library versions, so the algorithm is pinned here instead.
pub const RNG_ALGORITHM: &str = "chacha12";

pub fn engine_version() -> String {
    env!("CARGO_PKG_VERSION").to_string()
}

/// Dispatch target for `Random` / `ConstrainedRandom` / `BlockedRandom`.
pub fn compute_random_grouping(
    dataset: Dataset,
    group_config: GroupConfig,
    stat_config: StatConfig,
) -> Result<MultiGroupingResult> {
    let start_time = std::time::Instant::now();

    let rand_config = group_config.randomization.clone().unwrap_or_default();
    enumerator::validate_config(&dataset.animals, &group_config)?;
    validate_randomization(&dataset, &group_config, &rand_config)?;

    let order = normalized_order(&dataset.animals);
    let input_fingerprint = fingerprint_of(&dataset.animals, &order);

    // A caller-supplied seed is the audit trail; a generated one is echoed back in the
    // record so the run stays reproducible either way.
    let seed = rand_config
        .seed
        .unwrap_or_else(|| rand::thread_rng().gen::<u64>());
    let mut rng = ChaCha12Rng::seed_from_u64(seed);

    let plan = build_plan(&dataset, &group_config, &rand_config, &order)?;

    let max_attempts = if rand_config.enforce_criteria {
        rand_config.max_attempts.max(1)
    } else {
        1
    };

    let mut scratch = evaluator::EvalScratch::default();
    let mut observed_min_p: Vec<f64> = Vec::new();
    let mut accepted: Option<(Draw, usize)> = None;
    let mut last_rejected: Option<CandidateGrouping> = None;

    for attempt in 1..=max_attempts {
        let draw = plan.draw(&mut rng);

        if !rand_config.enforce_criteria {
            accepted = Some((draw, attempt));
            break;
        }

        let score = evaluator::score_candidate(
            &draw.candidate,
            &dataset,
            &stat_config,
            Some(&group_config.sex_constraints),
            &mut scratch,
            evaluator::Untestable::Skip,
        )?;
        observed_min_p.push(score.min_p_value);

        if score.meets_criteria(stat_config.mode) {
            accepted = Some((draw, attempt));
            break;
        }
        last_rejected = Some(draw.candidate);
    }

    // Never degrade silently: a run that could not satisfy its own declared acceptance
    // criterion has to say so, and say which indicator blocked it, so the user can decide
    // between relaxing the criterion and dropping it. Picking one for them would put the
    // exported method description at odds with what actually ran.
    let (draw, attempts) = match accepted {
        Some(pair) => pair,
        None => {
            return Err(acceptance_failure(
                &dataset,
                &group_config,
                &stat_config,
                last_rejected.as_ref(),
                &observed_min_p,
                max_attempts,
            ))
        }
    };

    let mut result = evaluator::evaluate_grouping_with_constraints(
        &draw.candidate,
        &dataset,
        &stat_config,
        Some(&group_config.sex_constraints),
        evaluator::Untestable::Skip,
    )?;

    // Carry the accepted draw's numbers onto the assignments. It has to be *this* draw's
    // numbers, not the last one taken: under rejection sampling they are different.
    let index_of: std::collections::HashMap<&str, usize> = dataset
        .animals
        .iter()
        .enumerate()
        .map(|(idx, animal)| (animal.id.as_str(), idx))
        .collect();

    for assignment in &mut result.assignments {
        if let Some(&idx) = index_of.get(assignment.animal_id.as_str()) {
            assignment.random_number = Some(draw.randoms[idx]);
            assignment.block_index = draw.blocks[idx];
        }
    }

    result.computation_time_ms = start_time.elapsed().as_millis() as u64;
    result.method = group_config.method;
    result.randomization = Some(RandomizationRecord {
        seed,
        rng_algorithm: RNG_ALGORITHM.to_string(),
        input_fingerprint,
        engine_version: engine_version(),
        attempts,
        enforce_criteria: rand_config.enforce_criteria,
        primary_indicator: rand_config.primary_indicator.clone(),
        block_size: plan.block_size,
        incomplete_last_block: plan.incomplete_last_block,
    });

    let meets_criteria = result.summary.meets_criteria;

    Ok(MultiGroupingResult {
        candidates: vec![result],
        // Randomization draws once (or until accepted); these counts describe the draws
        // consumed, not a search space, and the UI has to word them accordingly.
        total_evaluated: attempts,
        total_valid: usize::from(meets_criteria),
        computation_time_ms: start_time.elapsed().as_millis() as u64,
    })
}

/// Reject illegal method/parameter combinations before any allocation happens.
///
/// This runs in the core rather than only at the IPC boundary: "blocked by an indicator"
/// with no indicator named would otherwise travel all the way down to the blocking code.
pub fn validate_randomization(
    dataset: &Dataset,
    group_config: &GroupConfig,
    rand_config: &RandomizationConfig,
) -> Result<()> {
    match group_config.method {
        GroupingMethod::Random => {
            if rand_config.primary_indicator.is_some() {
                bail!("完全随机不使用主指标，请改选「按主指标分层随机」或清除主指标。");
            }
            if rand_config.enforce_criteria {
                bail!("完全随机不带接受准则，启用接受准则请改选「受限随机化」。");
            }
        }
        GroupingMethod::ConstrainedRandom => {
            if rand_config.primary_indicator.is_some() {
                bail!("受限随机化不按指标分层，请改选「按主指标分层随机」或清除主指标。");
            }
            if !rand_config.enforce_criteria {
                bail!("受限随机化必须启用接受准则。");
            }
        }
        GroupingMethod::BlockedRandom => {
            let key = rand_config
                .primary_indicator
                .as_deref()
                .ok_or_else(|| anyhow!("按主指标分层随机必须指定主指标。"))?;

            if !dataset.indicator_names.iter().any(|name| name == key) {
                bail!("主指标「{key}」不在本次数据的指标列中。");
            }

            let missing: Vec<&str> = dataset
                .animals
                .iter()
                .filter(|a| !a.indicators.contains_key(key))
                .map(|a| a.id.as_str())
                .collect();

            if !missing.is_empty() {
                // Blocking positions each animal by its primary value; an animal without
                // one has no block to sit in, and quietly parking it in the last block
                // would break the balance the method exists to guarantee.
                bail!(
                    "主指标「{}」缺失 {} 只动物的数值（如 {}），无法按其分层。请更换主指标或补齐数据。",
                    key,
                    missing.len(),
                    missing.iter().take(5).copied().collect::<Vec<_>>().join("、")
                );
            }
        }
        GroupingMethod::Optimized => {
            bail!("统计均衡优化不走随机化路径。");
        }
        GroupingMethod::Minimization => {
            bail!("最小化法（序贯协变量自适应随机化）暂未实现。");
        }
    }

    if rand_config.enforce_criteria && rand_config.max_attempts == 0 {
        bail!("启用接受准则时，最大抽样次数必须至少为 1。");
    }

    Ok(())
}

/// Deterministic animal order the shuffle starts from.
///
/// `shuffle` permutes whatever order it is handed, so the Excel row order would
/// otherwise leak into the result: the same seed on a re-sorted copy of the same file
/// would produce a different allocation.
pub fn normalized_order(animals: &[Animal]) -> Vec<usize> {
    let mut order: Vec<usize> = (0..animals.len()).collect();
    order.sort_by(|&a, &b| compare_ids(&animals[a].id, &animals[b].id));
    order
}

/// Fingerprint of the normalized animal id sequence — the "same input" test used when
/// checking years later that a recorded seed reproduces a recorded allocation.
pub fn dataset_fingerprint(animals: &[Animal]) -> String {
    fingerprint_of(animals, &normalized_order(animals))
}

fn fingerprint_of(animals: &[Animal], order: &[usize]) -> String {
    // FNV-1a. This identifies an input, it does not protect one, so a short
    // dependency-free hash is the right size of tool.
    let mut hash: u64 = 0xcbf2_9ce4_8422_2325;
    for &idx in order {
        for byte in animals[idx].id.as_bytes().iter().chain(b"\n".iter()) {
            hash ^= u64::from(*byte);
            hash = hash.wrapping_mul(0x0000_0100_0000_01b3);
        }
    }
    format!("{hash:016x}")
}

/// Numeric prefixes compare numerically, so `2` sorts before `10`; anything else is
/// lexicographic, with the raw string as the final tiebreak to keep the order total.
fn compare_ids(a: &str, b: &str) -> Ordering {
    match (numeric_prefix(a), numeric_prefix(b)) {
        (Some(x), Some(y)) => x.cmp(&y).then_with(|| a.cmp(b)),
        (Some(_), None) => Ordering::Less,
        (None, Some(_)) => Ordering::Greater,
        (None, None) => a.cmp(b),
    }
}

fn numeric_prefix(s: &str) -> Option<u128> {
    let digits: String = s.chars().take_while(char::is_ascii_digit).collect();
    digits.parse().ok()
}

/// One sex stratum: the animals it holds, in deal order, and each group's quota in it.
struct Stratum {
    ordered: Vec<usize>,
    quotas: Vec<usize>,
    blocks: usize,
}

/// The full allocation plan for one run. Built once; every draw replays it against a
/// fresh stretch of the seeded stream.
struct Plan {
    strata: Vec<Stratum>,
    /// Every animal, in the normalized order the draw is taken in.
    draw_order: Vec<usize>,
    num_animals: usize,
    num_groups: usize,
    /// Reported block size. With more than one sex stratum the strata can block
    /// differently, in which case the largest is reported.
    block_size: Option<usize>,
    incomplete_last_block: bool,
    /// Blocking is in play, so the block a number was sorted within is part of the record.
    blocked: bool,
}

/// One draw, together with the numbers that produced it.
struct Draw {
    candidate: CandidateGrouping,
    /// Indexed by dataset animal index.
    randoms: Vec<f64>,
    blocks: Vec<Option<usize>>,
}

impl Plan {
    /// Hand every animal a uniform draw, then inside each block sort by it and deal each
    /// group its quota in turn.
    ///
    /// Sorting iid uniforms yields a uniform random permutation, so this is distributionally
    /// the same as shuffling the block — but it leaves behind a per-animal number that *is*
    /// the allocation rule rather than a by-product of it, which is what makes the exported
    /// column checkable by hand.
    fn draw(&self, rng: &mut ChaCha12Rng) -> Draw {
        let mut randoms = vec![0.0f64; self.num_animals];
        for &idx in &self.draw_order {
            randoms[idx] = rng.gen::<f64>();
        }

        let mut groups: Vec<Vec<usize>> = vec![Vec::new(); self.num_groups];
        let mut blocks: Vec<Option<usize>> = vec![None; self.num_animals];

        for stratum in &self.strata {
            let dealt = deal_blocks(stratum, &randoms, self.blocked, &mut blocks);
            for (group, animals) in groups.iter_mut().zip(dealt) {
                group.extend(animals);
            }
        }

        Draw {
            candidate: CandidateGrouping { groups },
            randoms,
            blocks,
        }
    }
}

fn build_plan(
    dataset: &Dataset,
    group_config: &GroupConfig,
    rand_config: &RandomizationConfig,
    order: &[usize],
) -> Result<Plan> {
    let num_groups = group_config.sex_constraints.len();
    let primary = match group_config.method {
        GroupingMethod::BlockedRandom => rand_config.primary_indicator.as_deref(),
        _ => None,
    };

    let mut strata = Vec::new();
    let mut block_size: Option<usize> = None;
    let mut incomplete_last_block = false;

    for sex in [Sex::Male, Sex::Female] {
        let quotas: Vec<usize> = group_config
            .sex_constraints
            .iter()
            .map(|c| match sex {
                Sex::Male => c.male_count,
                Sex::Female => c.female_count,
            })
            .collect();

        let mut ordered: Vec<usize> = order
            .iter()
            .copied()
            .filter(|&idx| dataset.animals[idx].sex == sex)
            .collect();

        if ordered.is_empty() {
            continue;
        }

        // Blocks are cut along the primary indicator, so the sort has to happen before
        // the chunking. `sort_by` is stable, which is what makes tied values fall back to
        // the normalized id order rather than drifting between runs.
        if let Some(key) = primary {
            ordered.sort_by(|&a, &b| {
                let va = dataset.animals[a].indicators.get(key).copied();
                let vb = dataset.animals[b].indicators.get(key).copied();
                va.partial_cmp(&vb).unwrap_or(Ordering::Equal)
            });
        }

        // Blocking uses the largest block count that still divides every quota, which is
        // their gcd: each block then hands out quota_i / g animals to group i, so every
        // group draws the same number of animals from every stretch of the primary
        // indicator. Without a primary indicator the whole stratum is one block, which is
        // exactly complete randomization.
        let blocks = if primary.is_some() {
            quotas.iter().copied().fold(0usize, gcd).max(1)
        } else {
            1
        };

        let size = ordered.len().div_ceil(blocks);
        if primary.is_some() {
            block_size = Some(block_size.map_or(size, |current: usize| current.max(size)));
            if ordered.len() % blocks != 0 {
                incomplete_last_block = true;
            }
        }

        strata.push(Stratum {
            ordered,
            quotas,
            blocks,
        });
    }

    if strata.is_empty() {
        bail!("数据集中没有任何动物，无法分组。");
    }

    Ok(Plan {
        strata,
        draw_order: order.to_vec(),
        num_animals: dataset.animals.len(),
        num_groups,
        block_size,
        incomplete_last_block,
        blocked: primary.is_some(),
    })
}

/// Deal one stratum: inside each block, order the animals by their draw and hand out each
/// group's per-block quota in turn.
///
/// Ordering by the draw before dealing is what randomizes the group labels too — which
/// group ends up being the control is decided by the numbers, not by position in the file.
fn deal_blocks(
    stratum: &Stratum,
    randoms: &[f64],
    blocked: bool,
    block_of: &mut [Option<usize>],
) -> Vec<Vec<usize>> {
    let Stratum {
        ordered,
        quotas,
        blocks,
    } = stratum;
    let blocks = *blocks;

    let mut groups: Vec<Vec<usize>> = vec![Vec::new(); quotas.len()];
    let mut remaining = quotas.clone();
    let block_size = ordered.len().div_ceil(blocks.max(1));

    for (block_idx, chunk) in ordered.chunks(block_size).enumerate() {
        let mut block = chunk.to_vec();
        // Stable sort, so the (astronomically unlikely) tie between two identical draws
        // falls back to the normalized animal order rather than drifting between runs.
        block.sort_by(|&a, &b| {
            randoms[a]
                .partial_cmp(&randoms[b])
                .unwrap_or(Ordering::Equal)
        });

        if blocked {
            for &idx in chunk {
                block_of[idx] = Some(block_idx + 1);
            }
        }

        let mut cursor = 0;
        for (group_idx, &quota) in quotas.iter().enumerate() {
            let take = (quota / blocks)
                .min(remaining[group_idx])
                .min(block.len() - cursor);
            groups[group_idx].extend_from_slice(&block[cursor..cursor + take]);
            remaining[group_idx] -= take;
            cursor += take;
        }

        // Only reachable if a block could not be split evenly, which the gcd rules out
        // for every configuration the validator accepts. Kept so an unevenly-sized last
        // block still places every animal instead of dropping it.
        for (group_idx, left) in remaining.iter_mut().enumerate() {
            if cursor >= block.len() {
                break;
            }
            let take = (*left).min(block.len() - cursor);
            groups[group_idx].extend_from_slice(&block[cursor..cursor + take]);
            *left -= take;
            cursor += take;
        }
    }

    groups
}

fn gcd(a: usize, b: usize) -> usize {
    if b == 0 {
        a
    } else {
        gcd(b, a % b)
    }
}

/// Build the diagnostic for a rejection-sampling run that never met its criterion.
fn acceptance_failure(
    dataset: &Dataset,
    group_config: &GroupConfig,
    stat_config: &StatConfig,
    last_rejected: Option<&CandidateGrouping>,
    observed_min_p: &[f64],
    max_attempts: usize,
) -> anyhow::Error {
    let mut sorted: Vec<f64> = observed_min_p
        .iter()
        .copied()
        .filter(|p| p.is_finite())
        .collect();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap_or(Ordering::Equal));

    let quantile = |q: f64| -> String {
        if sorted.is_empty() {
            return "—".to_string();
        }
        let idx = ((sorted.len() - 1) as f64 * q).round() as usize;
        format!("{:.4}", sorted[idx])
    };

    let bottleneck = last_rejected
        .and_then(|candidate| {
            evaluator::evaluate_grouping_with_constraints(
                candidate,
                dataset,
                stat_config,
                Some(&group_config.sex_constraints),
                evaluator::Untestable::Skip,
            )
            .ok()
        })
        .map(|result| {
            let failing: Vec<String> = result
                .statistics
                .iter()
                .filter(|s| !s.is_valid)
                .map(|s| format!("{}（P = {:.4}）", s.indicator_name, s.diff_p_value))
                .collect();
            if failing.is_empty() {
                "—".to_string()
            } else {
                failing.join("、")
            }
        })
        .unwrap_or_else(|| "—".to_string());

    anyhow!(
        "抽样 {} 次仍未满足接受准则（α = {}）。\n\
         观察到的 min(P) 分布：中位数 {}，最大值 {}。\n\
         最后一次抽样中不达标的指标：{}。\n\
         可选处理：① 把判定口径放宽为「优化」（允许 1 个指标不达标）后重试；\
         ② 关闭接受准则，改为纯随机化并接受失衡告警——此时导出的分组原理会相应改变。\n\
         请勿反复更换种子重算，那属于事后挑选结果。",
        max_attempts,
        stat_config.alpha,
        quantile(0.5),
        quantile(1.0),
        bottleneck
    )
}

#[cfg(test)]
mod tests;
