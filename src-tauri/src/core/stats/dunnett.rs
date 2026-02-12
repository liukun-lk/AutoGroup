use anyhow::Result;
use statrs::distribution::{StudentsT, ContinuousCDF};

/// Dunnett's T3 test
/// Post-hoc test for Welch ANOVA when variances are unequal
/// Returns pairwise comparisons: Vec of (group1_idx, group2_idx, p_value)
pub fn dunnett_t3(groups: &[Vec<f64>]) -> Result<Vec<(usize, usize, f64)>> {
    if groups.len() < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Dunnett's T3"));
    }

    let k = groups.len();

    // Calculate group statistics
    let group_stats: Vec<(f64, f64, usize)> = groups
        .iter()
        .map(|g| {
            let n = g.len() as f64;
            let mean = g.iter().sum::<f64>() / n;
            let variance = g
                .iter()
                .map(|x| (x - mean).powi(2))
                .sum::<f64>() / (n - 1.0);
            (mean, variance, g.len())
        })
        .collect();

    // Pairwise comparisons
    let mut results = Vec::new();

    for i in 0..k {
        for j in (i + 1)..k {
            let (mean_i, var_i, n_i) = group_stats[i];
            let (mean_j, var_j, n_j) = group_stats[j];

            // Welch's t-statistic
            let se = (var_i / n_i as f64 + var_j / n_j as f64).sqrt();
            let t_stat = ((mean_i - mean_j) / se).abs();

            // Welch-Satterthwaite degrees of freedom
            let df = welch_df(var_i, n_i, var_j, n_j);

            // P-value from t-distribution (two-tailed)
            let t_dist = StudentsT::new(0.0, 1.0, df)?;
            let p_one_tail = 1.0 - t_dist.cdf(t_stat);
            let p_value = 2.0 * p_one_tail;

            results.push((i, j, p_value));
        }
    }

    Ok(results)
}

/// Calculate Welch-Satterthwaite degrees of freedom
fn welch_df(var1: f64, n1: usize, var2: f64, n2: usize) -> f64 {
    let n1 = n1 as f64;
    let n2 = n2 as f64;

    let numerator = (var1 / n1 + var2 / n2).powi(2);
    let denominator = (var1 / n1).powi(2) / (n1 - 1.0) + (var2 / n2).powi(2) / (n2 - 1.0);

    numerator / denominator
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_dunnett_t3_similar_groups() {
        // Groups with similar means but different variances
        let groups = vec![
            vec![1.0, 1.1, 1.2], // Low variance
            vec![2.0, 3.0, 4.0], // Higher variance
            vec![1.5, 1.6, 1.7], // Low variance
        ];

        let results = dunnett_t3(&groups).unwrap();

        // Should have C(3,2) = 3 comparisons
        assert_eq!(results.len(), 3);

        for (i, j, p) in &results {
            println!("Group {} vs {}: P = {}", i, j, p);
        }
    }

    #[test]
    fn test_dunnett_t3_different_groups() {
        // Groups with different means
        let groups = vec![
            vec![1.0, 1.1, 1.2, 1.3],
            vec![5.0, 5.1, 5.2, 5.3],
            vec![10.0, 10.1, 10.2, 10.3],
        ];

        let results = dunnett_t3(&groups).unwrap();

        // All comparisons should show significant differences
        for (i, j, p) in &results {
            println!("Group {} vs {}: P = {}", i, j, p);
            assert!(*p < 0.01, "Should detect significant difference");
        }
    }

    #[test]
    fn test_welch_df_calculation() {
        // Test degrees of freedom calculation
        let df = welch_df(1.0, 10, 4.0, 15);
        assert!(df > 0.0, "DF should be positive");
        assert!(df < 24.0, "DF should be less than n1 + n2 - 2");
    }
}
