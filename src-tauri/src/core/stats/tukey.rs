use anyhow::Result;

/// Tukey's Honest Significant Difference (HSD) test
/// Post-hoc test for ANOVA when variances are equal
/// Returns pairwise comparisons: Vec of (group1_idx, group2_idx, p_value)
pub fn tukey_hsd(groups: &[Vec<f64>]) -> Result<Vec<(usize, usize, f64)>> {
    let mut results = Vec::new();
    tukey_hsd_into(groups, &mut results)?;
    Ok(results)
}

/// Same as [`tukey_hsd`] but appends into a caller-owned buffer, so hot loops can
/// reuse one allocation across indicators and candidates.
pub fn tukey_hsd_into(groups: &[Vec<f64>], results: &mut Vec<(usize, usize, f64)>) -> Result<()> {
    if groups.len() < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Tukey HSD"));
    }

    let k = groups.len();

    // Calculate group means and sizes
    let group_stats: Vec<(f64, usize)> = groups
        .iter()
        .map(|g| {
            let n = g.len();
            let mean = g.iter().sum::<f64>() / n as f64;
            (mean, n)
        })
        .collect();

    // Calculate pooled within-group variance (MSE)
    let mut ss_within = 0.0;
    let mut df_within = 0;

    for (i, group) in groups.iter().enumerate() {
        let mean = group_stats[i].0;
        for &x in group {
            ss_within += (x - mean).powi(2);
        }
        df_within += group.len() - 1;
    }

    let mse = ss_within / df_within as f64;

    // Pairwise comparisons
    for i in 0..k {
        for j in (i + 1)..k {
            let (mean_i, n_i) = group_stats[i];
            let (mean_j, n_j) = group_stats[j];

            // Tukey's Q statistic
            let se = (mse * (1.0 / n_i as f64 + 1.0 / n_j as f64) / 2.0).sqrt();
            let q_stat = (mean_i - mean_j).abs() / se;

            // P-value from Studentized Range distribution
            // Note: statrs may not have StudRangeDistribution, use conservative approximation
            let p_value = tukey_q_to_p(q_stat, k, df_within);

            results.push((i, j, p_value));
        }
    }

    Ok(())
}

/// Convert Tukey's Q statistic to P-value
/// Uses conservative approximation based on Studentized Range distribution
fn tukey_q_to_p(q_stat: f64, num_groups: usize, df: usize) -> f64 {
    // Conservative approximation: treat Q as approximately chi-square distributed
    // For more accurate implementation, would need qtukey distribution tables

    // Simple approximation: convert to approximate t-statistic
    let t_approx = q_stat / std::f64::consts::SQRT_2;

    // Use two-tailed t-distribution as approximation
    // This is conservative (tends to give higher P-values)
    use statrs::distribution::{ContinuousCDF, StudentsT};

    let t_dist = StudentsT::new(0.0, 1.0, df as f64).unwrap();
    let p_one_tail = 1.0 - t_dist.cdf(t_approx);
    let p_value = 2.0 * p_one_tail * num_groups as f64; // Bonferroni-like adjustment

    // Clamp to [0, 1]
    p_value.min(1.0).max(0.0)
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_tukey_hsd_similar_groups() {
        // Groups with similar means should have high P-values
        let groups = vec![
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            vec![1.5, 2.5, 3.5, 4.5, 5.5],
            vec![1.2, 2.2, 3.2, 4.2, 5.2],
        ];

        let results = tukey_hsd(&groups).unwrap();

        // Should have C(3,2) = 3 pairwise comparisons
        assert_eq!(results.len(), 3);

        // All P-values should be relatively high
        for (i, j, p) in &results {
            println!("Group {i} vs {j}: P = {p}");
            assert!(p > &0.05, "Expected high P-value for similar groups");
        }
    }

    #[test]
    fn test_tukey_hsd_different_groups() {
        // One group very different from others
        let groups = vec![
            vec![1.0, 1.1, 1.2, 1.3, 1.4],
            vec![1.5, 1.6, 1.7, 1.8, 1.9],
            vec![10.0, 10.1, 10.2, 10.3, 10.4],
        ];

        let results = tukey_hsd(&groups).unwrap();

        // Comparisons involving group 2 should have low P-values
        for (i, j, p) in &results {
            if *i == 2 || *j == 2 {
                println!("Group {i} vs {j}: P = {p}");
                // Should detect difference
                assert!(p < &0.05, "Should detect difference with outlier group");
            }
        }
    }
}
