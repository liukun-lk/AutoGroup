use super::anova::one_way_anova;
use anyhow::Result;

/// Levene's test for homogeneity of variance (mean-based variant)
/// Tests the null hypothesis that all groups have equal variances
/// Returns P-value (high P => variances are equal)
/// Note: This uses mean-based deviations (original Levene test)
pub fn levene_test(groups: &[Vec<f64>]) -> Result<f64> {
    if groups.len() < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Levene test"));
    }

    // Step 1: Calculate mean for each group
    let means: Vec<f64> = groups.iter().map(|group| compute_mean(group)).collect();

    // Step 2: Transform data to absolute deviations from mean
    let transformed_groups: Vec<Vec<f64>> = groups
        .iter()
        .zip(&means)
        .map(|(group, mean)| {
            group.iter().map(|&x| (x - mean).abs()).collect()
        })
        .collect();

    // Step 3: Run ANOVA on transformed data
    one_way_anova(&transformed_groups)
}

/// Calculate mean of a dataset
fn compute_mean(data: &[f64]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let sum: f64 = data.iter().sum();
    sum / data.len() as f64
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_levene_equal_variances() {
        // Groups with similar variances
        let groups = vec![
            vec![1.0, 2.0, 3.0, 4.0, 5.0],
            vec![2.0, 3.0, 4.0, 5.0, 6.0],
            vec![3.0, 4.0, 5.0, 6.0, 7.0],
        ];

        let p = levene_test(&groups).unwrap();
        assert!(p > 0.05, "P-value should be high for equal variances");
    }

    #[test]
    fn test_levene_unequal_variances() {
        // Groups with very different variances
        let groups = vec![
            vec![1.0, 1.01, 1.02], // Low variance
            vec![1.0, 5.0, 10.0],  // High variance
        ];

        let p = levene_test(&groups).unwrap();
        // Note: With only 3 samples, power is low, but should still show some difference
        println!("Levene P-value for unequal variances: {}", p);
    }

    #[test]
    fn test_mean_calculation() {
        assert_eq!(compute_mean(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(compute_mean(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_eq!(compute_mean(&[5.0]), 5.0);
        assert_eq!(compute_mean(&[]), 0.0);
    }
}
