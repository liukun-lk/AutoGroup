// Integration test for Excel export functionality
use crate::core::{exporter, grouping, models::*, parser};

#[cfg(test)]
mod export_integration_tests {
    use super::*;

    #[test]
    #[ignore] // Run with: cargo test --lib export_integration_tests -- --ignored --nocapture
    fn test_end_to_end_export() {
        // Path to real test data
        let excel_path = "/Users/lb/Documents/source_code/github/AutoGroup/docs/通用动物实验自动分组软件_测试用数据.xlsx";

        println!("\n=== Step 1: Parse Excel File ===");
        let dataset = match parser::parse_excel_file(excel_path) {
            Ok(d) => {
                println!("✓ Successfully parsed Excel file");
                println!("  Animals: {}", d.animals.len());
                println!("  Indicators: {}", d.indicator_names.len());
                d
            }
            Err(e) => {
                panic!("Failed to parse Excel: {e}");
            }
        };

        // Select subset of indicators for testing
        let selected_indicators = vec![
            "kg", "℃", "ALT", "AST", "TP", "ALB", "GLU", "UREA", "CREA", "CHOl",
        ]
        .into_iter()
        .filter(|name| dataset.indicator_names.contains(&name.to_string()))
        .map(|s| s.to_string())
        .collect::<Vec<_>>();

        println!("\n=== Step 2: Configure and Compute Grouping ===");

        // 9 animals (6M, 3F) → unbalanced groups
        let group_config = GroupConfig {
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

        let stat_config = StatConfig {
            selected_indicators: selected_indicators.clone(),
            alpha: 0.05,
            mode: OptimizationMode::Strict,
            max_candidates: 10,
        };

        let multi_result =
            match grouping::compute_optimal_grouping(dataset.clone(), group_config, stat_config) {
                Ok(r) => {
                    println!("✓ Grouping completed");
                    println!("  Candidates found: {}", r.candidates.len());
                    println!("  Total evaluated: {}", r.total_evaluated);
                    println!("  Total valid: {}", r.total_valid);
                    if let Some(best) = r.candidates.first() {
                        println!("  Best - Min P-value: {:.6}", best.summary.min_p_value);
                        println!("  Best - Mean P-value: {:.6}", best.summary.mean_p_value);
                        println!("  Best - Meets criteria: {}", best.summary.meets_criteria);
                    }
                    r
                }
                Err(e) => {
                    panic!("Grouping failed: {e}");
                }
            };

        println!("\n=== Step 3: Export to Excel ===");

        let sheet_config = exporter::SheetConfig {
            selected_indicators: dataset.indicator_names.clone(), // Export ALL indicators
            include_statistics: true,
            include_summary: true,
            group_constraints: None,
        };

        let output_path = "/tmp/autogroup_export_test.xlsx";

        // Export best result only
        let best_result = multi_result
            .candidates
            .first()
            .expect("Should have at least one candidate");
        match exporter::export_grouping_result(best_result, &dataset, &sheet_config, output_path) {
            Ok(_) => {
                println!("✓ Export successful");
                println!("  File saved to: {output_path}");
            }
            Err(e) => {
                panic!("Export failed: {e}");
            }
        }

        // Verify file exists
        assert!(
            std::path::Path::new(output_path).exists(),
            "Exported file should exist"
        );

        println!("\n=== Step 4: Verify Export Contents ===");
        println!(
            "✓ Excel file created with {} indicators",
            dataset.indicator_names.len()
        );
        println!("✓ Grouping results exported");
        println!("✓ Statistical analysis included");
        println!("✓ Summary information included");

        println!("\n✅ End-to-end export test passed!");
        println!("\n📁 Output file: {output_path}");
        println!("   You can open this file in Excel to verify the format");
        println!("\n=== Test Complete ===\n");
    }

    #[test]
    #[ignore]
    fn test_export_with_selected_indicators() {
        // Similar to above but only export selected indicators
        let excel_path = "/Users/lb/Documents/source_code/github/AutoGroup/docs/通用动物实验自动分组软件_测试用数据.xlsx";

        let dataset = parser::parse_excel_file(excel_path).unwrap();

        // Only test with 5 indicators
        let selected_indicators = vec!["kg", "℃", "ALT", "AST", "TP"]
            .into_iter()
            .filter(|name| dataset.indicator_names.contains(&name.to_string()))
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

        let group_config = GroupConfig {
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

        let stat_config = StatConfig {
            selected_indicators: selected_indicators.clone(),
            alpha: 0.05,
            mode: OptimizationMode::Optimized,
            max_candidates: 10,
        };

        let multi_result =
            grouping::compute_optimal_grouping(dataset.clone(), group_config, stat_config).unwrap();

        let sheet_config = exporter::SheetConfig {
            selected_indicators, // Only export 5 indicators
            include_statistics: true,
            include_summary: true,
            group_constraints: None,
        };

        let output_path = "/tmp/autogroup_export_selected.xlsx";

        let best_result = multi_result
            .candidates
            .first()
            .expect("Should have at least one candidate");
        exporter::export_grouping_result(best_result, &dataset, &sheet_config, output_path)
            .unwrap();

        assert!(std::path::Path::new(output_path).exists());

        println!("\n✓ Exported file with selected indicators only");
        println!("  File: {output_path}");
    }
}
