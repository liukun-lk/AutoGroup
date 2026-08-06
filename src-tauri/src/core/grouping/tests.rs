// Integration tests for the complete grouping pipeline
use crate::core::grouping::evaluator;
use crate::core::{grouping, models::*};
use std::collections::HashMap;

#[cfg(test)]
mod integration_tests {
    use super::*;

    #[test]
    fn test_end_to_end_grouping() {
        // Create test dataset: 10 animals (6M, 4F) with 3 indicators
        let mut animals = Vec::new();

        // Males
        for i in 0..6 {
            let mut indicators = HashMap::new();
            indicators.insert("Weight".to_string(), 30.0 + i as f64);
            indicators.insert("Temperature".to_string(), 38.0 + (i as f64) * 0.1);
            indicators.insert("Glucose".to_string(), 5.0 + (i as f64) * 0.5);

            animals.push(Animal {
                id: format!("M{:03}", i + 1),
                sex: Sex::Male,
                indicators,
            });
        }

        // Females
        for i in 0..4 {
            let mut indicators = HashMap::new();
            indicators.insert("Weight".to_string(), 28.0 + i as f64);
            indicators.insert("Temperature".to_string(), 38.0 + (i as f64) * 0.1);
            indicators.insert("Glucose".to_string(), 4.5 + (i as f64) * 0.5);

            animals.push(Animal {
                id: format!("F{:03}", i + 1),
                sex: Sex::Female,
                indicators,
            });
        }

        let dataset = Dataset {
            indicator_names: vec![
                "Weight".to_string(),
                "Temperature".to_string(),
                "Glucose".to_string(),
            ],
            indicator_metadata: vec![
                IndicatorMetadata::new(
                    "Weight".to_string(),
                    "Weight".to_string(),
                    "kg".to_string(),
                ),
                IndicatorMetadata::new(
                    "Temperature".to_string(),
                    "Temperature".to_string(),
                    "℃".to_string(),
                ),
                IndicatorMetadata::new(
                    "Glucose".to_string(),
                    "Glucose".to_string(),
                    "mmol/L".to_string(),
                ),
            ],
            metadata: DatasetMetadata {
                total_animals: 10,
                male_count: 6,
                female_count: 4,
                indicator_count: 3,
            },
            animals,
        };

        // Config: 2 groups, 5 animals each (3M+2F)
        let group_config = GroupConfig {
            num_groups: 2,
            animals_per_group: GroupSize::Uniform { value: 5 },
            sex_constraints: vec![
                SexConstraint {
                    group_index: 0,
                    male_count: 3,
                    female_count: 2,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 3,
                    female_count: 2,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                },
            ],
        };

        let stat_config = StatConfig {
            selected_indicators: vec![
                "Weight".to_string(),
                "Temperature".to_string(),
                "Glucose".to_string(),
            ],
            alpha: 0.05,
            mode: OptimizationMode::Strict,
            max_candidates: 10,
        };

        // Run the complete pipeline
        let multi_result = grouping::compute_optimal_grouping(dataset, group_config, stat_config);

        // Should find at least one valid grouping
        assert!(multi_result.is_ok(), "Should find a valid grouping");

        let multi_result = multi_result.unwrap();
        assert!(
            !multi_result.candidates.is_empty(),
            "Should have at least one candidate"
        );

        let result = &multi_result.candidates[0];

        // Validate result structure
        assert_eq!(result.assignments.len(), 10, "Should have 10 assignments");
        assert_eq!(result.statistics.len(), 3, "Should have 3 indicator stats");

        // Check that all animals are assigned
        let mut assigned_ids: Vec<String> = result
            .assignments
            .iter()
            .map(|a| a.animal_id.clone())
            .collect();
        assigned_ids.sort();

        let mut expected_ids: Vec<String> = (0..6)
            .map(|i| format!("M{:03}", i + 1))
            .chain((0..4).map(|i| format!("F{:03}", i + 1)))
            .collect();
        expected_ids.sort();

        assert_eq!(assigned_ids, expected_ids);

        // Validate group distribution
        let group0_count = result
            .assignments
            .iter()
            .filter(|a| a.group_id == 0)
            .count();
        let group1_count = result
            .assignments
            .iter()
            .filter(|a| a.group_id == 1)
            .count();

        assert_eq!(group0_count, 5);
        assert_eq!(group1_count, 5);

        // Print result summary
        println!("=== Grouping Result ===");
        println!("Min P-value: {:.6}", result.summary.min_p_value);
        println!("Mean P-value: {:.6}", result.summary.mean_p_value);
        println!(
            "Invalid indicators: {}",
            result.summary.num_invalid_indicators
        );
        println!("Meets criteria: {}", result.summary.meets_criteria);
        println!("Computation time: {}ms", result.computation_time_ms);

        println!("\n=== Indicator Statistics ===");
        for stat in &result.statistics {
            println!(
                "{}: Levene P={:.6}, Diff P={:.6} ({})",
                stat.indicator_name, stat.levene_p_value, stat.diff_p_value, stat.test_method
            );
        }

        println!("\n=== Group Assignments ===");
        for assignment in &result.assignments {
            println!(
                "{} ({}) → Group {}",
                assignment.animal_id,
                assignment.sex.to_char(),
                assignment.group_id
            );
        }
    }

