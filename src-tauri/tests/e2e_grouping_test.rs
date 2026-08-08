//! End-to-end regression test over the whole pipeline:
//!
//!   input .xlsx -> parse -> compute grouping -> export .xlsx -> compare
//!
//! Both fixtures in `tests/fixtures/` are real application artifacts: the input is a
//! real dataset and the expected output is an export that was produced and accepted by
//! the user. Only the animal IDs were anonymized (the study code was replaced with a
//! `DEMO0xx` prefix) and the document properties were scrubbed; every measurement,
//! header, sheet and cell is untouched.
//!
//! This locks down the parts a refactor can silently break: header parsing, indicator
//! keys, sex handling, candidate ranking, the chosen assignment, every reported P value
//! and the exported workbook layout. If this test fails, the grouping engine no longer
//! reproduces a result a human already signed off on — investigate before updating the
//! fixture.

use autogroup_lib::core::{exporter, grouping, models::*, parser};
use calamine::{open_workbook_auto, Data, Reader};
use std::path::{Path, PathBuf};

const INPUT_FIXTURE: &str = "tests/fixtures/e2e_input.xlsx";
const EXPECTED_FIXTURE: &str = "tests/fixtures/e2e_expected_output.xlsx";

/// Text columns carry IDs rather than measurements. The UI drops them from the default
/// selection (see `src/utils/indicator-filter.ts`); mirror that here.
const NON_NUMERIC_COLUMNS: [&str; 3] = ["样本号", "样品识别号", "FULLNAME"];

/// The grouping the user accepted: 3 groups of 2 males + 1 female.
const EXPECTED_GROUPS: [[&str; 3]; 3] = [
    ["DEMO001", "DEMO004", "DEMO008"],
    ["DEMO002", "DEMO005", "DEMO007"],
    ["DEMO003", "DEMO006", "DEMO009"],
];

/// Rows whose value legitimately changes between runs, so a cell-by-cell comparison would
/// only ever be measuring the clock or the release number. `引擎版本` is asserted against
/// `CARGO_PKG_VERSION` in `each_method_exports_its_own_scenario_and_principle` instead —
/// otherwise every version bump would look like a pipeline regression.
const VOLATILE_LABELS: [&str; 2] = ["计算耗时 (ms)", "引擎版本"];

fn fixture(relative: &str) -> PathBuf {
    Path::new(env!("CARGO_MANIFEST_DIR")).join(relative)
}

