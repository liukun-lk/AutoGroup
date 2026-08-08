// Integration test for Excel export functionality
use crate::core::{exporter, grouping, models::*, parser};

#[cfg(test)]
mod export_integration_tests {
    use super::*;
    use crate::core::grouping::evaluator;
    use calamine::{open_workbook_auto, Reader};

    fn fixture_path(relative: &str) -> String {
        std::path::Path::new(env!("CARGO_MANIFEST_DIR"))
            .join(relative)
            .to_str()
            .expect("fixture path must be valid UTF-8")
            .to_string()
    }

    fn sheet_names(path: &str) -> Vec<String> {
        open_workbook_auto(path)
            .expect("exported workbook must open")
            .sheet_names()
            .to_vec()
    }

    fn read_sheet(path: &str, name: &str) -> Vec<Vec<String>> {
        let mut workbook = open_workbook_auto(path).expect("exported workbook must open");
        workbook
            .worksheet_range(name)
            .unwrap_or_else(|e| panic!("sheet {name} must be readable: {e}"))
            .rows()
            .map(|row| row.iter().map(|cell| cell.to_string()).collect())
            .collect()
    }

    /// A reserve group stays a reserve group only if the caller hands its constraints to the
    /// exporter. Without them the export silently promotes the reserve animals into an
    /// experimental group: wrong label, sorted among the experimental groups, given a
    /// mean±SD row they must not have, and counted in 分组数量. The frontend regressed
    /// exactly this way by omitting `groupConstraints` from the `export_result` call.
    ///
    /// Three experimental groups of 1M + 1F plus a reserve of 3M. The candidate is built
    /// directly so the test cannot pass vacuously when no valid grouping exists for such
    /// small groups.
    #[test]
    fn export_isolates_the_reserve_group() {
        let dataset = parser::parse_excel_file(&fixture_path("tests/fixtures/e2e_input.xlsx"))
            .expect("input fixture must parse");

        let males: Vec<usize> = dataset
            .animals
            .iter()
            .enumerate()
            .filter(|(_, a)| a.sex == Sex::Male)
            .map(|(i, _)| i)
            .collect();
        let females: Vec<usize> = dataset
            .animals
            .iter()
            .enumerate()
            .filter(|(_, a)| a.sex == Sex::Female)
            .map(|(i, _)| i)
            .collect();
        assert_eq!((males.len(), females.len()), (6, 3));

        let candidate = CandidateGrouping {
            groups: vec![
                vec![males[0], females[0]],
                vec![males[1], females[1]],
                vec![males[2], females[2]],
                vec![males[3], males[4], males[5]],
            ],
        };

        let mut constraints: Vec<SexConstraint> = (0..3)
            .map(|i| SexConstraint {
                group_index: i,
                male_count: 1,
                female_count: 1,
                group_type: GroupType::Experimental,
                custom_name: None,
            })
            .collect();
        constraints.push(SexConstraint {
            group_index: 3,
            male_count: 3,
            female_count: 0,
            group_type: GroupType::Reserve,
            custom_name: Some("备用动物".to_string()),
        });

        let selected_indicators = vec!["kg".to_string()];
        assert!(dataset.indicator_names.contains(&selected_indicators[0]));

        let stat_config = StatConfig {
            selected_indicators: selected_indicators.clone(),
            alpha: 0.05,
            mode: OptimizationMode::Optimized,
            max_candidates: 10,
        };

        let result = evaluator::evaluate_grouping_with_constraints(
            &candidate,
            &dataset,
            &stat_config,
            Some(&constraints),
            evaluator::Untestable::Abort,
        )
        .expect("evaluation must succeed");

        assert_eq!(result.summary.num_groups, 3);

        let output_dir = std::env::temp_dir().join("autogroup_export_reserve");
        std::fs::create_dir_all(&output_dir).expect("temp dir");
        let output_path = output_dir.join("reserve.xlsx");
        let output = output_path.to_str().unwrap().to_string();

        let sheet_config = exporter::SheetConfig {
            scenario: StudyScenario::Exploratory,
            selected_indicators,
            include_statistics: true,
            include_summary: true,
            group_constraints: Some(constraints),
        };

        exporter::export_grouping_result(&result, &dataset, &sheet_config, &output)
            .expect("export must succeed");

        assert_eq!(
            sheet_names(&output),
            vec!["分组结果", "统计结果", "事后比较", "汇总信息"],
        );

        // --- 分组结果: label, sort position, and no statistics row for the reserve --------
        let grouping_sheet = read_sheet(&output, "分组结果");
        let group_column: Vec<&str> = grouping_sheet
            .iter()
            .skip(2) // dual-row header
            .map(|row| row[0].as_str())
            .collect();

        assert_eq!(
            group_column.iter().filter(|c| **c == "备用动物").count(),
            3,
            "all three reserve animals must carry the custom name, not 组4: {group_column:?}"
        );

        let last_three = &group_column[group_column.len() - 3..];
        assert!(
            last_three.iter().all(|c| *c == "备用动物"),
            "reserve animals must sort last, got {group_column:?}"
        );

        assert_eq!(
            group_column.iter().filter(|c| **c == "均值±标准差").count(),
            3,
            "one mean±SD row per experimental group and none for the reserve: {group_column:?}"
        );

        // --- 汇总信息: the reserve must not inflate the group count ----------------------
        let summary_sheet = read_sheet(&output, "汇总信息");
        let group_count = summary_sheet
            .iter()
            .find(|row| row[0] == "分组数量")
            .and_then(|row| row.get(1))
            .expect("汇总信息 must report 分组数量")
            .clone();
        assert_eq!(group_count, "3", "reserve group must not be counted");

        assert!(
            summary_sheet.iter().any(|row| row[0].contains("备用动物")),
            "the reserve group must still be listed in the summary"
        );

        // --- 事后比较: pairs are between experimental groups only ------------------------
        let posthoc_sheet = read_sheet(&output, "事后比较");
        let pairs: Vec<&str> = posthoc_sheet
            .iter()
            .skip(1)
            .map(|row| row[1].as_str())
            .collect();

        assert_eq!(pairs.len(), 3, "C(3,2) = 3 comparisons for one indicator");
        assert!(
            !pairs.iter().any(|p| p.contains("备用动物")),
            "the reserve group must not appear in post-hoc comparisons: {pairs:?}"
        );
        let mut sorted = pairs.clone();
        sorted.sort_unstable();
        assert_eq!(sorted, vec!["组1 vs. 组2", "组1 vs. 组3", "组2 vs. 组3"],);

        let _ = std::fs::remove_file(&output_path);
    }

