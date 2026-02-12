pub mod levene;
pub mod ttest;
pub mod anova;
pub mod tukey;
pub mod dunnett;

use anyhow::Result;

/// Select and run appropriate statistical test
/// Returns: (levene_p_value, diff_p_value, test_method, optional_posthoc_results)
pub fn compute_p_value(
    groups: &[Vec<f64>],
    alpha: f64,
) -> Result<(f64, f64, String, Option<Vec<(usize, usize, f64)>>)> {
    let num_groups = groups.len();

    if num_groups < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups"));
    }

    // Test variance homogeneity
    let p_levene = levene::levene_test(groups)?;
    let is_homogeneous = p_levene > alpha;

    if num_groups == 2 {
        // Two-group comparison: t-test (no post-hoc needed)
        if is_homogeneous {
            let p = ttest::student_ttest(&groups[0], &groups[1])?;
            Ok((p_levene, p, "Student t-test".to_string(), None))
        } else {
            let p = ttest::welch_ttest(&groups[0], &groups[1])?;
            Ok((p_levene, p, "Welch t-test".to_string(), None))
        }
    } else {
        // Multi-group comparison: ANOVA + post-hoc (always execute post-hoc)
        if is_homogeneous {
            // Variance homogeneous: One-way ANOVA + Tukey HSD
            let p = anova::one_way_anova(groups)?;
            let posthoc = tukey::tukey_hsd(groups)?;
            Ok((p_levene, p, "One-way ANOVA + Tukey HSD".to_string(), Some(posthoc)))
        } else {
            // Variance not homogeneous: Welch ANOVA + Dunnett's T3
            let p = anova::welch_anova(groups)?;
            let posthoc = dunnett::dunnett_t3(groups)?;
            Ok((p_levene, p, "Welch ANOVA + Dunnett's T3".to_string(), Some(posthoc)))
        }
    }
}
