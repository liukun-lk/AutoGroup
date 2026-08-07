pub mod anova;
pub mod distributions;
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
#[derive(Debug)]
pub struct IndicatorTest {
    pub levene_p_value: f64,
    pub diff_p_value: f64,
    pub method: TestMethod,
    /// True when every pairwise post-hoc comparison passed (`P > alpha`).
    /// Always true for two-group comparisons, which have no post-hoc stage.
    pub posthoc_all_valid: bool,
}

/// How much post-hoc detail [`compute_indicator_test`] should produce.
///
/// Exact post-hoc p-values come from the studentized range / maximum modulus distributions,
/// each of which costs a numerical quadrature. Ranking never reads those p-values — it only
/// needs to know whether they all clear `alpha` (see the algorithm contract in
/// `.claude/skills/animal-grouping/`) — so the scoring pass asks for the verdict alone and
/// only the reported Top-N candidates pay for exact values.
#[derive(Debug, Clone, Copy, PartialEq, Eq)]
pub enum PostHocDetail {
    /// Decide whether every pairwise comparison clears `alpha`; leave `posthoc_out` empty.
    ValidityOnly,
    /// Also fill `posthoc_out` with the exact p-value of every pairwise comparison.
    Exact,
}

/// Select and run the appropriate statistical test for one indicator.
///
/// `posthoc_out` is cleared, and under [`PostHocDetail::Exact`] filled with
/// `(group1_idx, group2_idx, p_value)` for every pairwise comparison. It stays empty for
/// two-group comparisons, which have no post-hoc stage, and under
/// [`PostHocDetail::ValidityOnly`]. Callers that run this in a hot loop should reuse a single
/// buffer instead of allocating per call.
pub fn compute_indicator_test(
    groups: &[Vec<f64>],
    alpha: f64,
    detail: PostHocDetail,
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

    let (diff_p_value, method, posthoc_all_valid) = if num_groups == 2 {
        // Two-group comparison: t-test (no post-hoc needed)
        if is_homogeneous {
            (
                ttest::student_ttest(&groups[0], &groups[1])?,
                TestMethod::StudentTTest,
                true,
            )
        } else {
            (
                ttest::welch_ttest(&groups[0], &groups[1])?,
                TestMethod::WelchTTest,
                true,
            )
        }
    } else if is_homogeneous {
        // Variance homogeneous: One-way ANOVA + Tukey HSD
        let p = anova::one_way_anova(groups)?;
        let all_valid = match detail {
            PostHocDetail::ValidityOnly => tukey::tukey_all_valid(groups, alpha)?,
            PostHocDetail::Exact => {
                tukey::tukey_hsd_into(groups, posthoc_out)?;
                posthoc_out.iter().all(|&(_, _, p)| p > alpha)
            }
        };
        (p, TestMethod::AnovaTukey, all_valid)
    } else {
        // Variance not homogeneous: Welch ANOVA + Dunnett's T3
        let p = anova::welch_anova(groups)?;
        let all_valid = match detail {
            PostHocDetail::ValidityOnly => dunnett::dunnett_t3_all_valid(groups, alpha)?,
            PostHocDetail::Exact => {
                dunnett::dunnett_t3_into(groups, posthoc_out)?;
                posthoc_out.iter().all(|&(_, _, p)| p > alpha)
            }
        };
        (p, TestMethod::WelchAnovaDunnett, all_valid)
    };

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
    let test = compute_indicator_test(groups, alpha, PostHocDetail::Exact, &mut posthoc)?;

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

#[cfg(test)]
mod tests {
    use super::*;

    /// The whole cascade over a column with no variation at all. Levene runs a one-way
    /// ANOVA on the absolute deviations, which are all zero here, so this used to panic
    /// before the main test was ever reached.
    #[test]
    fn constant_indicator_runs_through_the_cascade() {
        let groups = vec![vec![1.0; 4], vec![1.0; 4], vec![1.0; 4]];
        let mut posthoc = Vec::new();
        let test =
            compute_indicator_test(&groups, 0.05, PostHocDetail::Exact, &mut posthoc).unwrap();

        assert_eq!(test.levene_p_value, 1.0);
        assert_eq!(test.diff_p_value, 1.0);
        assert!(
            test.posthoc_all_valid,
            "identical groups cannot be unbalanced"
        );
        assert_eq!(
            posthoc.iter().map(|c| c.2).collect::<Vec<_>>(),
            vec![1.0; 3]
        );
    }

    /// Same shape, but the constants differ between groups: the split is as unbalanced as
    /// it gets, and the cascade has to say so rather than fall over.
    #[test]
    fn separated_constant_groups_are_reported_as_different() {
        let groups = vec![vec![1.0; 4], vec![2.0; 4], vec![3.0; 4]];
        let mut posthoc = Vec::new();
        let test =
            compute_indicator_test(&groups, 0.05, PostHocDetail::Exact, &mut posthoc).unwrap();

        assert_eq!(test.diff_p_value, 0.0);
        assert!(!test.posthoc_all_valid);
    }

    /// The ranking pass takes the ValidityOnly shortcut and the reported Top-N takes the
    /// exact path; a candidate must not change verdict between them.
    ///
    /// The last shape has two constant groups and one that varies, so Levene declares the
    /// variances unequal and the cascade reaches Welch's ANOVA, where a zero-variance
    /// group leaves the weights undefined. Both routes then fail identically — the
    /// reference implementation returns NaN there for the same reason — and the callers
    /// treat that as "this indicator cannot be tested on this split".
    #[test]
    fn validity_shortcut_agrees_with_exact_on_degenerate_data() {
        for groups in [
            vec![vec![1.0; 4], vec![1.0; 4], vec![1.0; 4]],
            vec![vec![1.0; 4], vec![2.0; 4], vec![3.0; 4]],
            vec![vec![1.0; 4], vec![1.0; 4], vec![3.0, 9.0, 4.0, 1.0]],
        ] {
            let mut posthoc = Vec::new();
            let exact = compute_indicator_test(&groups, 0.05, PostHocDetail::Exact, &mut posthoc);
            let shortcut =
                compute_indicator_test(&groups, 0.05, PostHocDetail::ValidityOnly, &mut Vec::new());

            match (exact, shortcut) {
                (Ok(exact), Ok(shortcut)) => {
                    assert_eq!(
                        exact.posthoc_all_valid, shortcut.posthoc_all_valid,
                        "verdicts diverged on {groups:?}"
                    );
                    assert_eq!(exact.diff_p_value, shortcut.diff_p_value);
                }
                (Err(_), Err(_)) => {}
                (exact, shortcut) => panic!(
                    "one route succeeded and the other did not on {groups:?}: \
                     exact={exact:?}, shortcut={shortcut:?}"
                ),
            }
        }
    }
}
