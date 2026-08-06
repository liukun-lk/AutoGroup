pub mod anova;
pub mod dunnett;
pub mod levene;
pub mod ttest;
pub mod tukey;

use anyhow::Result;

/// Which test cascade was applied to an indicator.
///
/// Kept as a `Copy` enum rather than a `String` because the grouping engine calls
/// [`compute_p_value`] once per indicator per candidate — up to millions of times per run.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum TestMethod {
    StudentTTest,
    WelchTTest,
    AnovaTukey,
    WelchAnovaDunnett,
}

impl TestMethod {
    pub fn as_str(self) -> &'static str {
        match self {
            TestMethod::StudentTTest => "Student t-test",
            TestMethod::WelchTTest => "Welch t-test",
            TestMethod::AnovaTukey => "One-way ANOVA + Tukey HSD",
            TestMethod::WelchAnovaDunnett => "Welch ANOVA + Dunnett's T3",
        }
    }
}

impl std::fmt::Display for TestMethod {
    fn fmt(&self, f: &mut std::fmt::Formatter<'_>) -> std::fmt::Result {
        f.write_str(self.as_str())
    }
}

/// Outcome of the test cascade for a single indicator.
pub struct IndicatorTest {
    pub levene_p_value: f64,
    pub diff_p_value: f64,
    pub method: TestMethod,
    /// True when every pairwise post-hoc comparison passed (`P > alpha`).
    /// Always true for two-group comparisons, which have no post-hoc stage.
    pub posthoc_all_valid: bool,
}

/// Select and run the appropriate statistical test for one indicator.
///
/// `posthoc_out` is cleared and then filled with `(group1_idx, group2_idx, p_value)` for
/// every pairwise comparison. It stays empty for two-group comparisons. Callers that run
/// this in a hot loop should reuse a single buffer instead of allocating per call.
pub fn compute_indicator_test(
    groups: &[Vec<f64>],
    alpha: f64,
    posthoc_out: &mut Vec<(usize, usize, f64)>,
) -> Result<IndicatorTest> {
    let num_groups = groups.len();

    if num_groups < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups"));
    }

    posthoc_out.clear();

    // Test variance homogeneity
    let p_levene = levene::levene_test(groups)?;
    let is_homogeneous = p_levene > alpha;

    let (diff_p_value, method) = if num_groups == 2 {
        // Two-group comparison: t-test (no post-hoc needed)
        if is_homogeneous {
            (
                ttest::student_ttest(&groups[0], &groups[1])?,
                TestMethod::StudentTTest,
            )
        } else {
            (
                ttest::welch_ttest(&groups[0], &groups[1])?,
                TestMethod::WelchTTest,
            )
        }
    } else if is_homogeneous {
        // Variance homogeneous: One-way ANOVA + Tukey HSD
        let p = anova::one_way_anova(groups)?;
        tukey::tukey_hsd_into(groups, posthoc_out)?;
        (p, TestMethod::AnovaTukey)
    } else {
        // Variance not homogeneous: Welch ANOVA + Dunnett's T3
        let p = anova::welch_anova(groups)?;
        dunnett::dunnett_t3_into(groups, posthoc_out)?;
        (p, TestMethod::WelchAnovaDunnett)
    };

    let posthoc_all_valid = posthoc_out.iter().all(|&(_, _, p)| p > alpha);

    Ok(IndicatorTest {
        levene_p_value: p_levene,
        diff_p_value,
        method,
        posthoc_all_valid,
    })
}

/// Convenience wrapper around [`compute_indicator_test`] that allocates its own post-hoc buffer.
///
/// Returns: `(levene_p_value, diff_p_value, test_method, optional_posthoc_results)`.
/// Prefer [`compute_indicator_test`] on hot paths.
pub fn compute_p_value(
    groups: &[Vec<f64>],
    alpha: f64,
) -> Result<(f64, f64, String, Option<Vec<(usize, usize, f64)>>)> {
    let mut posthoc = Vec::new();
    let test = compute_indicator_test(groups, alpha, &mut posthoc)?;

    let posthoc = if groups.len() > 2 {
        Some(posthoc)
    } else {
        None
    };

    Ok((
        test.levene_p_value,
        test.diff_p_value,
        test.method.as_str().to_string(),
        posthoc,
    ))
}
