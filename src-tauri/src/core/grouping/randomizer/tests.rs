use super::*;
use crate::core::parser;
use std::collections::HashMap;

const FIXTURE_60F: &str = concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/randomization_input_60f.xlsx"
);

const BW: &str = "体重";
const CD45: &str = "CD45 比例";

fn dataset_60f() -> Dataset {
    parser::parse_excel_file(FIXTURE_60F).expect("fixture must parse")
}

/// Female-only layout, `groups` groups of equal size.
fn female_constraints(groups: usize, per_group: usize) -> Vec<SexConstraint> {
    (0..groups)
        .map(|i| SexConstraint {
            group_index: i,
            male_count: 0,
            female_count: per_group,
            group_type: GroupType::Experimental,
            custom_name: None,
        })
        .collect()
}

fn group_config(
    constraints: Vec<SexConstraint>,
    method: GroupingMethod,
    randomization: RandomizationConfig,
) -> GroupConfig {
    GroupConfig {
        num_groups: constraints.len(),
        animals_per_group: GroupSize::Custom {
            values: constraints
                .iter()
                .map(|c| c.male_count + c.female_count)
                .collect(),
        },
        sex_constraints: constraints,
        scenario: StudyScenario::Exploratory,
        method,
        randomization: Some(randomization),
    }
}

fn blocked(seed: u64, enforce: bool) -> RandomizationConfig {
    RandomizationConfig {
        seed: Some(seed),
        primary_indicator: Some(BW.to_string()),
        acceptance: enforce.then_some(AcceptanceCriterion::AlphaLine),
        max_attempts: 1000,
    }
}

fn plain(seed: u64) -> RandomizationConfig {
    RandomizationConfig {
        seed: Some(seed),
        primary_indicator: None,
        acceptance: None,
        max_attempts: 1,
    }
}

fn stat_config(indicators: &[&str]) -> StatConfig {
    StatConfig {
        selected_indicators: indicators.iter().map(|s| s.to_string()).collect(),
        alpha: 0.05,
        mode: OptimizationMode::Strict,
        max_candidates: 1,
    }
}

fn run(dataset: Dataset, config: GroupConfig, stat: StatConfig) -> GroupingResult {
    compute_random_grouping(dataset, config, stat)
        .expect("randomized allocation must succeed")
        .candidates
        .remove(0)
}

/// Animal id -> group id, the comparable form of an allocation.
fn allocation(result: &GroupingResult) -> Vec<(String, usize)> {
    let mut pairs: Vec<(String, usize)> = result
        .assignments
        .iter()
        .map(|a| (a.animal_id.clone(), a.group_id))
        .collect();
    pairs.sort();
    pairs
}

fn synthetic(n: usize, sexes: &[Sex]) -> Dataset {
    let animals: Vec<Animal> = (0..n)
        .map(|i| Animal {
            id: format!("{:03}", i + 1),
            sex: sexes[i % sexes.len()],
            indicators: HashMap::from([("x".to_string(), i as f64)]),
        })
        .collect();

    let male_count = animals.iter().filter(|a| a.sex == Sex::Male).count();

    Dataset {
        indicator_names: vec!["x".to_string()],
        indicator_metadata: vec![IndicatorMetadata::new(
            "x".to_string(),
            "x".to_string(),
            String::new(),
        )],
        metadata: DatasetMetadata {
            total_animals: n,
            male_count,
            female_count: n - male_count,
            indicator_count: 1,
        },
        animals,
    }
}

// ---------------------------------------------------------------- reproducibility

#[test]
fn same_seed_reproduces_the_allocation() {
    let config = group_config(
        female_constraints(3, 20),
        GroupingMethod::BlockedRandom,
        blocked(42, true),
    );

    let first = run(dataset_60f(), config.clone(), stat_config(&[BW, CD45]));
    let second = run(dataset_60f(), config, stat_config(&[BW, CD45]));

    assert_eq!(allocation(&first), allocation(&second));
    let (a, b) = (
        first.randomization.as_ref().unwrap(),
        second.randomization.as_ref().unwrap(),
    );
    assert_eq!(a.seed, b.seed);
    assert_eq!(
        a.attempts, b.attempts,
        "draw count must be reproducible too"
    );
    assert_eq!(a.input_fingerprint, b.input_fingerprint);
    assert_eq!(a.rng_algorithm, RNG_ALGORITHM);
}

