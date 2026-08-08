use super::*;
use crate::core::grouping::compute_grouping;
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

/// A dataset whose covariate `x` is whatever `values` says, one animal per entry.
fn synthetic(sexes: &[Sex], values: &[f64]) -> Dataset {
    assert_eq!(sexes.len(), values.len());

    let animals: Vec<Animal> = sexes
        .iter()
        .zip(values)
        .enumerate()
        .map(|(i, (&sex, &value))| Animal {
            id: format!("{:03}", i + 1),
            sex,
            indicators: HashMap::from([("x".to_string(), value)]),
        })
        .collect();

    let male_count = animals.iter().filter(|a| a.sex == Sex::Male).count();
    let total = animals.len();

    Dataset {
        indicator_names: vec!["x".to_string()],
        indicator_metadata: vec![IndicatorMetadata::new(
            "x".to_string(),
            "x".to_string(),
            String::new(),
        )],
        metadata: DatasetMetadata {
            total_animals: total,
            male_count,
            female_count: total - male_count,
            indicator_count: 1,
        },
        animals,
    }
}

fn constraints(quotas: &[(usize, usize)]) -> Vec<SexConstraint> {
    quotas
        .iter()
        .enumerate()
        .map(|(i, &(male, female))| SexConstraint {
            group_index: i,
            male_count: male,
            female_count: female,
            group_type: GroupType::Experimental,
            custom_name: None,
        })
        .collect()
}

fn minimization_config(covariates: &[&str], p: f64) -> MinimizationConfig {
    MinimizationConfig {
        covariates: covariates.iter().map(|s| s.to_string()).collect(),
        allocation_probability: p,
        binning: CovariateBinning::Tertiles,
    }
}

