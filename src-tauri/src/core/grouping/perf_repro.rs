//! Reproduction harness for the "grouping into 3+ groups never finishes" report.
//!
//! Two groups produce a few hundred candidates; three groups produce 10^5–10^6. The
//! engine used to build a full `GroupingResult` (per-indicator statistics, post-hoc
//! comparisons, assignments — roughly 13 KB) for every one of them and keep them all
//! alive, so a 3-group run over 46 indicators peaked at ~1.5 GB and the machine spent
//! its time in the allocator instead of finishing.
//!
//! Ignored by default because these runs take seconds even when healthy. Run with:
//! `cargo test --release perf_ -- --nocapture --ignored`
use crate::core::{grouping, models::*};
use std::collections::HashMap;

fn build_dataset(num_males: usize, num_females: usize, num_indicators: usize) -> Dataset {
    let indicator_names: Vec<String> = (0..num_indicators).map(|i| format!("IND{i:02}")).collect();

    let mut animals = Vec::new();
    for i in 0..(num_males + num_females) {
        let mut indicators = HashMap::new();
        for (j, name) in indicator_names.iter().enumerate() {
            // Deterministic pseudo-random spread
            let v = ((i * 7919 + j * 104729) % 1000) as f64 / 10.0 + 20.0;
            indicators.insert(name.clone(), v);
        }
        animals.push(Animal {
            id: format!("A{i:03}"),
            sex: if i < num_males {
                Sex::Male
            } else {
                Sex::Female
            },
            indicators,
        });
    }

    let indicator_metadata = indicator_names
        .iter()
        .map(|n| IndicatorMetadata::new(n.clone(), n.clone(), "u".to_string()))
        .collect();

    Dataset {
        indicator_names,
        indicator_metadata,
        metadata: DatasetMetadata {
            total_animals: num_males + num_females,
            male_count: num_males,
            female_count: num_females,
            indicator_count: num_indicators,
        },
        animals,
    }
}

fn run_case(num_exp_groups: usize, males_per_group: usize, females_per_group: usize) {
    let num_males = num_exp_groups * males_per_group;
    let num_females = num_exp_groups * females_per_group;
    let dataset = build_dataset(num_males, num_females, 46);

    let mut sex_constraints: Vec<SexConstraint> = (0..num_exp_groups)
        .map(|i| SexConstraint {
            group_index: i,
            male_count: males_per_group,
            female_count: females_per_group,
            group_type: GroupType::Experimental,
            custom_name: None,
        })
        .collect();
    // Frontend always appends an (often empty) reserve group
    sex_constraints.push(SexConstraint {
        group_index: num_exp_groups,
        male_count: 0,
        female_count: 0,
        group_type: GroupType::Reserve,
        custom_name: Some("备用动物".to_string()),
    });

    let group_config = GroupConfig {
        scenario: StudyScenario::Exploratory,
        method: GroupingMethod::Optimized,
        randomization: None,
        num_groups: num_exp_groups + 1,
        animals_per_group: GroupSize::Uniform {
            value: males_per_group + females_per_group,
        },
        sex_constraints,
    };

    let stat_config = StatConfig {
        selected_indicators: dataset.indicator_names.clone(),
        alpha: 0.05,
        mode: OptimizationMode::Strict,
        max_candidates: 10,
    };

    let t0 = std::time::Instant::now();
    let result = grouping::compute_optimal_grouping(dataset, group_config, stat_config);
    let elapsed = t0.elapsed();

    let label = format!("case {num_exp_groups}x({males_per_group}M+{females_per_group}F)");
    let result = result.unwrap_or_else(|e| panic!("{label}: failed after {elapsed:?}: {e}"));

    println!(
        "{label}: evaluated={} valid={} elapsed={elapsed:?}",
        result.total_evaluated, result.total_valid
    );

    // Deliberately generous: this guards against the pathological blow-up (which took
    // minutes and gigabytes), not against ordinary machine-to-machine variation.
    assert!(
        elapsed.as_secs() < 60,
        "{label}: took {elapsed:?}, expected well under 60s"
    );
}

#[test]
#[ignore]
fn perf_two_groups() {
    run_case(2, 3, 3);
}

/// Exhaustive path: C(12,4)*C(3,1)*C(8,4)*C(2,1) = 207 900 candidates.
#[test]
#[ignore]
fn perf_three_groups_exhaustive() {
    run_case(3, 4, 1);
}

/// Monte Carlo path, matching the reported failure: 3 experimental groups of 6
/// (plus an empty reserve group) over 46 indicators — 100 000 sampled candidates.
#[test]
#[ignore]
fn perf_three_groups_sampled() {
    run_case(3, 3, 3);
}