#[test]
fn different_seeds_give_different_allocations() {
    let dataset = dataset_60f();
    let make = |seed| {
        group_config(
            female_constraints(3, 20),
            GroupingMethod::BlockedRandom,
            blocked(seed, false),
        )
    };

    let first = run(dataset.clone(), make(1), stat_config(&[BW, CD45]));
    let second = run(dataset, make(2), stat_config(&[BW, CD45]));

    assert_ne!(allocation(&first), allocation(&second));
}

/// The shuffle permutes whatever order it is handed, so without the normalization step
/// the Excel row order would silently become part of the seed.
#[test]
fn excel_row_order_does_not_affect_the_result() {
    let dataset = dataset_60f();
    let mut shuffled = dataset.clone();
    shuffled.animals.reverse();

    let config = group_config(
        female_constraints(3, 20),
        GroupingMethod::BlockedRandom,
        blocked(7, true),
    );

    let original = run(dataset, config.clone(), stat_config(&[BW, CD45]));
    let reordered = run(shuffled, config, stat_config(&[BW, CD45]));

    assert_eq!(allocation(&original), allocation(&reordered));
    assert_eq!(
        original.randomization.unwrap().input_fingerprint,
        reordered.randomization.unwrap().input_fingerprint
    );
}

#[test]
fn ids_sort_numerically_before_lexicographically() {
    let ids = ["10", "2", "A2", "A10", "1"];
    let animals: Vec<Animal> = ids
        .iter()
        .map(|id| Animal {
            id: (*id).to_string(),
            sex: Sex::Female,
            indicators: HashMap::new(),
        })
        .collect();

    let order: Vec<&str> = normalized_order(&animals)
        .into_iter()
        .map(|i| animals[i].id.as_str())
        .collect();

    assert_eq!(order, vec!["1", "2", "10", "A10", "A2"]);
}

#[test]
fn fingerprint_tracks_the_id_set_not_the_row_order() {
    let dataset = dataset_60f();
    let mut reversed = dataset.animals.clone();
    reversed.reverse();
    assert_eq!(
        dataset_fingerprint(&dataset.animals),
        dataset_fingerprint(&reversed)
    );

    let mut altered = dataset.animals.clone();
    altered[0].id = "999".to_string();
    assert_ne!(
        dataset_fingerprint(&dataset.animals),
        dataset_fingerprint(&altered)
    );
}

// ---------------------------------------------------------------- constraints

#[test]
fn allocation_satisfies_group_quotas_including_reserve() {
    let mut constraints = female_constraints(3, 18);
    constraints.push(SexConstraint {
        group_index: 3,
        male_count: 0,
        female_count: 6,
        group_type: GroupType::Reserve,
        custom_name: Some("备用动物".to_string()),
    });

    let config = group_config(
        constraints,
        GroupingMethod::BlockedRandom,
        blocked(11, false),
    );
    let result = run(dataset_60f(), config, stat_config(&[BW, CD45]));

    let mut sizes = [0usize; 4];
    for assignment in &result.assignments {
        sizes[assignment.group_id] += 1;
    }
    assert_eq!(sizes, [18, 18, 18, 6]);

    // The reserve group holds animals but is not an experimental group.
    assert_eq!(result.summary.num_groups, 3);
}

#[test]
fn mixed_sex_allocation_respects_per_sex_quotas() {
    let dataset = synthetic(12, &[Sex::Male, Sex::Female]);
    let constraints: Vec<SexConstraint> = (0..3)
        .map(|i| SexConstraint {
            group_index: i,
            male_count: 2,
            female_count: 2,
            group_type: GroupType::Experimental,
            custom_name: None,
        })
        .collect();

    let config = group_config(constraints, GroupingMethod::Random, plain(5));
    let result = run(dataset, config, stat_config(&["x"]));

    for group_id in 0..3 {
        let members: Vec<_> = result
            .assignments
            .iter()
            .filter(|a| a.group_id == group_id)
            .collect();
        assert_eq!(members.iter().filter(|a| a.sex == Sex::Male).count(), 2);
        assert_eq!(members.iter().filter(|a| a.sex == Sex::Female).count(), 2);
    }
}

// ---------------------------------------------------------------- blocking