#[test]
fn end_to_end_matches_accepted_export() {
    // --- Stage 1: parse -------------------------------------------------------------
    let dataset = parser::parse_excel_file(fixture(INPUT_FIXTURE).to_str().unwrap())
        .expect("input fixture must parse");

    assert_eq!(dataset.metadata.total_animals, 9);
    assert_eq!(dataset.metadata.male_count, 6);
    assert_eq!(dataset.metadata.female_count, 3);
    assert_eq!(
        dataset.indicator_names.len(),
        73,
        "parsed indicator keys changed — the dual-row header rules moved"
    );

    let selected_indicators: Vec<String> = dataset
        .indicator_names
        .iter()
        .filter(|name| !NON_NUMERIC_COLUMNS.contains(&name.as_str()))
        .cloned()
        .collect();
    assert_eq!(selected_indicators.len(), 70);

    // --- Stage 2: compute -----------------------------------------------------------
    let sex_constraints: Vec<SexConstraint> = (0..3)
        .map(|i| SexConstraint {
            group_index: i,
            male_count: 2,
            female_count: 1,
            group_type: GroupType::Experimental,
            custom_name: None,
        })
        .collect();

    let group_config = GroupConfig {
        scenario: StudyScenario::Exploratory,
        method: GroupingMethod::Optimized,
        randomization: None,
        num_groups: 3,
        animals_per_group: GroupSize::Uniform { value: 3 },
        sex_constraints: sex_constraints.clone(),
    };

    let stat_config = StatConfig {
        selected_indicators: selected_indicators.clone(),
        alpha: 0.05,
        mode: OptimizationMode::Strict,
        max_candidates: 10,
    };

    let multi = grouping::compute_optimal_grouping(dataset.clone(), group_config, stat_config)
        .expect("grouping must succeed");

    let result = multi
        .candidates
        .first()
        .expect("at least one valid candidate");

    assert_eq!(result.summary.num_groups, 3);
    assert_eq!(result.summary.total_indicators, 70);
    assert_eq!(result.summary.num_invalid_indicators, 0);
    assert!(result.summary.meets_criteria);

    // The winning assignment must be the one the user accepted. Ties are broken by
    // enumeration order, so this is deterministic.
    for (group_id, expected_members) in EXPECTED_GROUPS.iter().enumerate() {
        let mut actual: Vec<&str> = result
            .assignments
            .iter()
            .filter(|a| a.group_id == group_id)
            .map(|a| a.animal_id.as_str())
            .collect();
        actual.sort_unstable();

        assert_eq!(
            actual,
            expected_members.to_vec(),
            "group {} membership changed",
            group_id + 1
        );
    }

    // Every indicator carries the full set of pairwise post-hoc comparisons, and all of them
    // must clear alpha for the grouping to be valid. A saturated column (every P exactly
    // 1.0) means the post-hoc distribution regressed to an approximation and is not testing
    // anything — that is what the last assertion guards.
    for stat in &result.statistics {
        let comparisons = stat.posthoc_results.as_ref().unwrap_or_else(|| {
            panic!(
                "indicator {} lost its post-hoc results",
                stat.indicator_name
            )
        });

        assert_eq!(
            comparisons.len(),
            3,
            "indicator {}: expected C(3,2) = 3 pairwise comparisons",
            stat.indicator_name
        );
        assert!(
            comparisons.iter().all(|c| c.is_valid),
            "indicator {} has a failing pairwise comparison: {:?}",
            stat.indicator_name,
            comparisons
        );
    }

    let saturated = result
        .statistics
        .iter()
        .flat_map(|s| s.posthoc_results.iter().flatten())
        .filter(|c| c.p_value >= 1.0)
        .count();
    assert!(
        saturated < 20,
        "{saturated} post-hoc p-values are pinned at 1.0; the studentized range distribution \
         has likely been replaced by an approximation again"
    );

    // --- Stage 3: export ------------------------------------------------------------
    let output_dir = std::env::temp_dir().join("autogroup_e2e");
    std::fs::create_dir_all(&output_dir).expect("temp dir");
    let output_path = output_dir.join("e2e_actual_output.xlsx");

    let sheet_config = exporter::SheetConfig {
        scenario: StudyScenario::Exploratory,
        selected_indicators,
        include_statistics: true,
        include_summary: true,
        group_constraints: Some(sex_constraints),
    };

    exporter::export_grouping_result(
        result,
        &dataset,
        &sheet_config,
        output_path.to_str().unwrap(),
    )
    .expect("export must succeed");

    // --- Stage 4: compare against the accepted export -------------------------------
    let actual_sheets = read_workbook(&output_path);
    let expected_sheets = read_workbook(&fixture(EXPECTED_FIXTURE));

    assert_eq!(
        actual_sheets.iter().map(|(n, _)| n).collect::<Vec<_>>(),
        expected_sheets.iter().map(|(n, _)| n).collect::<Vec<_>>(),
        "sheet names or order changed"
    );

    for ((sheet_name, actual), (_, expected)) in actual_sheets.iter().zip(&expected_sheets) {
        compare_sheet(sheet_name, actual, expected);
    }

    let _ = std::fs::remove_file(&output_path);
}

type Grid = Vec<Vec<Cell>>;

/// A cell reduced to what the comparison cares about: text or a number.
#[derive(Debug, Clone, PartialEq)]
enum Cell {
    Empty,
    Text(String),
    Number(f64),
}

impl Cell {
    fn describe(&self) -> String {
        match self {
            Cell::Empty => "<empty>".to_string(),
            Cell::Text(s) => format!("{s:?}"),
            Cell::Number(n) => n.to_string(),
        }
    }
}

