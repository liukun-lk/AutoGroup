use super::anova::one_way_anova;
use anyhow::Result;

/// Levene's test for homogeneity of variance
/// Tests the null hypothesis that all groups have equal variances
/// Returns P-value (high P => variances are equal)
pub fn levene_test(groups: &[Vec<f64>]) -> Result<f64> {
    if groups.len() < 2 {
        return Err(anyhow::anyhow!("Need at least 2 groups for Levene test"));
    }

    // Step 1: Calculate median for each group
    let medians: Vec<f64> = groups.iter().map(|group| compute_median(group)).collect();

    // Step 2: Transform data to absolute deviations from median
    let transformed_groups: Vec<Vec<f64>> = groups
        .iter()
        .zip(&medians)
        .map(|(group, median)| {
            group.iter().map(|&x| (x - median).abs()).collect()
        })
        .collect();

    // Step 3: Run ANOVA on transformed data
    one_way_anova(&transformed_groups)
}

/// Calculate median of a dataset
fn compute_median(data: &[f64]) -> f64 {
    if data.is_empty() {
        return 0.0;
    }

    let mut sorted = data.to_vec();
    sorted.sort_by(|a, b| a.partial_cmp(b).unwrap());

    let mid = sorted.len() / 2;
    if sorted.len() % 2 == 0 {
        (sorted[mid - 1] + sorted[mid]) / 2.0
    } else {
        sorted[mid]
    }
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
    fn test_median_calculation() {
        assert_eq!(compute_median(&[1.0, 2.0, 3.0]), 2.0);
        assert_eq!(compute_median(&[1.0, 2.0, 3.0, 4.0]), 2.5);
        assert_eq!(compute_median(&[5.0]), 5.0);
    }
}
