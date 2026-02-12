pub mod levene;
pub mod ttest;
pub mod anova;
pub mod tukey;
pub mod dunnett;

use anyhow::Result;

/// Select and run appropriate statistical test
pub fn compute_p_value(
    groups: &[Vec<f64>],
    alpha: f64,
) -> Result<(f64, String)> {
    let num_groups = groups.len();

    if num_groups < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups"));
    }

    // Test variance homogeneity
    let p_levene = levene::levene_test(groups)?;
    let is_homogeneous = p_levene > alpha;

    if num_groups == 2 {
        // Two-group comparison: t-test
        if is_homogeneous {
            let p = ttest::student_ttest(&groups[0], &groups[1])?;
            Ok((p, "Student t-test".to_string()))
        } else {
            let p = ttest::welch_ttest(&groups[0], &groups[1])?;
            Ok((p, "Welch t-test".to_string()))
        }
    } else {
        // Multi-group comparison: ANOVA
        if is_homogeneous {
            let p = anova::one_way_anova(groups)?;
            Ok((p, "One-way ANOVA + Tukey HSD".to_string()))
        } else {
            let p = anova::welch_anova(groups)?;
            Ok((p, "Welch ANOVA + Dunnett's T3".to_string()))
        }
    }
}
