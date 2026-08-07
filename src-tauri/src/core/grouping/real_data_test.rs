// Real-world data integration test
use crate::core::{grouping, models::*, parser, validator};

#[cfg(test)]
mod real_data_test {
    use super::*;

    #[test]
    #[ignore] // Run with: cargo test --lib real_data_test -- --ignored --nocapture
    fn test_with_real_excel_data() {
        // Path to the real test data
        // Repo-relative so the test runs from any checkout, not just one machine.
        let excel_path = &format!(
            "{}/../docs/通用动物实验自动分组软件_测试用数据.xlsx",
            env!("CARGO_MANIFEST_DIR")
        );

        println!("\n=== Step 1: Parse Excel File ===");
        let dataset = match parser::parse_excel_file(excel_path) {
            Ok(d) => {
                println!("✓ Successfully parsed Excel file");
                println!("  Animals: {}", d.animals.len());
                println!("  Indicators: {}", d.indicator_names.len());
                println!("  Males: {}", d.metadata.male_count);
                println!("  Females: {}", d.metadata.female_count);
                d
            }
            Err(e) => {
                panic!("Failed to parse Excel: {e}");
            }
        };

        // Validate dataset
        println!("\n=== Step 2: Validate Dataset ===");
        match validator::validate_dataset(&dataset) {
            Ok(_) => println!("✓ Dataset validation passed"),
            Err(e) => {
                panic!("Dataset validation failed: {e}");
            }
        }

        // Print first few indicators
        println!("\n=== Available Indicators (first 20) ===");
        for (i, name) in dataset.indicator_names.iter().take(20).enumerate() {
            println!("  {}. {}", i + 1, name);
        }

        // Select a subset of common indicators for testing
        let selected_indicators = vec![
            "kg",   // Body weight
            "℃",    // Temperature
            "ALT",  // Alanine aminotransferase
            "AST",  // Aspartate aminotransferase
            "TP",   // Total protein
            "ALB",  // Albumin
            "GLU",  // Glucose
            "UREA", // Urea
            "CREA", // Creatinine
            "CHOl", // Cholesterol
        ]
        .into_iter()
        .filter(|name| dataset.indicator_names.contains(&name.to_string()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

        println!(
            "\n=== Step 3: Selected {} Indicators for Testing ===",
            selected_indicators.len()
        );
        for indicator in &selected_indicators {
            println!("  - {indicator}");
        }

        // Configure grouping based on actual animal counts
        // We have 9 animals (6M, 3F)
        // Option 1: Unbalanced groups (5+4)
        // Option 2: Exclude 1 animal (4+4 with 1 leftover)
        // Let's use Option 1: Group0(3M+2F=5), Group1(3M+1F=4)

        let group_config = GroupConfig {
            scenario: StudyScenario::Exploratory,
            method: GroupingMethod::Optimized,
            randomization: None,
            num_groups: 2,
            animals_per_group: GroupSize::Custom { values: vec![5, 4] },
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
                    female_count: 1,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                },
            ],
        };

        println!("\n=== Step 4: Grouping Configuration ===");
        println!("  Number of groups: {}", group_config.num_groups);
        println!("  Group 0: 5 animals (3M + 2F)");
        println!("  Group 1: 4 animals (3M + 1F)");
        println!("  Note: Unbalanced groups due to 9 total animals (6M + 3F)");

        // Statistical configuration
        let stat_config = StatConfig {
            selected_indicators,
            alpha: 0.05,
            mode: OptimizationMode::Strict,
            max_candidates: 10,
        };

        println!("\n=== Step 5: Statistical Configuration ===");
        println!("  Significance level (α): {}", stat_config.alpha);
        println!("  Optimization mode: Strict (all P > α)");
        println!("  Max candidates: {}", stat_config.max_candidates);
        println!(
            "  Number of indicators to test: {}",
            stat_config.selected_indicators.len()
        );

        // Run the grouping algorithm
        println!("\n=== Step 6: Computing Optimal Grouping ===");
        println!("  This may take a few seconds...");

        let start = std::time::Instant::now();
        let multi_result =
            match grouping::compute_optimal_grouping(dataset.clone(), group_config, stat_config) {
                Ok(r) => {
                    let elapsed = start.elapsed();
                    println!("✓ Grouping computation completed in {elapsed:?}");
                    println!("  Candidates found: {}", r.candidates.len());
                    println!("  Total evaluated: {}", r.total_evaluated);
                    r
                }
                Err(e) => {
                    panic!("Grouping computation failed: {e}");
                }
            };

        let result = multi_result
            .candidates
            .first()
            .expect("Should have at least one candidate");

        // Display results
        println!("\n=== RESULTS (Best Candidate) ===");
        println!("\n📊 Summary:");
        println!("  Min P-value:        {:.6}", result.summary.min_p_value);
        println!("  Mean P-value:       {:.6}", result.summary.mean_p_value);
        println!(
            "  Invalid indicators: {}",
            result.summary.num_invalid_indicators
        );
        println!(
            "  Meets criteria:     {}",
            if result.summary.meets_criteria {
                "✓ YES"
            } else {
                "✗ NO"
            }
        );
        println!("  Computation time:   {}ms", result.computation_time_ms);

        println!("\n📈 Indicator Statistics:");
        println!(
            "  {:<15} {:<12} {:<12} {:<8} {:<30}",
            "Indicator", "Levene P", "Diff P", "Valid", "Method"
        );
        println!("  {}", "-".repeat(85));
        for stat in &result.statistics {
            println!(
                "  {:<15} {:<12.6} {:<12.6} {:<8} {}",
                stat.indicator_name,
                stat.levene_p_value,
                stat.diff_p_value,
                if stat.is_valid { "✓" } else { "✗" },
                stat.test_method
            );
        }

        println!("\n👥 Group Assignments:");
        // Group by group_id
        for group_id in 0..2 {
            let group_members: Vec<_> = result
                .assignments
                .iter()
                .filter(|a| a.group_id == group_id)
                .collect();

            println!("\n  Group {group_id}:");
            let males = group_members.iter().filter(|a| a.sex == Sex::Male).count();
            let females = group_members
                .iter()
                .filter(|a| a.sex == Sex::Female)
                .count();
            println!(
                "    Size: {} animals ({}M + {}F)",
                group_members.len(),
                males,
                females
            );

            for assignment in group_members {
                // Get animal data to show indicator values
                let animal = dataset
                    .animals
                    .iter()
                    .find(|a| a.id == assignment.animal_id)
                    .unwrap();

                print!(
                    "    - {} ({}) ",
                    assignment.animal_id,
                    assignment.sex.to_char()
                );

                // Show first 3 indicator values
                for indicator_name in result.statistics.iter().take(3).map(|s| &s.indicator_name) {
                    if let Some(&value) = animal.indicators.get(indicator_name) {
                        print!(" {indicator_name}={value:.1}");
                    }
                }
                println!();
            }
        }

        // Assertions
        assert_eq!(result.assignments.len(), 9, "Should have 9 assignments");
        assert!(
            result.summary.min_p_value > 0.0,
            "Min P-value should be positive"
        );
        assert!(
            result.summary.mean_p_value > 0.0,
            "Mean P-value should be positive"
        );

        // Check if strict mode criteria met
        if result.summary.meets_criteria {
            assert_eq!(
                result.summary.num_invalid_indicators, 0,
                "Strict mode should have 0 invalid indicators when meets_criteria is true"
            );
        }

        println!("\n✅ All validations passed!");
        println!("\n=== Test Complete ===\n");
    }