fn group_config(
    sex_constraints: Vec<SexConstraint>,
    seed: u64,
    covariates: &[&str],
    p: f64,
) -> GroupConfig {
    GroupConfig {
        num_groups: sex_constraints.len(),
        animals_per_group: GroupSize::Custom {
            values: sex_constraints
                .iter()
                .map(|c| c.male_count + c.female_count)
                .collect(),
        },
        sex_constraints,
        scenario: StudyScenario::ConfirmatoryTrial,
        method: GroupingMethod::Minimization,
        randomization: Some(RandomizationConfig {
            seed: Some(seed),
            primary_indicator: None,
            acceptance: None,
            max_attempts: 1,
            draw_index: 1,
            minimization: Some(minimization_config(covariates, p)),
        }),
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
    compute_minimization_grouping(dataset, config, stat)
        .expect("minimization must succeed")
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

fn record_of(result: &GroupingResult) -> &MinimizationRecord {
    result
        .randomization
        .as_ref()
        .expect("minimization records its run")
        .minimization
        .as_ref()
        .expect("with a minimization block")
}

// ------------------------------------------------------------------ reproducibility

#[test]
fn same_seed_reproduces_the_allocation_animal_for_animal() {
    let stat = stat_config(&[BW, CD45]);
    let first = run(
        dataset_60f(),
        group_config(constraints(&[(0, 20); 3]), 7, &[BW, CD45], 0.8),
        stat.clone(),
    );
    let second = run(
        dataset_60f(),
        group_config(constraints(&[(0, 20); 3]), 7, &[BW, CD45], 0.8),
        stat,
    );

    assert_eq!(allocation(&first), allocation(&second));

    // The entry order is a random draw of its own; reproducing the groups but not the
    // order would mean the decision log could not be replayed.
    let first_order: Vec<_> = first.assignments.iter().map(|a| a.entry_index).collect();
    let second_order: Vec<_> = second.assignments.iter().map(|a| a.entry_index).collect();
    assert_eq!(first_order, second_order);
}

#[test]
fn different_seeds_produce_different_allocations() {
    let stat = stat_config(&[BW, CD45]);
    let a = run(
        dataset_60f(),
        group_config(constraints(&[(0, 20); 3]), 1, &[BW], 0.8),
        stat.clone(),
    );
    let b = run(
        dataset_60f(),
        group_config(constraints(&[(0, 20); 3]), 2, &[BW], 0.8),
        stat,
    );

    assert_ne!(allocation(&a), allocation(&b));
}

/// The entry order starts from the normalized animal order, not the Excel row order, so
/// re-sorting the input file must not move the result under a fixed seed.
#[test]
fn shuffling_the_input_rows_does_not_move_the_allocation() {
    let dataset = dataset_60f();
    let mut reversed = dataset.clone();
    reversed.animals.reverse();

    let stat = stat_config(&[BW]);
    let original = run(
        dataset,
        group_config(constraints(&[(0, 20); 3]), 99, &[BW], 0.8),
        stat.clone(),
    );
    let shuffled = run(
        reversed,
        group_config(constraints(&[(0, 20); 3]), 99, &[BW], 0.8),
        stat,
    );

    assert_eq!(allocation(&original), allocation(&shuffled));
}

/// Every animal costs exactly two uniforms, including the ones at the end of a stratum
/// whose group is already forced. Skipping a draw there looks harmless and would shift
/// every later animal's allocation, silently breaking every archived seed.
#[test]
fn the_pass_consumes_exactly_two_uniforms_per_animal() {
    // Equal quotas guarantee a forced tail: once two of the three groups are full, the
    // remaining animals have exactly one eligible group and nothing to decide.
    let dataset = synthetic(
        &[Sex::Female; 9],
        &[1.0, 2.0, 3.0, 4.0, 5.0, 6.0, 7.0, 8.0, 9.0],
    );
    let config = group_config(constraints(&[(0, 3); 3]), 5, &["x"], 0.8);
    let covariates = vec![BinnedCovariate::build("x", &dataset.animals).unwrap()];

    let mut rng = ChaCha12Rng::seed_from_u64(5);
    let mut order = randomizer::normalized_order(&dataset.animals);
    order.shuffle(&mut rng);
    allocate(&dataset, &config, &covariates, &order, 0.8, &mut rng).unwrap();
    let next_after_pass: f64 = rng.gen();

    let mut reference = ChaCha12Rng::seed_from_u64(5);
    let mut same_order = randomizer::normalized_order(&dataset.animals);
    same_order.shuffle(&mut reference);
    for _ in 0..(2 * dataset.animals.len()) {
        let _: f64 = reference.gen();
    }
    let next_after_manual: f64 = reference.gen();

    assert_eq!(
        next_after_pass, next_after_manual,
        "the pass must leave the stream exactly 2n uniforms along"
    );
}

// ------------------------------------------------------------------------- binning

/// The regression that motivated per-stratum binning: males and females on disjoint
/// ranges. Binning globally would put every male in the top level and every female in the
/// bottom one, and since eligibility is already filtered by sex the covariate would carry
/// no information at all.
#[test]
fn covariates_are_binned_inside_each_sex_stratum() {
    let sexes = [
        Sex::Male,
        Sex::Male,
        Sex::Male,
        Sex::Male,
        Sex::Male,
        Sex::Male,
        Sex::Female,
        Sex::Female,
        Sex::Female,
        Sex::Female,
        Sex::Female,
        Sex::Female,
    ];
    // Males 30-35, females 10-15: no overlap whatsoever.
    let values = [
        30.0, 31.0, 32.0, 33.0, 34.0, 35.0, 10.0, 11.0, 12.0, 13.0, 14.0, 15.0,
    ];
    let dataset = synthetic(&sexes, &values);

    let binned = BinnedCovariate::build("x", &dataset.animals).unwrap();

    assert_eq!(
        binned.levels_per_sex,
        [3, 3],
        "each stratum must get its own tertiles"
    );

    for sex in [Sex::Male, Sex::Female] {
        let levels: std::collections::HashSet<usize> = dataset
            .animals
            .iter()
            .enumerate()
            .filter(|(_, a)| a.sex == sex)
            .map(|(idx, _)| binned.level_of[idx])
            .collect();
        assert_eq!(
            levels.len(),
            3,
            "{sex:?} must span three levels, not collapse into one"
        );
    }
}

#[test]
fn equal_values_always_share_a_level() {
    // Six animals, three distinct values, each repeated: the tertile boundaries have to
    // snap onto the value changes rather than splitting a tied pair.
    let dataset = synthetic(&[Sex::Female; 6], &[1.0, 1.0, 2.0, 2.0, 3.0, 3.0]);
    let binned = BinnedCovariate::build("x", &dataset.animals).unwrap();

    assert_eq!(binned.levels_per_sex[1], 3);
    assert_eq!(binned.level_of, vec![0, 0, 1, 1, 2, 2]);
}

#[test]
fn a_constant_covariate_collapses_to_a_single_level() {
    let dataset = synthetic(&[Sex::Female; 6], &[4.2; 6]);
    let binned = BinnedCovariate::build("x", &dataset.animals).unwrap();

    assert_eq!(binned.levels_per_sex[1], 1);
    assert!(binned.cuts_per_sex[1].is_empty());
    assert!(binned.level_of.iter().all(|&level| level == 0));
}

#[test]
fn fewer_than_three_distinct_values_gives_fewer_levels() {
    let dataset = synthetic(&[Sex::Female; 6], &[1.0, 1.0, 1.0, 9.0, 9.0, 9.0]);
    let binned = BinnedCovariate::build("x", &dataset.animals).unwrap();

    assert_eq!(
        binned.levels_per_sex[1], 2,
        "two values cannot make three levels"
    );
    assert_eq!(binned.cuts_per_sex[1], vec![5.0]);
}

#[test]
fn cut_points_are_recorded_for_every_stratum() {
    let dataset = dataset_60f();
    let result = run(
        dataset,
        group_config(constraints(&[(0, 20); 3]), 3, &[BW, CD45], 0.8),
        stat_config(&[BW]),
    );
    let record = record_of(&result);

    assert_eq!(record.covariates, vec![BW.to_string(), CD45.to_string()]);
    assert_eq!(record.bins.len(), 2);
    for bins in &record.bins {
        // The fixture is female-only, so exactly one stratum is present.
        assert_eq!(bins.strata.len(), 1);
        let stratum = &bins.strata[0];
        assert_eq!(stratum.sex, Sex::Female);
        assert_eq!(stratum.levels, stratum.cut_points.len() + 1);
        assert_eq!(stratum.levels, 3, "60 distinct values give full tertiles");
    }
}

// --------------------------------------------------------------- imbalance measure

/// Counts (2, 1) against quotas (20, 10) are already proportional. Raw counts would send
/// the animal to the small group to level the integers; the quota-normalized measure
/// keeps the proportions level instead, which is what "balanced" means when the groups
/// are not the same size.
#[test]
fn quota_normalization_beats_raw_counts_when_groups_differ_in_size() {
    let dataset = synthetic(&[Sex::Female; 3], &[1.0, 2.0, 3.0]);
    let covariates = vec![BinnedCovariate::build("x", &dataset.animals).unwrap()];
    let quotas = vec![[0usize, 20], [0usize, 10]];
    let measured = vec![0usize, 1usize];

    // One cell, holding 2 animals in group 0 and 1 in group 1.
    let cell = covariates[0].cell(Sex::Female, 0);
    let mut counts = vec![vec![vec![0usize; 2]; covariates[0].cells()]];
    counts[0][cell][0] = 2;
    counts[0][cell][1] = 1;

    let to_large = imbalance_score(&covariates, &counts, &quotas, &measured, Sex::Female, 0, 0);
    let to_small = imbalance_score(&covariates, &counts, &quotas, &measured, Sex::Female, 0, 1);

    assert!(
        to_large < to_small,
        "the larger group should take it (3/20 vs 1/10 = 0.05 spread, \
         against 2/20 vs 2/10 = 0.10): {to_large} vs {to_small}"
    );
}

/// With equal quotas the normalization is a constant factor, so the ranking has to be
/// identical to the raw-count range this replaced.
#[test]
fn equal_quotas_rank_exactly_like_raw_counts() {
    let dataset = synthetic(&[Sex::Female; 3], &[1.0, 2.0, 3.0]);
    let covariates = vec![BinnedCovariate::build("x", &dataset.animals).unwrap()];
    let quotas = vec![[0usize, 10], [0usize, 10]];
    let measured = vec![0usize, 1usize];

    let cell = covariates[0].cell(Sex::Female, 0);
    let mut counts = vec![vec![vec![0usize; 2]; covariates[0].cells()]];
    counts[0][cell][0] = 2;
    counts[0][cell][1] = 1;

    let to_fuller = imbalance_score(&covariates, &counts, &quotas, &measured, Sex::Female, 0, 0);
    let to_emptier = imbalance_score(&covariates, &counts, &quotas, &measured, Sex::Female, 0, 1);

    // Raw counts: (3,1) spreads by 2, (2,2) spreads by 0 — the emptier group wins.
    assert!(to_emptier < to_fuller);
    assert!((to_emptier - 0.0).abs() < 1e-12);
    assert!((to_fuller - 0.2).abs() < 1e-12);
}

#[test]
fn a_single_measured_group_has_no_imbalance_to_speak_of() {
    let dataset = synthetic(&[Sex::Female; 3], &[1.0, 2.0, 3.0]);
    let covariates = vec![BinnedCovariate::build("x", &dataset.animals).unwrap()];
    let quotas = vec![[0usize, 3]];
    let counts = vec![vec![vec![0usize; 1]; covariates[0].cells()]];

    let score = imbalance_score(&covariates, &counts, &quotas, &[0], Sex::Female, 0, 0);
    assert_eq!(score, 0.0);
}

// ----------------------------------------------------------------- decision branch

#[test]
fn p_one_always_takes_a_minimizer_and_p_zero_never_does() {
    let minimizers = [1usize];
    let others = [0usize, 2usize];

    for pick in [0.0, 0.4, 0.99] {
        let (group, took) = choose_group(&minimizers, &others, 1.0, 0.999_999, pick);
        assert_eq!(group, 1);
        assert!(took);

        let (group, took) = choose_group(&minimizers, &others, 0.0, 0.0, pick);
        assert_ne!(group, 1, "the 1 - p branch must avoid the minimizer");
        assert!(!took);
    }
}

#[test]
fn the_coin_decides_which_branch_fires() {
    let minimizers = [1usize];
    let others = [0usize, 2usize];

    let (group, took) = choose_group(&minimizers, &others, 0.8, 0.79, 0.0);
    assert!(took, "coin below p takes the minimizer");
    assert_eq!(group, 1);

    let (group, took) = choose_group(&minimizers, &others, 0.8, 0.81, 0.0);
    assert!(!took, "coin above p takes an other");
    assert_eq!(group, 0);
}

/// Every group tied is the normal state at the first animal. There is no "other" to fall
/// back to, and the uniform pick over the tied groups is what makes which group becomes
/// the control genuinely random instead of always group 1.
#[test]
fn a_full_tie_picks_uniformly_among_the_minimizers() {
    let minimizers = [0usize, 1usize, 2usize];
    let others: [usize; 0] = [];

    let picked: Vec<usize> = [0.0, 0.4, 0.9]
        .iter()
        .map(|&pick| choose_group(&minimizers, &others, 0.8, 0.99, pick).0)
        .collect();

    assert_eq!(picked, vec![0, 1, 2]);
    assert!(
        choose_group(&minimizers, &others, 0.8, 0.99, 0.5).1,
        "with nothing else to choose, the animal did go to a minimizer"
    );
}

#[test]
fn the_pick_never_runs_off_the_end_of_the_pool() {
    let minimizers = [0usize, 1usize];
    let others: [usize; 0] = [];
    // `gen::<f64>()` is exclusive of 1.0, but the clamp has to hold regardless.
    let (group, _) = choose_group(&minimizers, &others, 1.0, 0.0, 1.0);
    assert_eq!(group, 1);
}

// ------------------------------------------------------------ quotas and reserve

#[test]
fn quotas_and_sex_ratios_are_met_exactly() {
    let sexes: Vec<Sex> = (0..24)
        .map(|i| if i < 12 { Sex::Male } else { Sex::Female })
        .collect();
    let values: Vec<f64> = (0..24).map(|i| i as f64).collect();
    let dataset = synthetic(&sexes, &values);

    let config = group_config(constraints(&[(4, 4), (4, 4), (4, 4)]), 11, &["x"], 0.8);
    let result = run(dataset, config, stat_config(&["x"]));

    for group_id in 0..3 {
        let members: Vec<_> = result
            .assignments
            .iter()
            .filter(|a| a.group_id == group_id)
            .collect();
        assert_eq!(members.len(), 8);
        assert_eq!(members.iter().filter(|a| a.sex == Sex::Male).count(), 4);
        assert_eq!(members.iter().filter(|a| a.sex == Sex::Female).count(), 4);
    }
}

/// Reserve animals are the overflow: within each sex they are the last to enter. That is
/// what makes them a uniform random subset rather than, say, the heaviest animals.
#[test]
fn the_reserve_group_takes_the_tail_of_the_entry_order() {
    let dataset = dataset_60f();
    let mut sex_constraints = constraints(&[(0, 18); 3]);
    sex_constraints.push(SexConstraint {
        group_index: 3,
        male_count: 0,
        female_count: 6,
        group_type: GroupType::Reserve,
        custom_name: Some("备用动物".to_string()),
    });

    let mut config = group_config(sex_constraints, 2026, &[BW], 0.8);
    config.num_groups = 4;

    let result = run(dataset, config, stat_config(&[BW]));

    let entry_of = |group_id: usize| -> Vec<usize> {
        result
            .assignments
            .iter()
            .filter(|a| a.group_id == group_id)
            .map(|a| a.entry_index.expect("minimization records entry order"))
            .collect()
    };

    let reserve = entry_of(3);
    assert_eq!(reserve.len(), 6);

    let latest_experimental = (0..3)
        .flat_map(entry_of)
        .max()
        .expect("experimental groups hold animals");
    let earliest_reserve = *reserve.iter().min().expect("reserve holds animals");

    assert!(
        earliest_reserve > latest_experimental,
        "reserve must only take animals arriving after the study is fully staffed"
    );

    // And the reserve stays out of the statistics, as for every other method.
    assert_eq!(result.summary.num_groups, 3);
}

// --------------------------------------------------------------------- balance

/// The whole point of the method: covariate levels should sit more evenly across groups
/// than complete randomization manages. Compared over a fixed seed sequence so the test
/// cannot flake on an unlucky draw.
#[test]
fn minimization_balances_the_covariate_better_than_complete_randomization() {
    let dataset = dataset_60f();
    let stat = stat_config(&[BW]);

    let group_mean_range = |result: &GroupingResult| -> f64 {
        let value_of: HashMap<&str, f64> = dataset
            .animals
            .iter()
            .map(|a| (a.id.as_str(), a.indicators[BW]))
            .collect();

        let means: Vec<f64> = (0..3)
            .map(|group_id| {
                let values: Vec<f64> = result
                    .assignments
                    .iter()
                    .filter(|a| a.group_id == group_id)
                    .map(|a| value_of[a.animal_id.as_str()])
                    .collect();
                values.iter().sum::<f64>() / values.len() as f64
            })
            .collect();

        means.iter().cloned().fold(f64::MIN, f64::max)
            - means.iter().cloned().fold(f64::MAX, f64::min)
    };

    let mut minimized = 0.0;
    let mut randomized = 0.0;
    const SEEDS: u64 = 40;

    for seed in 1..=SEEDS {
        let result = run(
            dataset.clone(),
            group_config(constraints(&[(0, 20); 3]), seed, &[BW], 0.8),
            stat.clone(),
        );
        minimized += group_mean_range(&result);

        let mut random_config = group_config(constraints(&[(0, 20); 3]), seed, &[BW], 0.8);
        random_config.method = GroupingMethod::Random;
        random_config.randomization = Some(RandomizationConfig {
            seed: Some(seed),
            ..Default::default()
        });
        let random_result = compute_grouping(dataset.clone(), random_config, stat.clone())
            .expect("complete randomization must succeed")
            .candidates
            .remove(0);
        randomized += group_mean_range(&random_result);
    }

    assert!(
        minimized < randomized * 0.8,
        "minimization on 体重 should tighten the group means well below complete \
         randomization: {} vs {}",
        minimized / SEEDS as f64,
        randomized / SEEDS as f64
    );
}

// ------------------------------------------------------------------ decision log

/// The exported 最小化过程 sheet is only worth anything if it actually describes the run.
/// This replays it: every step's recorded branch has to match its recorded scores, and
/// the group it names has to be the group the animal ended up in.
#[test]
fn the_decision_log_replays_into_the_same_allocation() {
    let dataset = dataset_60f();
    let result = run(
        dataset,
        group_config(constraints(&[(0, 20); 3]), 4242, &[BW, CD45], 0.8),
        stat_config(&[BW, CD45]),
    );
    let record = record_of(&result);

    assert_eq!(record.decisions.len(), 60);

    let group_of: HashMap<&str, usize> = result
        .assignments
        .iter()
        .map(|a| (a.animal_id.as_str(), a.group_id))
        .collect();
    let entry_of: HashMap<&str, usize> = result
        .assignments
        .iter()
        .map(|a| (a.animal_id.as_str(), a.entry_index.unwrap()))
        .collect();

    for (position, decision) in record.decisions.iter().enumerate() {
        assert_eq!(decision.entry_index, position + 1, "log is in entry order");
        assert_eq!(
            entry_of[decision.animal_id.as_str()],
            decision.entry_index,
            "the exported grouping and the log must agree on when an animal entered"
        );
        assert_eq!(
            group_of[decision.animal_id.as_str()],
            decision.group_id,
            "the log must name the group the animal actually landed in"
        );

        let best = decision
            .scores
            .iter()
            .flatten()
            .cloned()
            .fold(f64::INFINITY, f64::min);
        let chosen = decision.scores[decision.group_id]
            .expect("the chosen group was a candidate, so it was scored");

        if decision.took_minimizer {
            assert!(
                chosen <= best + SCORE_EPSILON,
                "step {} claims a minimizer but scored {chosen} against a best of {best}",
                decision.entry_index
            );
        } else {
            assert!(
                chosen > best + SCORE_EPSILON,
                "step {} claims the 1 - p branch but landed on a minimizer",
                decision.entry_index
            );
        }
        assert_eq!(decision.levels.len(), 2, "one level per covariate");
    }
}

#[test]
fn the_record_names_the_rule_it_ran() {
    let result = run(
        dataset_60f(),
        group_config(constraints(&[(0, 20); 3]), 8, &[BW], 0.75),
        stat_config(&[BW]),
    );
    let record = record_of(&result);

    assert_eq!(record.allocation_probability, 0.75);
    assert_eq!(record.imbalance_measure, IMBALANCE_MEASURE);
    assert_eq!(record.allocation_rule, ALLOCATION_RULE);
    assert_eq!(record.binning, CovariateBinning::Tertiles.as_str());

    let outer = result.randomization.as_ref().unwrap();
    assert_eq!(outer.attempts, 1, "minimization allocates in one pass");
    assert!(outer.acceptance.is_none());
    assert!(outer.primary_indicator.is_none());
    assert_eq!(outer.rng_algorithm, randomizer::RNG_ALGORITHM);
    assert_eq!(outer.seed, 8);
}

/// Minimization publishes 入组顺序, never a per-animal draw: there is no "sort by this
/// column and deal" check to offer, and the export must not imply one.
#[test]
fn no_per_animal_draw_is_published() {
    let result = run(
        dataset_60f(),
        group_config(constraints(&[(0, 20); 3]), 13, &[BW], 0.8),
        stat_config(&[BW]),
    );

    for assignment in &result.assignments {
        assert!(assignment.random_number.is_none());
        assert!(assignment.block_index.is_none());
        assert!(assignment.entry_index.is_some());
    }

    let orders: std::collections::HashSet<usize> = result
        .assignments
        .iter()
        .map(|a| a.entry_index.unwrap())
        .collect();
    assert_eq!(orders.len(), 60, "entry order is a permutation");
    assert_eq!(orders.iter().min(), Some(&1));
    assert_eq!(orders.iter().max(), Some(&60));
}

// -------------------------------------------------------------------- validation

fn rejection(mutate: impl FnOnce(&mut GroupConfig)) -> String {
    let mut config = group_config(constraints(&[(0, 20); 3]), 1, &[BW], 0.8);
    mutate(&mut config);

    compute_grouping(dataset_60f(), config, stat_config(&[BW]))
        .expect_err("this configuration must be rejected")
        .to_string()
}

fn with_minimization(config: &mut GroupConfig, mutate: impl FnOnce(&mut MinimizationConfig)) {
    let randomization = config.randomization.as_mut().unwrap();
    mutate(randomization.minimization.as_mut().unwrap());
}

#[test]
fn an_empty_covariate_list_is_rejected() {
    let err = rejection(|config| with_minimization(config, |m| m.covariates.clear()));
    assert!(err.contains("协变量"), "{err}");
}

#[test]
fn a_duplicated_covariate_is_rejected() {
    let err = rejection(|config| {
        with_minimization(config, |m| m.covariates.push(BW.to_string()));
    });
    assert!(err.contains("重复"), "{err}");
}

#[test]
fn an_unknown_covariate_is_rejected() {
    let err = rejection(|config| {
        with_minimization(config, |m| m.covariates = vec!["不存在的指标".to_string()]);
    });
    assert!(err.contains("不在本次数据的指标列中"), "{err}");
}

#[test]
fn a_covariate_with_missing_values_names_the_animals() {
    let mut dataset = dataset_60f();
    dataset.animals[3].indicators.remove(BW);
    dataset.animals[17].indicators.remove(BW);

    let err = compute_grouping(
        dataset.clone(),
        group_config(constraints(&[(0, 20); 3]), 1, &[BW], 0.8),
        stat_config(&[CD45]),
    )
    .expect_err("a covariate with holes cannot be binned")
    .to_string();

    assert!(err.contains("缺失 2 只动物"), "{err}");
    assert!(
        err.contains(&dataset.animals[3].id),
        "the error has to name the animals: {err}"
    );
}

/// A covariate that is constant inside every stratum would let the run allocate by coin
/// flip while the export claimed it balanced on covariates.
#[test]
fn a_covariate_without_discriminating_power_is_rejected() {
    let dataset = synthetic(&[Sex::Female; 9], &[7.0; 9]);
    let err = compute_grouping(
        dataset,
        group_config(constraints(&[(0, 3); 3]), 1, &["x"], 0.8),
        stat_config(&["x"]),
    )
    .expect_err("a constant covariate must be refused")
    .to_string();

    assert!(err.contains("区分度"), "{err}");
}

#[test]
fn the_allocation_probability_must_be_strictly_inside_zero_and_one() {
    for p in [0.0, 1.0, -0.1, 1.5, f64::NAN] {
        let err = rejection(|config| {
            with_minimization(config, |m| m.allocation_probability = p);
        });
        assert!(err.contains("分配概率"), "p = {p} must be rejected: {err}");
    }
}

#[test]
fn a_primary_indicator_or_acceptance_criterion_is_a_conflict_not_a_no_op() {
    let err = rejection(|config| {
        config.randomization.as_mut().unwrap().primary_indicator = Some(BW.to_string());
    });
    assert!(err.contains("主指标"), "{err}");

    let err = rejection(|config| {
        config.randomization.as_mut().unwrap().acceptance = Some(AcceptanceCriterion::AlphaLine);
    });
    assert!(err.contains("接受准则"), "{err}");
}

#[test]
fn minimization_without_its_parameters_is_rejected() {
    let err = rejection(|config| {
        config.randomization.as_mut().unwrap().minimization = None;
    });
    assert!(err.contains("协变量"), "{err}");

    let err = rejection(|config| config.randomization = None);
    assert!(err.contains("随机化参数"), "{err}");
}

/// The mirror image: no other method may carry covariates and pretend they did anything.
#[test]
fn other_methods_may_not_carry_minimization_parameters() {
    let mut config = group_config(constraints(&[(0, 20); 3]), 1, &[BW], 0.8);
    config.method = GroupingMethod::Random;

    let err = compute_grouping(dataset_60f(), config, stat_config(&[BW]))
        .expect_err("covariates on a non-minimization run must be refused")
        .to_string();

    assert!(err.contains("只有最小化法"), "{err}");
}

/// Allocation concealment: under GLP there is one draw and no redraw entry point, and a
/// hand-built IPC request must not get one either.
#[test]
fn glp_submission_still_refuses_a_redraw() {
    let err = rejection(|config| {
        config.scenario = StudyScenario::GlpSubmission;
        config.randomization.as_mut().unwrap().draw_index = 2;
    });
    assert!(err.contains("分配隐藏"), "{err}");
}

#[test]
fn minimization_runs_under_every_scenario_that_allows_it() {
    for scenario in [
        StudyScenario::GlpSubmission,
        StudyScenario::ConfirmatoryTrial,
        StudyScenario::Exploratory,
    ] {
        let mut config = group_config(constraints(&[(0, 20); 3]), 77, &[BW], 0.8);
        config.scenario = scenario;

        let result = compute_grouping(dataset_60f(), config, stat_config(&[BW]))
            .unwrap_or_else(|e| panic!("{scenario:?} must allow minimization: {e}"));

        assert_eq!(result.total_evaluated, 1, "one pass, not a search");
        assert_eq!(result.candidates.len(), 1);
        assert_eq!(result.candidates[0].method, GroupingMethod::Minimization);
    }
}