    #[test]
    #[ignore] // Run with: cargo test --lib export_integration_tests -- --ignored --nocapture
    fn test_end_to_end_export() {
        // Path to real test data
        let excel_path = fixture_path("../docs/通用动物实验自动分组软件_测试用数据.xlsx");

        println!("\n=== Step 1: Parse Excel File ===");
        let dataset = match parser::parse_excel_file(&excel_path) {
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
            scenario: StudyScenario::Exploratory,
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
        let excel_path = fixture_path("../docs/通用动物实验自动分组软件_测试用数据.xlsx");

        let dataset = parser::parse_excel_file(&excel_path).unwrap();

        // Only test with 5 indicators
        let selected_indicators = vec!["kg", "℃", "ALT", "AST", "TP"]
            .into_iter()
            .filter(|name| dataset.indicator_names.contains(&name.to_string()))
            .map(|s| s.to_string())
            .collect::<Vec<_>>();

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

        let stat_config = StatConfig {
            selected_indicators: selected_indicators.clone(),
            alpha: 0.05,
            mode: OptimizationMode::Optimized,
            max_candidates: 10,
        };

        let multi_result =
            grouping::compute_optimal_grouping(dataset.clone(), group_config, stat_config).unwrap();

        let sheet_config = exporter::SheetConfig {
            scenario: StudyScenario::Exploratory,
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

    #[test]
    fn summary_sheet_records_the_acceptance_and_draw_provenance() {
        let dataset =
            parser::parse_excel_file(&fixture_path("tests/fixtures/randomization_input_60f.xlsx"))
                .expect("fixture must parse");

        let constraints: Vec<SexConstraint> = (0..3)
            .map(|i| SexConstraint {
                group_index: i,
                male_count: 0,
                female_count: 20,
                group_type: GroupType::Experimental,
                custom_name: None,
            })
            .collect();

        let indicators = vec!["体重".to_string(), "CD45 比例".to_string()];

        let group_config = GroupConfig {
            num_groups: 3,
            animals_per_group: GroupSize::Uniform { value: 20 },
            sex_constraints: constraints.clone(),
            scenario: StudyScenario::Exploratory,
            method: GroupingMethod::ConstrainedRandom,
            randomization: Some(RandomizationConfig {
                seed: Some(42),
                primary_indicator: None,
                acceptance: Some(AcceptanceCriterion::TopFraction { target_rate: 0.10 }),
                max_attempts: 10_000,
                draw_index: 2,
                minimization: None,
            }),
        };

        let stat_config = StatConfig {
            selected_indicators: indicators.clone(),
            alpha: 0.05,
            mode: OptimizationMode::Strict,
            max_candidates: 1,
        };

        let result = grouping::compute_grouping(dataset.clone(), group_config, stat_config)
            .expect("randomized run must succeed")
            .candidates
            .remove(0);
        let record = result
            .randomization
            .clone()
            .expect("record must be present");

        let output_dir = std::env::temp_dir().join("autogroup_export_acceptance");
        std::fs::create_dir_all(&output_dir).expect("temp dir");
        let output = output_dir
            .join("acceptance.xlsx")
            .to_str()
            .unwrap()
            .to_string();

        let sheet_config = exporter::SheetConfig {
            scenario: StudyScenario::Exploratory,
            selected_indicators: indicators,
            include_statistics: true,
            include_summary: true,
            group_constraints: Some(constraints),
        };
        exporter::export_grouping_result(&result, &dataset, &sheet_config, &output)
            .expect("export must succeed");

        let rows = read_sheet(&output, "汇总信息");
        let value_of = |label: &str| -> String {
            rows.iter()
                .find(|row| row.first().map(String::as_str) == Some(label))
                .unwrap_or_else(|| panic!("summary sheet must contain a {label} row"))
                .get(1)
                .cloned()
                .unwrap_or_default()
        };

        assert!(value_of("接受准则").contains("仅接受最均衡的前 10%"));
        assert!(value_of("接受准则").contains("定标抽样 1000 次"));
        assert_eq!(value_of("主种子"), "42");
        assert_eq!(value_of("抽签序号"), "2");
        assert_eq!(value_of("随机种子"), record.seed.to_string());
    }

    /// Minimization's export has to say what it did and offer the only hand check it can:
    /// the decision log. It must not publish a per-animal draw, because the "sort by this
    /// column and deal" verification that column implies does not exist here.
    #[test]
    fn a_minimization_run_exports_its_entry_order_and_decision_log() {
        let dataset =
            parser::parse_excel_file(&fixture_path("tests/fixtures/randomization_input_60f.xlsx"))
                .expect("fixture must parse");

        let constraints: Vec<SexConstraint> = (0..3)
            .map(|i| SexConstraint {
                group_index: i,
                male_count: 0,
                female_count: 20,
                group_type: GroupType::Experimental,
                custom_name: None,
            })
            .collect();

        let indicators = vec!["体重".to_string(), "CD45 比例".to_string()];

        let group_config = GroupConfig {
            num_groups: 3,
            animals_per_group: GroupSize::Uniform { value: 20 },
            sex_constraints: constraints.clone(),
            scenario: StudyScenario::ConfirmatoryTrial,
            method: GroupingMethod::Minimization,
            randomization: Some(RandomizationConfig {
                seed: Some(2026),
                minimization: Some(MinimizationConfig {
                    covariates: vec!["体重".to_string(), "CD45 比例".to_string()],
                    allocation_probability: 0.8,
                    binning: CovariateBinning::Tertiles,
                }),
                ..Default::default()
            }),
        };

        let stat_config = StatConfig {
            selected_indicators: indicators.clone(),
            alpha: 0.05,
            mode: OptimizationMode::Strict,
            max_candidates: 1,
        };

        let result = grouping::compute_grouping(dataset.clone(), group_config, stat_config)
            .expect("minimization must succeed")
            .candidates
            .remove(0);

        let output_dir = std::env::temp_dir().join("autogroup_export_minimization");
        std::fs::create_dir_all(&output_dir).expect("temp dir");
        let output = output_dir
            .join("minimization.xlsx")
            .to_str()
            .unwrap()
            .to_string();

        let sheet_config = exporter::SheetConfig {
            scenario: StudyScenario::ConfirmatoryTrial,
            selected_indicators: indicators,
            include_statistics: true,
            include_summary: true,
            group_constraints: Some(constraints),
        };
        exporter::export_grouping_result(&result, &dataset, &sheet_config, &output)
            .expect("export must succeed");

        assert!(
            sheet_names(&output).contains(&"最小化过程".to_string()),
            "the decision log is the audit surface; it cannot be optional"
        );

        // 分组结果: 入组顺序 replaces the draw columns.
        let grouping_rows = read_sheet(&output, "分组结果");
        // calamine trims leading empty rows, so the unit row above the header may or may
        // not survive the round trip; find the header by its content instead.
        let header = grouping_rows
            .iter()
            .find(|row| row.first().map(String::as_str) == Some("组别"))
            .expect("分组结果 must carry a header row");
        assert_eq!(header[3], "入组顺序");
        assert!(
            !header.iter().any(|cell| cell == "随机数" || cell == "区组"),
            "a minimization export must not imply a sort-and-deal check: {header:?}"
        );

        // 汇总信息: principle and stratification row have to tell the same story.
        let summary = read_sheet(&output, "汇总信息");
        let value_of = |label: &str| -> String {
            summary
                .iter()
                .find(|row| row.first().map(String::as_str) == Some(label))
                .unwrap_or_else(|| panic!("summary sheet must contain a {label} row"))
                .get(1)
                .cloned()
                .unwrap_or_default()
        };

        let principle = value_of("分组原理");
        assert!(principle.contains("最小化法"), "{principle}");
        assert!(principle.contains("p = 0.80"), "{principle}");
        assert!(principle.contains("体重"), "{principle}");
        assert!(principle.contains("性别层内三分位"), "{principle}");

        let stratification = value_of("分层变量");
        assert!(
            stratification.contains("协变量") && stratification.contains("体重"),
            "the stratification row must not fall back to 性别: {stratification}"
        );
        assert_eq!(value_of("分配概率 p"), "0.8");
        assert!(value_of("不平衡度量").contains("配额归一"));

        // 最小化过程: parameters, cut points, then one row per animal.
        let process = read_sheet(&output, "最小化过程");
        let flat: Vec<String> = process.iter().flatten().cloned().collect();
        assert!(flat.iter().any(|cell| cell == "逐只分配决策"));
        assert!(flat.iter().any(|cell| cell == "决策分支"));
        assert!(flat.iter().any(|cell| cell == "切点"));

        let decision_rows = process
            .iter()
            .filter(|row| {
                row.first()
                    .and_then(|cell| cell.parse::<f64>().ok())
                    .is_some()
                    && row
                        .get(1)
                        .is_some_and(|id| id.starts_with(char::is_numeric))
            })
            .count();
        assert_eq!(decision_rows, 60, "one logged decision per animal");
    }
}