    #[test]
    fn test_optimized_mode() {
        // Create a dataset where strict mode might fail
        let mut animals = Vec::new();

        // Create animals with some variance
        for i in 0..6 {
            let mut indicators = HashMap::new();
            // Make one indicator highly variable
            indicators.insert("Var1".to_string(), 10.0 + (i as f64) * 5.0);
            indicators.insert("Var2".to_string(), 20.0 + (i as f64) * 0.5);

            animals.push(Animal {
                id: format!("M{i}"),
                sex: Sex::Male,
                indicators,
            });
        }

        for i in 0..4 {
            let mut indicators = HashMap::new();
            indicators.insert("Var1".to_string(), 15.0 + (i as f64) * 5.0);
            indicators.insert("Var2".to_string(), 20.0 + (i as f64) * 0.5);

            animals.push(Animal {
                id: format!("F{i}"),
                sex: Sex::Female,
                indicators,
            });
        }

        let dataset = Dataset {
            indicator_names: vec!["Var1".to_string(), "Var2".to_string()],
            indicator_metadata: vec![
                IndicatorMetadata::new("Var1".to_string(), "Var1".to_string(), String::new()),
                IndicatorMetadata::new("Var2".to_string(), "Var2".to_string(), String::new()),
            ],
            metadata: DatasetMetadata {
                total_animals: 10,
                male_count: 6,
                female_count: 4,
                indicator_count: 2,
            },
            animals,
        };

        let group_config = GroupConfig {
            num_groups: 2,
            animals_per_group: GroupSize::Uniform { value: 5 },
            sex_constraints: vec![
                SexConstraint {
                    group_index: 0,
                    male_count: 3,
                    female_count: 2,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 3,
                    female_count: 2,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                },
            ],
        };

        // Try optimized mode (allow 1 indicator to fail)
        let stat_config = StatConfig {
            selected_indicators: vec!["Var1".to_string(), "Var2".to_string()],
            alpha: 0.05,
            mode: OptimizationMode::Optimized,
            max_candidates: 10,
        };

        let multi_result = grouping::compute_optimal_grouping(dataset, group_config, stat_config);

        assert!(
            multi_result.is_ok(),
            "Optimized mode should find a grouping"
        );

        let multi_result = multi_result.unwrap();
        assert!(!multi_result.candidates.is_empty());

        let result = &multi_result.candidates[0];
        println!("Optimized mode result:");
        println!(
            "  Invalid indicators: {}",
            result.summary.num_invalid_indicators
        );
        assert!(result.summary.num_invalid_indicators <= 1);
    }