    #[test]
    #[ignore] // Run with: cargo test --lib test_three_groups_real_data -- --ignored --nocapture
    fn test_three_groups_real_data() {
        // Path to the real test data
        // Repo-relative so the test runs from any checkout, not just one machine.
        let excel_path = &format!(
            "{}/../docs/通用动物实验自动分组软件_测试用数据.xlsx",
            env!("CARGO_MANIFEST_DIR")
        );

        println!("\n=== THREE-GROUP GROUPING TEST ===");
        println!("\n=== Step 1: Parse Excel File ===");
        let dataset = match parser::parse_excel_file(excel_path) {
            Ok(d) => {
                println!("✓ Successfully parsed Excel file");
                println!("  Animals: {}", d.animals.len());
                println!("  Males: {}", d.metadata.male_count);
                println!("  Females: {}", d.metadata.female_count);
                d
            }
            Err(e) => {
                panic!("Failed to parse Excel: {e}");
            }
        };

        // Select indicators
        let selected_indicators = vec!["kg", "℃", "ALT", "AST", "TP", "ALB", "GLU"]
            .into_iter()
            .filter(|name| dataset.indicator_names.contains(&name.to_string()))
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        println!(
            "\n=== Step 2: Selected {} Indicators ===",
            selected_indicators.len()
        );

        // Configure 3 groups: each with 2M+1F=3 animals
        // Total: 6M + 3F = 9 animals
        let group_config = GroupConfig {
            scenario: StudyScenario::Exploratory,
            method: GroupingMethod::Optimized,
            randomization: None,
            num_groups: 3,
            animals_per_group: GroupSize::Uniform { value: 3 },
            sex_constraints: vec![
                SexConstraint {
                    group_index: 0,
                    male_count: 2,
                    female_count: 1,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 2,
                    female_count: 1,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                },
                SexConstraint {
                    group_index: 2,
                    male_count: 2,
                    female_count: 1,
                    group_type: GroupType::Experimental,
                    custom_name: None,
                },
            ],
        };

        println!("\n=== Step 3: Grouping Configuration ===");
        println!("  Number of groups: {}", group_config.num_groups);
        println!("  Each group: 3 animals (2M + 1F)");
        println!("  Total animals used: 9 (6M + 3F)");

        // Statistical configuration - Use Optimized mode for 3-group case
        let stat_config = StatConfig {
            selected_indicators,
            alpha: 0.05,
            mode: OptimizationMode::Optimized, // Allow up to 1 invalid indicator
            max_candidates: 10,
        };

        println!("\n=== Step 4: Statistical Configuration ===");
        println!("  Significance level (α): {}", stat_config.alpha);
        println!("  Optimization mode: Optimized (allow ≤1 invalid)");
        println!("  Max candidates to return: {}", stat_config.max_candidates);
        println!("  Test method: ANOVA + Post-hoc (Tukey HSD or Dunnett's T3)");

        // Run the grouping algorithm
        println!("\n=== Step 5: Computing Optimal 3-Group Grouping ===");
        println!("  This may take several seconds...");

        let start = std::time::Instant::now();
        let multi_result =
            match grouping::compute_optimal_grouping(dataset.clone(), group_config, stat_config) {
                Ok(r) => {
                    let elapsed = start.elapsed();
                    println!("✓ Grouping computation completed in {elapsed:?}");
                    println!("  Candidates found: {}", r.candidates.len());
                    println!("  Total evaluated: {}", r.total_evaluated);
                    println!("  Total valid: {}", r.total_valid);
                    r
                }
                Err(e) => {
                    panic!("Grouping computation failed: {e}");
                }
            };

        // Get the best result
        let result = multi_result
            .candidates
            .first()
            .expect("Should have at least one candidate");

        // Display results
        println!("\n=== BEST RESULT (Rank #1) ===");
        println!("\n📊 Summary:");
        println!("  Min P-value:        {:.6}", result.summary.min_p_value);
        println!("  Mean P-value:       {:.6}", result.summary.mean_p_value);
        println!(
            "  Invalid indicators: {}",
            result.summary.num_invalid_indicators
        );
        println!(
            "  Meets criteria:     {}",
            if result.summary.meets_criteria {
                "✓ YES"
            } else {
                "✗ NO"
            }
        );
        println!("  Computation time:   {}ms", result.computation_time_ms);

        println!("\n📈 Indicator Statistics (with Post-hoc Results):");
        println!(
            "  {:<15} {:<12} {:<12} {:<8} {:<40}",
            "Indicator", "Levene P", "Diff P", "Valid", "Method"
        );
        println!("  {}", "-".repeat(95));
        for stat in &result.statistics {
            println!(
                "  {:<15} {:<12.6} {:<12.6} {:<8} {}",
                stat.indicator_name,
                stat.levene_p_value,
                stat.diff_p_value,
                if stat.is_valid { "✓" } else { "✗" },
                stat.test_method
            );

            // Show post-hoc pairwise comparisons if available
            if let Some(ref posthoc) = stat.posthoc_results {
                println!("    Pairwise comparisons:");
                for comparison in posthoc {
                    println!(
                        "      Group {} vs {}: P = {:.6} {}",
                        comparison.group1_id,
                        comparison.group2_id,
                        comparison.p_value,
                        if comparison.is_valid { "✓" } else { "✗" }
                    );
                }
            }
        }

        println!("\n👥 Group Assignments:");
        for group_id in 0..3 {
            let group_members: Vec<_> = result
                .assignments
                .iter()
                .filter(|a| a.group_id == group_id)
                .collect();

            println!("\n  Group {group_id}:");
            let males = group_members.iter().filter(|a| a.sex == Sex::Male).count();
            let females = group_members
                .iter()
                .filter(|a| a.sex == Sex::Female)
                .count();
            println!(
                "    Size: {} animals ({}M + {}F)",
                group_members.len(),
                males,
                females
            );

            for assignment in group_members {
                let animal = dataset
                    .animals
                    .iter()
                    .find(|a| a.id == assignment.animal_id)
                    .unwrap();

                print!(
                    "    - {} ({}) ",
                    assignment.animal_id,
                    assignment.sex.to_char()
                );

                // Show first 3 indicator values
                for indicator_name in result.statistics.iter().take(3).map(|s| &s.indicator_name) {
                    if let Some(&value) = animal.indicators.get(indicator_name) {
                        print!(" {indicator_name}={value:.1}");
                    }
                }
                println!();
            }
        }

        // Assertions
        assert_eq!(result.assignments.len(), 9, "Should have 9 assignments");
        assert!(
            result.summary.min_p_value > 0.0,
            "Min P-value should be positive"
        );

        // Verify that all groups have correct size
        for group_id in 0..3 {
            let group_size = result
                .assignments
                .iter()
                .filter(|a| a.group_id == group_id)
                .count();
            assert_eq!(group_size, 3, "Each group should have 3 animals");
        }

        // Verify post-hoc results exist for multi-group indicators
        for stat in &result.statistics {
            if stat.test_method.contains("ANOVA") {
                assert!(
                    stat.posthoc_results.is_some(),
                    "ANOVA indicators should have post-hoc results"
                );

                let posthoc = stat.posthoc_results.as_ref().unwrap();
                assert_eq!(
                    posthoc.len(),
                    3,
                    "Should have 3 pairwise comparisons for 3 groups: C(3,2)=3"
                );
            }
        }

        println!("\n✅ All 3-group validations passed!");
        println!("\n=== Three-Group Test Complete ===\n");
    }
}