fn read_workbook(path: &Path) -> Vec<(String, Grid)> {
    let mut workbook = open_workbook_auto(path).expect("workbook must open");
    let names = workbook.sheet_names().to_vec();

    names
        .into_iter()
        .map(|name| {
            let range = workbook
                .worksheet_range(&name)
                .unwrap_or_else(|e| panic!("sheet {name} must be readable: {e}"));

            let grid: Grid = range
                .rows()
                .map(|row| {
                    row.iter()
                        .map(|cell| match cell {
                            Data::Empty => Cell::Empty,
                            Data::String(s) => Cell::Text(s.trim().to_string()),
                            Data::Float(f) => Cell::Number(*f),
                            Data::Int(i) => Cell::Number(*i as f64),
                            Data::Bool(b) => Cell::Text(b.to_string()),
                            other => Cell::Text(other.to_string()),
                        })
                        .collect()
                })
                .collect();

            (name, grid)
        })
        .collect()
}

fn compare_sheet(sheet_name: &str, actual: &Grid, expected: &Grid) {
    assert_eq!(
        actual.len(),
        expected.len(),
        "sheet {sheet_name}: row count changed"
    );

    for (row_idx, (actual_row, expected_row)) in actual.iter().zip(expected).enumerate() {
        // Skip rows whose value is a timing measurement.
        if let Some(Cell::Text(label)) = expected_row.first() {
            if VOLATILE_LABELS.contains(&label.as_str()) {
                continue;
            }
        }

        assert_eq!(
            actual_row.len(),
            expected_row.len(),
            "sheet {sheet_name} row {}: column count changed",
            row_idx + 1
        );

        for (col_idx, (actual_cell, expected_cell)) in
            actual_row.iter().zip(expected_row).enumerate()
        {
            let matches = match (actual_cell, expected_cell) {
                // Statistical values are compared with a tolerance: the arithmetic is
                // unchanged, but libm differences across platforms move the last bits.
                (Cell::Number(a), Cell::Number(b)) => approx_eq(*a, *b),
                (a, b) => a == b,
            };

            assert!(
                matches,
                "sheet {sheet_name}, row {}, column {}: expected {}, got {}",
                row_idx + 1,
                col_idx + 1,
                expected_cell.describe(),
                actual_cell.describe()
            );
        }
    }
}

fn approx_eq(a: f64, b: f64) -> bool {
    if a == b {
        return true;
    }
    let diff = (a - b).abs();
    diff <= 1e-9 * a.abs().max(b.abs()).max(1.0)
}

