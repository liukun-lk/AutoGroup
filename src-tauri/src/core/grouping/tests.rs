// Integration tests for the complete grouping pipeline
use crate::core::{grouping, models::*, parser};
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
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 3,
                    female_count: 2,
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
        };

        // Run the complete pipeline
        let result = grouping::compute_optimal_grouping(dataset, group_config, stat_config);

        // Should find at least one valid grouping
        assert!(result.is_ok(), "Should find a valid grouping");

        let result = result.unwrap();

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
        println!("Invalid indicators: {}", result.summary.num_invalid_indicators);
        println!("Meets criteria: {}", result.summary.meets_criteria);
        println!("Computation time: {}ms", result.computation_time_ms);

        println!("\n=== Indicator Statistics ===");
        for stat in &result.statistics {
            println!(
                "{}: P={:.6} ({})",
                stat.indicator_name, stat.p_value, stat.test_method
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
                id: format!("M{}", i),
                sex: Sex::Male,
                indicators,
            });
        }

        for i in 0..4 {
            let mut indicators = HashMap::new();
            indicators.insert("Var1".to_string(), 15.0 + (i as f64) * 5.0);
            indicators.insert("Var2".to_string(), 20.0 + (i as f64) * 0.5);

            animals.push(Animal {
                id: format!("F{}", i),
                sex: Sex::Female,
                indicators,
            });
        }

        let dataset = Dataset {
            indicator_names: vec!["Var1".to_string(), "Var2".to_string()],
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
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 3,
                    female_count: 2,
                },
            ],
        };

        // Try optimized mode (allow 1 indicator to fail)
        let stat_config = StatConfig {
            selected_indicators: vec!["Var1".to_string(), "Var2".to_string()],
            alpha: 0.05,
            mode: OptimizationMode::Optimized,
        };

        let result = grouping::compute_optimal_grouping(dataset, group_config, stat_config);

        assert!(result.is_ok(), "Optimized mode should find a grouping");

        let result = result.unwrap();
        println!("Optimized mode result:");
        println!("  Invalid indicators: {}", result.summary.num_invalid_indicators);
        assert!(result.summary.num_invalid_indicators <= 1);
    }
}