    /// The engine scores every candidate with a lightweight pass and only materializes
    /// full results for the winners. This asserts the shortcut is exact: the reported
    /// Top-N must equal what fully evaluating every candidate and sorting would produce.
    #[test]
    fn test_top_candidates_match_full_evaluation() {
        for mode in [OptimizationMode::Strict, OptimizationMode::Optimized] {
            let (dataset, group_config, stat_config) = three_group_fixture(mode);

            // Reference: fully evaluate every candidate, then filter and sort.
            let candidates =
                grouping::enumerator::enumerate_all(&dataset.animals, &group_config).unwrap();
            let mut reference: Vec<GroupingResult> = candidates
                .iter()
                .filter_map(|c| {
                    grouping::evaluator::evaluate_grouping_with_constraints(
                        c,
                        &dataset,
                        &stat_config,
                        Some(&group_config.sex_constraints),
                    )
                    .ok()
                })
                .filter(|r| match stat_config.mode {
                    OptimizationMode::Strict => r.summary.num_invalid_indicators == 0,
                    OptimizationMode::Optimized => r.summary.num_invalid_indicators <= 1,
                })
                .collect();

            reference.sort_by(|a, b| {
                b.summary
                    .min_p_value
                    .partial_cmp(&a.summary.min_p_value)
                    .unwrap()
                    .then_with(|| {
                        b.summary
                            .mean_p_value
                            .partial_cmp(&a.summary.mean_p_value)
                            .unwrap()
                    })
            });

            let actual = grouping::compute_optimal_grouping(
                dataset.clone(),
                group_config.clone(),
                stat_config.clone(),
            )
            .unwrap();

            assert_eq!(
                actual.total_valid,
                reference.len(),
                "{mode:?}: valid candidate count must match"
            );
            assert_eq!(
                actual.candidates.len(),
                stat_config.max_candidates.min(reference.len()),
                "{mode:?}: Top-N size must match"
            );

            for (rank, (got, want)) in actual.candidates.iter().zip(&reference).enumerate() {
                assert_eq!(
                    got.assignments, want.assignments,
                    "{mode:?}: rank {rank} assignment mismatch"
                );
                assert_eq!(
                    got.summary.min_p_value, want.summary.min_p_value,
                    "{mode:?}: rank {rank} min_p_value mismatch"
                );
                assert_eq!(
                    got.summary.mean_p_value, want.summary.mean_p_value,
                    "{mode:?}: rank {rank} mean_p_value mismatch"
                );
                assert_eq!(
                    got.summary.num_invalid_indicators, want.summary.num_invalid_indicators,
                    "{mode:?}: rank {rank} invalid count mismatch"
                );
                assert_eq!(
                    got.summary.total_indicators, want.summary.total_indicators,
                    "{mode:?}: rank {rank} total indicator count mismatch"
                );
            }
        }
    }

    /// `summary.num_groups` reports experimental groups only, so an (often empty)
    /// reserve group never inflates the group count shown in the UI and the export.
    #[test]
    fn test_summary_num_groups_excludes_reserve() {
        let (mut dataset, mut group_config, stat_config) =
            three_group_fixture(OptimizationMode::Optimized);

        // Add one reserve animal on top of the 9 experimental ones.
        let mut indicators = HashMap::new();
        indicators.insert("Weight".to_string(), 31.0);
        indicators.insert("Glucose".to_string(), 5.1);
        dataset.animals.push(Animal {
            id: "R001".to_string(),
            sex: Sex::Male,
            indicators,
        });
        dataset.metadata.total_animals = 10;
        dataset.metadata.male_count = 7;

        group_config.num_groups = 4;
        group_config.sex_constraints.push(SexConstraint {
            group_index: 3,
            male_count: 1,
            female_count: 0,
            group_type: GroupType::Reserve,
            custom_name: Some("备用动物".to_string()),
        });

        let result =
            grouping::compute_optimal_grouping(dataset, group_config, stat_config).unwrap();
        let summary = &result.candidates[0].summary;

        assert_eq!(summary.num_groups, 3, "reserve group must not be counted");
        assert_eq!(summary.total_animals, 10);
        assert_eq!(
            result.candidates[0].assignments.len(),
            10,
            "the reserve animal is still assigned"
        );
    }