/// The block table in the design doc: block count is the gcd of the quotas, block size is
/// the total divided by it.
#[test]
fn block_structure_matches_the_quota_table() {
    let dataset = dataset_60f();

    let cases: [(&[usize], usize, usize); 4] = [
        (&[10, 10, 10, 10, 10, 10], 10, 6),
        (&[20, 20, 20], 20, 3),
        (&[18, 18, 18, 6], 6, 10),
        (&[20, 20, 15], 5, 11),
    ];

    for (quotas, expected_blocks, expected_block_size) in cases {
        let constraints: Vec<SexConstraint> = quotas
            .iter()
            .enumerate()
            .map(|(i, &n)| SexConstraint {
                group_index: i,
                male_count: 0,
                female_count: n,
                group_type: GroupType::Experimental,
                custom_name: None,
            })
            .collect();

        let total: usize = quotas.iter().sum();
        let mut subset = dataset.clone();
        subset.animals.truncate(total);

        let config = group_config(
            constraints,
            GroupingMethod::BlockedRandom,
            blocked(0, false),
        );
        let plan = build_plan(
            &subset,
            &config,
            config.randomization.as_ref().unwrap(),
            &normalized_order(&subset.animals),
        )
        .unwrap();

        assert_eq!(
            plan.strata[0].blocks, expected_blocks,
            "quotas {quotas:?} should form {expected_blocks} blocks"
        );
        assert_eq!(plan.block_size, Some(expected_block_size));
        assert!(!plan.incomplete_last_block);
    }
}

/// The invariant blocking exists for: within every stretch of the primary indicator, each
/// group draws exactly its share. This is what makes the balance a property of the design
/// rather than of the draw.
#[test]
fn every_block_hands_each_group_its_exact_share() {
    let dataset = dataset_60f();
    let config = group_config(
        female_constraints(6, 10),
        GroupingMethod::BlockedRandom,
        blocked(3, false),
    );
    let result = run(dataset.clone(), config, stat_config(&[BW, CD45]));

    // Rank of each animal along the primary indicator.
    let mut by_weight: Vec<usize> = normalized_order(&dataset.animals);
    by_weight.sort_by(|&a, &b| {
        dataset.animals[a].indicators[BW]
            .partial_cmp(&dataset.animals[b].indicators[BW])
            .unwrap()
    });
    let rank: HashMap<&str, usize> = by_weight
        .iter()
        .enumerate()
        .map(|(r, &i)| (dataset.animals[i].id.as_str(), r))
        .collect();

    // 6 groups x 10 -> 10 blocks of 6, one animal per group per block.
    let mut per_block = vec![[0usize; 6]; 10];
    for assignment in &result.assignments {
        let block = rank[assignment.animal_id.as_str()] / 6;
        per_block[block][assignment.group_id] += 1;
    }

    for (block_idx, counts) in per_block.iter().enumerate() {
        assert_eq!(*counts, [1; 6], "block {block_idx} is not balanced");
    }
}

/// Numbers from the design doc: blocking pulls the worst-case spread of the group means
/// from ~2.7 g down below half a gram. Thresholds are deliberately loose (measured worst
/// case 0.465 g / 0.157 g) so this cannot go flaky.
#[test]
fn blocking_bounds_the_primary_indicator_spread() {
    let dataset = dataset_60f();
    let values: HashMap<&str, f64> = dataset
        .animals
        .iter()
        .map(|a| (a.id.as_str(), a.indicators[BW]))
        .collect();

    for (groups, per_group, limit) in [(6usize, 10usize, 0.6f64), (3, 20, 0.3)] {
        let mut worst: f64 = 0.0;
        for seed in 0..50u64 {
            let config = group_config(
                female_constraints(groups, per_group),
                GroupingMethod::BlockedRandom,
                blocked(seed, false),
            );
            let result = run(dataset.clone(), config, stat_config(&[BW, CD45]));

            let mut sums = vec![(0.0f64, 0usize); groups];
            for assignment in &result.assignments {
                let entry = &mut sums[assignment.group_id];
                entry.0 += values[assignment.animal_id.as_str()];
                entry.1 += 1;
            }
            let means: Vec<f64> = sums.iter().map(|(sum, n)| sum / *n as f64).collect();
            let spread = means.iter().cloned().fold(f64::MIN, f64::max)
                - means.iter().cloned().fold(f64::MAX, f64::min);
            worst = worst.max(spread);
        }

        assert!(
            worst < limit,
            "{groups} groups: worst spread {worst:.3} g exceeded {limit} g"
        );
    }
}