/// Every usable method has to survive the whole pipeline and label itself honestly in
/// the export. What a reviewer reads out of the summary sheet is the only place the
/// method description exists, so it is asserted here rather than at the model layer.
#[test]
fn each_method_exports_its_own_scenario_and_principle() {
    let dataset = parser::parse_excel_file(fixture(INPUT_FIXTURE).to_str().unwrap())
        .expect("input fixture must parse");

    let selected_indicators: Vec<String> = dataset
        .indicator_names
        .iter()
        .filter(|name| !NON_NUMERIC_COLUMNS.contains(&name.as_str()))
        .cloned()
        .collect();

    let sex_constraints: Vec<SexConstraint> = (0..3)
        .map(|i| SexConstraint {
            group_index: i,
            male_count: 2,
            female_count: 1,
            group_type: GroupType::Experimental,
            custom_name: None,
        })
        .collect();

    // The fixture holds both sexes, so every mode stratifies by sex and the labels say so.
    let cases: [(StudyScenario, GroupingMethod, &str, &str); 4] = [
        (
            StudyScenario::Exploratory,
            GroupingMethod::Optimized,
            "探索性 / 非 GLP 实验",
            "按性别分层 + 统计均衡优化（择优搜索式分配，非随机）",
        ),
        (
            StudyScenario::GlpSubmission,
            GroupingMethod::Random,
            "GLP 申报实验",
            "分层随机（分层变量：性别）",
        ),
        (
            StudyScenario::GlpSubmission,
            GroupingMethod::ConstrainedRandom,
            "GLP 申报实验",
            "分层随机（分层变量：性别）+ 基线均衡接受准则",
        ),
        (
            StudyScenario::GlpSubmission,
            GroupingMethod::BlockedRandom,
            "GLP 申报实验",
            "分层随机（分层变量：kg）+ 基线均衡接受准则",
        ),
    ];

    for (scenario, method, expected_scenario, expected_principle) in cases {
        let group_config = GroupConfig {
            num_groups: 3,
            animals_per_group: GroupSize::Uniform { value: 3 },
            sex_constraints: sex_constraints.clone(),
            scenario,
            method,
            randomization: (method != GroupingMethod::Optimized).then(|| RandomizationConfig {
                seed: Some(2026),
                primary_indicator: (method == GroupingMethod::BlockedRandom)
                    .then(|| "kg".to_string()),
                acceptance: (method == GroupingMethod::ConstrainedRandom
                    || method == GroupingMethod::BlockedRandom)
                    .then_some(AcceptanceCriterion::AlphaLine),
                max_attempts: 10_000,
                draw_index: 1,
                minimization: None,
            }),
        };

        let stat_config = StatConfig {
            selected_indicators: selected_indicators.clone(),
            alpha: 0.05,
            // 70 indicators under Strict accept roughly 1 draw in 80; the acceptance
            // criterion is exercised properly without the test taking a rejection budget
            // it cannot afford.
            mode: OptimizationMode::Optimized,
            max_candidates: 10,
        };

        let multi = grouping::compute_grouping(dataset.clone(), group_config, stat_config)
            .unwrap_or_else(|e| panic!("{method:?} must produce a grouping: {e}"));
        let result = multi.candidates.first().expect("one candidate");

        assert_eq!(result.method, method);
        assert_eq!(
            result.randomization.is_some(),
            method != GroupingMethod::Optimized,
            "{method:?}: only randomized runs carry a randomization record"
        );

        let output_dir = std::env::temp_dir().join("autogroup_e2e");
        std::fs::create_dir_all(&output_dir).expect("temp dir");
        let output_path = output_dir.join(format!("method_{method:?}.xlsx"));

        let sheet_config = exporter::SheetConfig {
            selected_indicators: selected_indicators.clone(),
            include_statistics: true,
            include_summary: true,
            group_constraints: Some(sex_constraints.clone()),
            scenario,
        };
        exporter::export_grouping_result(
            result,
            &dataset,
            &sheet_config,
            output_path.to_str().unwrap(),
        )
        .expect("export must succeed");

        let sheets = read_workbook(&output_path);
        let summary = &sheets
            .iter()
            .find(|(name, _)| name == "汇总信息")
            .expect("summary sheet")
            .1;

        let value_of = |label: &str| -> String {
            summary
                .iter()
                .find(|row| row.first() == Some(&Cell::Text(label.to_string())))
                .and_then(|row| row.get(1))
                .map(Cell::describe)
                .unwrap_or_else(|| panic!("{method:?}: summary row {label} is missing"))
        };

        assert_eq!(value_of("应用场景"), format!("{expected_scenario:?}"));
        assert_eq!(value_of("分组原理"), format!("{expected_principle:?}"));
        assert_eq!(value_of("分组方式"), format!("{:?}", method.to_chinese()));

        // The fingerprint identifies the input and must not depend on the method.
        assert_eq!(value_of("输入指纹"), format!("{:?}", "c3cdeb15caace00d"));
        // Recorded so a result can be re-checked against the engine that produced it.
        assert_eq!(
            value_of("引擎版本"),
            format!("{:?}", env!("CARGO_PKG_VERSION"))
        );

        if let Some(record) = &result.randomization {
            assert_eq!(value_of("随机种子"), format!("{:?}", "2026"));
            assert_eq!(value_of("随机数算法"), format!("{:?}", "chacha12"));
            assert_eq!(record.seed, 2026);
        } else {
            assert_eq!(value_of("随机种子"), format!("{:?}", "不适用"));
        }

        let _ = std::fs::remove_file(&output_path);
    }
}