    /// Post-hoc comparisons must be labelled with the caller's `group_id`, not with the
    /// index inside the compacted list of experimental groups.
    ///
    /// The statistics skip reserve groups, so with a reserve group sitting *between* two
    /// experimental ones the two numbering schemes diverge: experimental groups 0, 1, 2 are
    /// `group_id` 0, 2, 3. Reporting the compacted index would tell the user that "组1 vs
    /// 组2" differed when the comparison was actually between 组1 and 组3.
    ///
    /// The candidate is built directly rather than enumerated, so the test cannot start
    /// passing vacuously because no valid grouping was found.
    #[test]
    fn posthoc_comparisons_use_original_group_ids() {
        let (dataset, _, mut stat_config) = three_group_fixture(OptimizationMode::Optimized);
        stat_config.selected_indicators = vec!["Weight".to_string()];

        // Animals 0..=5 are male, 6..=8 female. Three experimental groups of 1M + 1F, with
        // the three remaining males parked in a reserve group at index 1.
        let candidate = CandidateGrouping {
            groups: vec![vec![0, 6], vec![1, 2, 3], vec![4, 7], vec![5, 8]],
        };

        let constraints = vec![
            SexConstraint {
                group_index: 0,
                male_count: 1,
                female_count: 1,
                group_type: GroupType::Experimental,
                custom_name: None,
            },
            SexConstraint {
                group_index: 1,
                male_count: 3,
                female_count: 0,
                group_type: GroupType::Reserve,
                custom_name: Some("备用动物".to_string()),
            },
            SexConstraint {
                group_index: 2,
                male_count: 1,
                female_count: 1,
                group_type: GroupType::Experimental,
                custom_name: None,
            },
            SexConstraint {
                group_index: 3,
                male_count: 1,
                female_count: 1,
                group_type: GroupType::Experimental,
                custom_name: None,
            },
        ];

        let result = evaluator::evaluate_grouping_with_constraints(
            &candidate,
            &dataset,
            &stat_config,
            Some(&constraints),
        )
        .unwrap();

        assert_eq!(result.summary.num_groups, 3, "reserve must not be counted");

        let stat = result
            .statistics
            .first()
            .expect("Weight must have been tested");
        let comparisons = stat
            .posthoc_results
            .as_ref()
            .expect("three experimental groups produce post-hoc comparisons");

        let mut pairs: Vec<(usize, usize)> = comparisons
            .iter()
            .map(|c| (c.group1_id, c.group2_id))
            .collect();
        pairs.sort_unstable();

        assert_eq!(
            pairs,
            vec![(0, 2), (0, 3), (2, 3)],
            "post-hoc pairs must reference group_id, skipping the reserve group at index 1"
        );
    }

    /// 9 animals (6M + 3F) into 3 experimental groups of 2M + 1F.
    fn three_group_fixture(mode: OptimizationMode) -> (Dataset, GroupConfig, StatConfig) {
        let weights = [30.6, 33.5, 34.0, 34.1, 30.3, 36.5, 31.9, 30.4, 31.2];
        let glucose = [5.0, 4.6, 5.3, 4.9, 5.1, 4.7, 5.2, 4.8, 5.0];

        let animals: Vec<Animal> = (0..9)
            .map(|i| {
                let mut indicators = HashMap::new();
                indicators.insert("Weight".to_string(), weights[i]);
                indicators.insert("Glucose".to_string(), glucose[i]);
                Animal {
                    id: format!("A{:03}", i + 1),
                    sex: if i < 6 { Sex::Male } else { Sex::Female },
                    indicators,
                }
            })
            .collect();

        let dataset = Dataset {
            indicator_names: vec!["Weight".to_string(), "Glucose".to_string()],
            indicator_metadata: vec![
                IndicatorMetadata::new(
                    "Weight".to_string(),
                    "Weight".to_string(),
                    "kg".to_string(),
                ),
                IndicatorMetadata::new(
                    "Glucose".to_string(),
                    "Glucose".to_string(),
                    "mmol/L".to_string(),
                ),
            ],
            metadata: DatasetMetadata {
                total_animals: 9,
                male_count: 6,
                female_count: 3,
                indicator_count: 2,
            },
            animals,
        };

        let group_config = GroupConfig {
            num_groups: 3,
            animals_per_group: GroupSize::Uniform { value: 3 },
            sex_constraints: (0..3)
                .map(|i| SexConstraint {
                    group_index: i,
                    male_count: 2,
                    female_count: 1,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                })
                .collect(),
        };

        let stat_config = StatConfig {
            selected_indicators: vec!["Weight".to_string(), "Glucose".to_string()],
            alpha: 0.05,
            mode,
            max_candidates: 10,
        };

        (dataset, group_config, stat_config)
    }
}