#[test]
fn tied_primary_values_still_reproduce() {
    // Every animal shares one weight, so ordering is decided entirely by the id tiebreak.
    let mut dataset = synthetic(12, &[Sex::Female]);
    for animal in &mut dataset.animals {
        animal.indicators.insert("x".to_string(), 1.0);
    }

    let config = group_config(
        female_constraints(3, 4),
        GroupingMethod::BlockedRandom,
        RandomizationConfig {
            seed: Some(9),
            primary_indicator: Some("x".to_string()),
            acceptance: None,
            max_attempts: 1,
        },
    );

    // A single shared value leaves every indicator with zero variance, so the statistics
    // are left out; the assertion is about the allocation.
    let first = run(dataset.clone(), config.clone(), stat_config(&[]));
    let mut reordered = dataset;
    reordered.animals.reverse();
    let second = run(reordered, config, stat_config(&[]));

    assert_eq!(allocation(&first), allocation(&second));
}

// ---------------------------------------------------------------- randomness

/// Checking only "each animal lands in each group equally often" would pass even if the
/// draw kept neighbours together, so the pairwise co-occurrence rate is checked too.
#[test]
fn complete_randomization_is_uniform() {
    let dataset = synthetic(6, &[Sex::Female]);
    let mut group_counts = [[0usize; 3]; 6];
    let mut together = [[0usize; 6]; 6];
    let draws = 900u64;

    for seed in 0..draws {
        let config = group_config(
            female_constraints(3, 2),
            GroupingMethod::Random,
            plain(seed),
        );
        // No indicators selected: this test is about the draw, not the statistics.
        let result = run(dataset.clone(), config, stat_config(&[]));

        let mut assigned: Vec<(usize, usize)> = Vec::new();
        for assignment in &result.assignments {
            let animal: usize = assignment.animal_id.parse::<usize>().unwrap() - 1;
            group_counts[animal][assignment.group_id] += 1;
            assigned.push((animal, assignment.group_id));
        }
        for (a, ga) in &assigned {
            for (b, gb) in &assigned {
                if a < b && ga == gb {
                    together[*a][*b] += 1;
                }
            }
        }
    }

    let expected_group = draws as f64 / 3.0;
    for (animal, counts) in group_counts.iter().enumerate() {
        for (group, &count) in counts.iter().enumerate() {
            let deviation = (count as f64 - expected_group).abs() / expected_group;
            assert!(
                deviation < 0.15,
                "animal {animal} lands in group {group} {count} times, expected ~{expected_group}"
            );
        }
    }

    // With 6 animals in 3 groups of 2, any two animals share a group with probability 1/5.
    let expected_pair = draws as f64 / 5.0;
    for (a, row) in together.iter().enumerate() {
        for (b, &count) in row.iter().enumerate().skip(a + 1) {
            let deviation = (count as f64 - expected_pair).abs() / expected_pair;
            assert!(
                deviation < 0.2,
                "animals {a}/{b} share a group {count} times, expected ~{expected_pair}"
            );
        }
    }
}

/// Complete randomization must not look at indicator values at all: same seed, wildly
/// different measurements, identical allocation.
#[test]
fn complete_randomization_ignores_indicator_values() {
    let dataset = synthetic(12, &[Sex::Female]);
    let mut rescaled = dataset.clone();
    for (i, animal) in rescaled.animals.iter_mut().enumerate() {
        animal
            .indicators
            .insert("x".to_string(), 1000.0 - i as f64 * 37.0);
    }

    let config = group_config(female_constraints(3, 4), GroupingMethod::Random, plain(4));
    let first = run(dataset, config.clone(), stat_config(&["x"]));
    let second = run(rescaled, config, stat_config(&["x"]));

    assert_eq!(allocation(&first), allocation(&second));
}

// ---------------------------------------------------------------- acceptance criterion

#[test]
fn acceptance_criterion_is_met_and_the_draw_count_is_recorded() {
    let config = group_config(
        female_constraints(6, 10),
        GroupingMethod::BlockedRandom,
        blocked(17, true),
    );
    let result = run(dataset_60f(), config, stat_config(&[BW, CD45]));

    assert!(result.summary.meets_criteria);
    let record = result.randomization.unwrap();
    assert!(record.attempts >= 1);
    assert_eq!(record.primary_indicator.as_deref(), Some(BW));
    assert_eq!(record.block_size, Some(6));
}