/// The audit columns exist so a reviewer can redo the allocation in Excel: sort by 区组
/// then 随机数, deal each group its quota in turn, and 组别 must come back out. This test
/// reads the exported sheet and performs exactly that check.
#[test]
fn the_exported_sheet_can_be_re_sorted_into_the_same_grouping() {
    let dataset = parser::parse_excel_file(fixture(INPUT_FIXTURE).to_str().unwrap())
        .expect("input fixture must parse");

    let selected_indicators: Vec<String> = dataset
        .indicator_names
        .iter()
        .filter(|name| !NON_NUMERIC_COLUMNS.contains(&name.as_str()))
        .cloned()
        .collect();

    let sex_constraints: Vec<SexConstraint> = (0..3)
        .map(|i| SexConstraint {
            group_index: i,
            male_count: 2,
            female_count: 1,
            group_type: GroupType::Experimental,
            custom_name: None,
        })
        .collect();

    let group_config = GroupConfig {
        num_groups: 3,
        animals_per_group: GroupSize::Uniform { value: 3 },
        sex_constraints: sex_constraints.clone(),
        scenario: StudyScenario::GlpSubmission,
        method: GroupingMethod::BlockedRandom,
        randomization: Some(RandomizationConfig {
            seed: Some(2026),
            primary_indicator: Some("kg".to_string()),
            acceptance: None,
            max_attempts: 1,
            draw_index: 1,
            minimization: None,
        }),
    };

    let stat_config = StatConfig {
        selected_indicators: selected_indicators.clone(),
        alpha: 0.05,
        mode: OptimizationMode::Optimized,
        max_candidates: 1,
    };

    let multi = grouping::compute_grouping(dataset.clone(), group_config, stat_config)
        .expect("blocked randomization must succeed");
    let result = multi.candidates.first().expect("one candidate");

    let output_dir = std::env::temp_dir().join("autogroup_e2e");
    std::fs::create_dir_all(&output_dir).expect("temp dir");
    let output_path = output_dir.join("audit_columns.xlsx");

    let sheet_config = exporter::SheetConfig {
        selected_indicators,
        include_statistics: false,
        include_summary: false,
        group_constraints: Some(sex_constraints),
        scenario: StudyScenario::GlpSubmission,
    };
    exporter::export_grouping_result(
        result,
        &dataset,
        &sheet_config,
        output_path.to_str().unwrap(),
    )
    .expect("export must succeed");

    let sheets = read_workbook(&output_path);
    let grid = &sheets
        .iter()
        .find(|(name, _)| name == "分组结果")
        .expect("grouping sheet")
        .1;

    // Header row: 组别 | 动物编号 | 性别 | 区组 | 随机数 | indicators...
    assert_eq!(grid[1][0], Cell::Text("组别".to_string()));
    assert_eq!(grid[1][3], Cell::Text("区组".to_string()));
    assert_eq!(grid[1][4], Cell::Text("随机数".to_string()));
    assert_eq!(
        grid[1][5],
        Cell::Text("体重".to_string()),
        "indicators must start right after the audit columns"
    );

    // Read the animal rows straight out of the sheet, the way a reviewer would.
    let mut rows: Vec<(String, String, usize, f64)> = Vec::new();
    for row in grid.iter().skip(2) {
        let (Some(Cell::Text(group)), Some(Cell::Text(animal))) = (row.first(), row.get(1)) else {
            continue;
        };
        // Skip the per-group mean±SD rows, which carry no draw.
        let (Some(Cell::Number(block)), Some(Cell::Number(draw))) = (row.get(3), row.get(4)) else {
            continue;
        };
        rows.push((group.clone(), animal.clone(), *block as usize, *draw));
    }
    assert_eq!(rows.len(), 9, "every animal must appear with its draw");

    // Sort by 区组 then 随机数, then deal 1 animal per group per block (3 groups of 3
    // means gcd 3, i.e. 3 blocks of 3 within each sex stratum). Sex is a separate
    // stratum, so blocks are replayed per sex.
    let sex_of: std::collections::HashMap<&str, Sex> = dataset
        .animals
        .iter()
        .map(|a| (a.id.as_str(), a.sex))
        .collect();

    for sex in [Sex::Male, Sex::Female] {
        let mut stratum: Vec<&(String, String, usize, f64)> = rows
            .iter()
            .filter(|(_, animal, _, _)| sex_of[animal.as_str()] == sex)
            .collect();
        stratum.sort_by(|a, b| a.2.cmp(&b.2).then(a.3.partial_cmp(&b.3).unwrap()));

        // Males: quotas 2/2/2 -> gcd 2 -> 2 blocks of 3. Females: quotas 1/1/1 -> 1 block
        // of 3. Either way each group takes exactly one animal per block, in order.
        for (position, (group, animal, _, _)) in stratum.iter().enumerate() {
            let expected = format!("组{}", position % 3 + 1);
            assert_eq!(
                *group, expected,
                "{animal} ({sex:?}) landed in {group} but the recorded draw puts it in {expected}"
            );
        }
    }

    let _ = std::fs::remove_file(&output_path);
}