#[test]
fn exhausting_the_attempt_budget_reports_the_bottleneck_instead_of_degrading() {
    // alpha = 0.999 makes essentially every draw fail the criterion.
    let config = group_config(
        female_constraints(3, 20),
        GroupingMethod::ConstrainedRandom,
        RandomizationConfig {
            seed: Some(1),
            primary_indicator: None,
            acceptance: Some(AcceptanceCriterion::AlphaLine),
            max_attempts: 5,
        },
    );
    let stat = StatConfig {
        selected_indicators: vec![BW.to_string(), CD45.to_string()],
        alpha: 0.999,
        mode: OptimizationMode::Strict,
        max_candidates: 1,
    };

    let err = compute_random_grouping(dataset_60f(), config, stat)
        .unwrap_err()
        .to_string();

    assert!(err.contains("抽样 5 次"), "{err}");
    assert!(err.contains("min(P)"), "{err}");
    assert!(
        err.contains(BW) || err.contains(CD45),
        "should name the blocking indicator: {err}"
    );
}

// ---------------------------------------------------------------- config validation

#[test]
fn blocked_random_requires_an_existing_complete_primary_indicator() {
    let dataset = dataset_60f();

    let missing_key = group_config(
        female_constraints(3, 20),
        GroupingMethod::BlockedRandom,
        RandomizationConfig {
            seed: Some(1),
            primary_indicator: Some("不存在".to_string()),
            acceptance: None,
            max_attempts: 1,
        },
    );
    let err = compute_random_grouping(dataset.clone(), missing_key, stat_config(&[BW]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("不存在"), "{err}");

    let mut holes = dataset;
    holes.animals[0].indicators.remove(BW);
    holes.animals[1].indicators.remove(BW);
    let config = group_config(
        female_constraints(3, 20),
        GroupingMethod::BlockedRandom,
        blocked(1, false),
    );
    let err = compute_random_grouping(holes, config, stat_config(&[CD45]))
        .unwrap_err()
        .to_string();
    assert!(err.contains("缺失 2 只"), "{err}");
}

#[test]
fn method_and_parameters_must_agree() {
    let dataset = dataset_60f();

    // Complete randomization with an acceptance criterion is constrained randomization
    // under a different name; the label has to match what actually ran.
    let mismatched = group_config(
        female_constraints(3, 20),
        GroupingMethod::Random,
        RandomizationConfig {
            seed: Some(1),
            primary_indicator: None,
            acceptance: Some(AcceptanceCriterion::AlphaLine),
            max_attempts: 10,
        },
    );
    assert!(compute_random_grouping(dataset.clone(), mismatched, stat_config(&[BW])).is_err());

    let no_criterion = group_config(
        female_constraints(3, 20),
        GroupingMethod::ConstrainedRandom,
        plain(1),
    );
    assert!(compute_random_grouping(dataset, no_criterion, stat_config(&[BW])).is_err());
}

/// `TopFraction` is declared in the enum but not implemented yet: `validate_randomization`
/// must refuse it before any draw happens, rather than falling through to the
/// `unreachable!()` in the compute loop. This test is temporary — a later task implements
/// the calibration and replaces it with one asserting the criterion actually works.
#[test]
fn top_fraction_is_rejected_until_it_is_implemented() {
    let config = group_config(
        female_constraints(3, 20),
        GroupingMethod::ConstrainedRandom,
        RandomizationConfig {
            seed: Some(1),
            primary_indicator: None,
            acceptance: Some(AcceptanceCriterion::TopFraction { target_rate: 0.10 }),
            max_attempts: 10,
        },
    );

    let err = compute_random_grouping(dataset_60f(), config, stat_config(&[BW]))
        .unwrap_err()
        .to_string();

    assert!(err.contains("尚未启用"), "{err}");
}

#[test]
fn blocked_random_drives_the_primary_indicator_p_value_towards_one() {
    // Not an assertion about quality: blocking forces the stratification variable's own
    // test towards P = 1, which is why it must be labelled a stratification variable
    // rather than read as a result.
    let config = group_config(
        female_constraints(6, 10),
        GroupingMethod::BlockedRandom,
        blocked(23, false),
    );
    let result = run(dataset_60f(), config, stat_config(&[BW, CD45]));

    let bw = result
        .statistics
        .iter()
        .find(|s| s.indicator_name == BW)
        .unwrap();
    assert!(bw.diff_p_value > 0.9, "BW P = {}", bw.diff_p_value);
}

/// A constant indicator used to take the whole run down: every group ends up with zero
/// variance, and the F distribution panicked on the resulting NaN. Randomization hits
/// this far more easily than optimization does — a draw can leave any indicator constant
/// within a group — and the draw is the allocation, so there is no other candidate to
/// fall back on.
#[test]
fn a_constant_indicator_does_not_bring_down_the_run() {
    let mut dataset = synthetic(12, &[Sex::Female]);
    for animal in &mut dataset.animals {
        animal.indicators.insert("x".to_string(), 1.0);
    }

    let config = group_config(female_constraints(3, 4), GroupingMethod::Random, plain(1));
    let result = run(dataset, config, stat_config(&["x"]));

    let stats = &result.statistics[0];
    assert_eq!(
        stats.diff_p_value, 1.0,
        "identical groups are perfectly balanced"
    );
    assert!(stats.is_valid);
    assert_eq!(result.summary.total_indicators, 1);
}

// ------------------------------------------------- the exported draw reproduces the run

/// Replay the documented rule by hand: inside each block, sort by the recorded draw and
/// hand each group its quota in turn. This is the check a reviewer does in Excel, so if
/// it does not come back out, the exported column is decoration rather than evidence.
fn replay(result: &GroupingResult, quotas: &[usize], blocks: usize) -> Vec<(String, usize)> {
    let mut rows: Vec<(&GroupAssignment, usize, f64)> = result
        .assignments
        .iter()
        .map(|a| {
            (
                a,
                a.block_index.unwrap_or(1),
                a.random_number.expect("every animal carries its draw"),
            )
        })
        .collect();

    // Block first, then the draw within it.
    rows.sort_by(|a, b| a.1.cmp(&b.1).then(a.2.partial_cmp(&b.2).unwrap()));

    let mut replayed = Vec::new();
    for block in rows.chunk_by(|a, b| a.1 == b.1) {
        let mut cursor = 0;
        for (group_id, quota) in quotas.iter().enumerate() {
            let take = quota / blocks;
            for (assignment, _, _) in &block[cursor..cursor + take] {
                replayed.push((assignment.animal_id.clone(), group_id));
            }
            cursor += take;
        }
    }

    replayed.sort();
    replayed
}

#[test]
fn the_exported_draw_reproduces_a_blocked_allocation() {
    let config = group_config(
        female_constraints(6, 10),
        GroupingMethod::BlockedRandom,
        blocked(2026, true),
    );
    let result = run(dataset_60f(), config, stat_config(&[BW, CD45]));

    assert_eq!(replay(&result, &[10; 6], 10), allocation(&result));
}

#[test]
fn the_exported_draw_reproduces_a_complete_randomization() {
    let config = group_config(female_constraints(3, 20), GroupingMethod::Random, plain(77));
    let result = run(dataset_60f(), config, stat_config(&[BW, CD45]));

    // No blocking: the whole stratum is one block, so a single sort by the draw settles it.
    assert!(result.assignments.iter().all(|a| a.block_index.is_none()));
    assert_eq!(replay(&result, &[20; 3], 1), allocation(&result));
}

/// Under rejection sampling the accepted allocation is not the last one drawn, so the
/// recorded numbers must be the ones that produced the reported grouping.
#[test]
fn the_recorded_draw_belongs_to_the_accepted_attempt() {
    let config = group_config(
        female_constraints(6, 10),
        GroupingMethod::ConstrainedRandom,
        RandomizationConfig {
            seed: Some(5),
            primary_indicator: None,
            acceptance: Some(AcceptanceCriterion::AlphaLine),
            max_attempts: 1000,
        },
    );
    let result = run(dataset_60f(), config, stat_config(&[BW, CD45]));

    assert!(result.summary.meets_criteria);
    assert!(result.randomization.as_ref().unwrap().attempts >= 1);
    assert_eq!(replay(&result, &[10; 6], 1), allocation(&result));
}

#[test]
fn draws_are_uniform_reproducible_and_absent_from_optimization() {
    let config = group_config(
        female_constraints(3, 20),
        GroupingMethod::BlockedRandom,
        blocked(31, false),
    );
    let first = run(dataset_60f(), config.clone(), stat_config(&[BW, CD45]));
    let second = run(dataset_60f(), config, stat_config(&[BW, CD45]));

    let draws = |r: &GroupingResult| -> Vec<(String, f64, Option<usize>)> {
        let mut v: Vec<_> = r
            .assignments
            .iter()
            .map(|a| (a.animal_id.clone(), a.random_number.unwrap(), a.block_index))
            .collect();
        v.sort_by(|a, b| a.0.cmp(&b.0));
        v
    };

    assert_eq!(draws(&first), draws(&second), "same seed, same draws");
    assert!(draws(&first).iter().all(|(_, r, _)| (0.0..1.0).contains(r)));
    // 3 groups of 20 -> gcd 20 -> 20 blocks of 3.
    assert!(draws(&first)
        .iter()
        .all(|(_, _, block)| matches!(block, Some(b) if (1..=20).contains(b))));
}
