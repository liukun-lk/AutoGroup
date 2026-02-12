use crate::core::models::*;
use anyhow::{anyhow, Result};
use rand::seq::SliceRandom;
use rand::thread_rng;

/// Generate all possible groupings through exhaustive enumeration
/// Suitable for datasets with ≤50 animals
/// Automatically switches to Monte Carlo sampling if combination count exceeds threshold
pub fn enumerate_all(
    animals: &[Animal],
    config: &GroupConfig,
) -> Result<Vec<CandidateGrouping>> {
    const MAX_EXHAUSTIVE_COMBINATIONS: usize = 500_000;
    const MONTE_CARLO_SAMPLE_SIZE: usize = 100_000;

    // Step 1: Validate configuration
    validate_config(animals, config)?;

    // Step 2: Separate animals by sex
    let male_indices: Vec<usize> = animals
        .iter()
        .enumerate()
        .filter(|(_, a)| a.sex == Sex::Male)
        .map(|(i, _)| i)
        .collect();

    let female_indices: Vec<usize> = animals
        .iter()
        .enumerate()
        .filter(|(_, a)| a.sex == Sex::Female)
        .map(|(i, _)| i)
        .collect();

    // Step 3: Estimate combination count
    let estimated_count = estimate_combination_count(&male_indices, &female_indices, config);

    println!(
        "Estimated combinations: {} (threshold: {})",
        estimated_count, MAX_EXHAUSTIVE_COMBINATIONS
    );

    // Step 4: Choose enumeration strategy
    let all_groupings = if config.num_groups == 2 {
        // Use optimized 2-group algorithm
        enumerate_two_groups(&male_indices, &female_indices, config)?
    } else if estimated_count <= MAX_EXHAUSTIVE_COMBINATIONS {
        // Use exhaustive enumeration for multi-group
        enumerate_multi_groups_exhaustive(&male_indices, &female_indices, config)?
    } else {
        // Use Monte Carlo sampling for large combination spaces
        println!("Using Monte Carlo sampling with {} samples", MONTE_CARLO_SAMPLE_SIZE);
        enumerate_multi_groups_sampling(
            &male_indices,
            &female_indices,
            config,
            MONTE_CARLO_SAMPLE_SIZE,
        )?
    };

    if all_groupings.is_empty() {
        return Err(anyhow!(
            "No valid groupings found. Check sex constraints and animal counts."
        ));
    }

    println!("Generated {} candidate groupings", all_groupings.len());
    Ok(all_groupings)
}

/// Optimized enumeration for 2 groups
fn enumerate_two_groups(
    male_indices: &[usize],
    female_indices: &[usize],
    config: &GroupConfig,
) -> Result<Vec<CandidateGrouping>> {
    let mut all_groupings = Vec::new();

    let group1_constraint = &config.sex_constraints[0];
    let group2_constraint = &config.sex_constraints[1];

    // Generate combinations for group 1
    let male_combos_g1 = combinations(male_indices, group1_constraint.male_count);
    let female_combos_g1 = combinations(female_indices, group1_constraint.female_count);

    for male_combo in &male_combos_g1 {
        for female_combo in &female_combos_g1 {
            // Group 1 animals
            let mut group1 = male_combo.clone();
            group1.extend_from_slice(female_combo);

            // Group 2 gets the remaining animals
            let remaining_males: Vec<usize> = male_indices
                .iter()
                .copied()
                .filter(|idx| !male_combo.contains(idx))
                .collect();

            let remaining_females: Vec<usize> = female_indices
                .iter()
                .copied()
                .filter(|idx| !female_combo.contains(idx))
                .collect();

            // Validate group 2 has correct counts
            if remaining_males.len() == group2_constraint.male_count
                && remaining_females.len() == group2_constraint.female_count
            {
                let mut group2 = remaining_males;
                group2.extend_from_slice(&remaining_females);

                all_groupings.push(CandidateGrouping {
                    groups: vec![group1, group2],
                });
            }
        }
    }

    Ok(all_groupings)
}

/// Exhaustive enumeration for multi-group (≥3 groups)
fn enumerate_multi_groups_exhaustive(
    male_indices: &[usize],
    female_indices: &[usize],
    config: &GroupConfig,
) -> Result<Vec<CandidateGrouping>> {
    let groupings = enumerate_recursive(
        male_indices,
        female_indices,
        &config.sex_constraints,
        Vec::new(),
    );

    Ok(groupings)
}

/// Recursive enumeration helper
fn enumerate_recursive(
    male_pool: &[usize],
    female_pool: &[usize],
    constraints: &[SexConstraint],
    current_groups: Vec<Vec<usize>>,
) -> Vec<CandidateGrouping> {
    // Base case: only one group left
    if constraints.len() == 1 {
        let last_constraint = &constraints[0];

        // Check if remaining animals exactly match the constraint
        if male_pool.len() == last_constraint.male_count
            && female_pool.len() == last_constraint.female_count
        {
            let mut last_group = male_pool.to_vec();
            last_group.extend_from_slice(female_pool);

            let mut final_groups = current_groups;
            final_groups.push(last_group);

            return vec![CandidateGrouping {
                groups: final_groups,
            }];
        } else {
            return Vec::new(); // Invalid, no match
        }
    }

    // Recursive case: assign current group and recurse
    let current_constraint = &constraints[0];
    let remaining_constraints = &constraints[1..];

    let mut results = Vec::new();

    // Generate combinations for current group
    let male_combos = combinations(male_pool, current_constraint.male_count);
    let female_combos = combinations(female_pool, current_constraint.female_count);

    for male_combo in &male_combos {
        for female_combo in &female_combos {
            // Build current group
            let mut current_group = male_combo.clone();
            current_group.extend_from_slice(female_combo);

            // Calculate remaining pools
            let remaining_males: Vec<usize> = male_pool
                .iter()
                .copied()
                .filter(|idx| !male_combo.contains(idx))
                .collect();

            let remaining_females: Vec<usize> = female_pool
                .iter()
                .copied()
                .filter(|idx| !female_combo.contains(idx))
                .collect();

            // Build updated groups list
            let mut updated_groups = current_groups.clone();
            updated_groups.push(current_group);

            // Recurse
            let sub_groupings = enumerate_recursive(
                &remaining_males,
                &remaining_females,
                remaining_constraints,
                updated_groups,
            );

            results.extend(sub_groupings);
        }
    }

    results
}

/// Monte Carlo sampling for large combination spaces
fn enumerate_multi_groups_sampling(
    male_indices: &[usize],
    female_indices: &[usize],
    config: &GroupConfig,
    sample_size: usize,
) -> Result<Vec<CandidateGrouping>> {
    let mut rng = thread_rng();
    let mut samples = Vec::new();

    for _ in 0..sample_size {
        // Randomly shuffle animals
        let mut male_pool = male_indices.to_vec();
        let mut female_pool = female_indices.to_vec();

        male_pool.shuffle(&mut rng);
        female_pool.shuffle(&mut rng);

        // Sequentially assign animals to groups
        let mut groups = Vec::new();
        let mut male_offset = 0;
        let mut female_offset = 0;

        let mut valid = true;

        for constraint in &config.sex_constraints {
            // Check if enough animals remaining
            if male_offset + constraint.male_count > male_pool.len()
                || female_offset + constraint.female_count > female_pool.len()
            {
                valid = false;
                break;
            }

            let mut group = Vec::new();

            // Take males
            for i in male_offset..(male_offset + constraint.male_count) {
                group.push(male_pool[i]);
            }
            male_offset += constraint.male_count;

            // Take females
            for i in female_offset..(female_offset + constraint.female_count) {
                group.push(female_pool[i]);
            }
            female_offset += constraint.female_count;

            groups.push(group);
        }

        if valid {
            samples.push(CandidateGrouping { groups });
        }
    }

    Ok(samples)
}

/// Estimate total combination count
fn estimate_combination_count(
    male_indices: &[usize],
    female_indices: &[usize],
    config: &GroupConfig,
) -> usize {
    let mut count: usize = 1;

    let mut remaining_males = male_indices.len();
    let mut remaining_females = female_indices.len();

    for constraint in &config.sex_constraints {
        // C(remaining_males, male_count) * C(remaining_females, female_count)
        let male_combos = binomial_coefficient(remaining_males, constraint.male_count);
        let female_combos = binomial_coefficient(remaining_females, constraint.female_count);

        count = count.saturating_mul(male_combos).saturating_mul(female_combos);

        remaining_males -= constraint.male_count;
        remaining_females -= constraint.female_count;

        // Prevent overflow
        if count > 10_000_000 {
            return usize::MAX;
        }
    }

    count
}

/// Calculate binomial coefficient C(n, k)
fn binomial_coefficient(n: usize, k: usize) -> usize {
    if k > n {
        return 0;
    }
    if k == 0 || k == n {
        return 1;
    }

    let k = k.min(n - k); // Optimization: C(n,k) = C(n,n-k)
    let mut result: usize = 1;

    for i in 0..k {
        result = result.saturating_mul(n - i) / (i + 1);
    }

    result
}

/// Validate that the configuration is feasible
fn validate_config(animals: &[Animal], config: &GroupConfig) -> Result<()> {
    let male_count = animals.iter().filter(|a| a.sex == Sex::Male).count();
    let female_count = animals.iter().filter(|a| a.sex == Sex::Female).count();

    // Check total animal count matches
    let total_required: usize = config
        .sex_constraints
        .iter()
        .map(|c| c.male_count + c.female_count)
        .sum();

    if total_required != animals.len() {
        return Err(anyhow!(
            "Total animals in constraints ({}) doesn't match dataset ({})",
            total_required,
            animals.len()
        ));
    }

    // Check sex counts match
    let total_males_required: usize = config
        .sex_constraints
        .iter()
        .map(|c| c.male_count)
        .sum();

    let total_females_required: usize = config
        .sex_constraints
        .iter()
        .map(|c| c.female_count)
        .sum();

    if total_males_required != male_count {
        return Err(anyhow!(
            "Total males required ({}) doesn't match available ({})",
            total_males_required,
            male_count
        ));
    }

    if total_females_required != female_count {
        return Err(anyhow!(
            "Total females required ({}) doesn't match available ({})",
            total_females_required,
            female_count
        ));
    }

    Ok(())
}

/// Generate all combinations of choosing k items from a list
/// Returns Vec<Vec<usize>> where each inner Vec is one combination
fn combinations(items: &[usize], k: usize) -> Vec<Vec<usize>> {
    if k == 0 {
        return vec![Vec::new()];
    }

    if k > items.len() {
        return Vec::new();
    }

    if k == items.len() {
        return vec![items.to_vec()];
    }

    let mut result = Vec::new();

    // Recursive approach
    for i in 0..=items.len() - k {
        let first = items[i];
        let rest = &items[i + 1..];
        let sub_combos = combinations(rest, k - 1);

        for mut combo in sub_combos {
            combo.insert(0, first);
            result.push(combo);
        }
    }

    result
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::collections::HashMap;

    #[test]
    fn test_combinations() {
        let items = vec![0, 1, 2, 3];

        // Choose 2 from 4: should be C(4,2) = 6
        let combos = combinations(&items, 2);
        assert_eq!(combos.len(), 6);
        assert!(combos.contains(&vec![0, 1]));
        assert!(combos.contains(&vec![0, 2]));
        assert!(combos.contains(&vec![0, 3]));
        assert!(combos.contains(&vec![1, 2]));
        assert!(combos.contains(&vec![1, 3]));
        assert!(combos.contains(&vec![2, 3]));

        // Choose 0 from 4: should be 1 (empty set)
        let combos = combinations(&items, 0);
        assert_eq!(combos.len(), 1);
        assert_eq!(combos[0], Vec::<usize>::new());

        // Choose 4 from 4: should be 1 (all items)
        let combos = combinations(&items, 4);
        assert_eq!(combos.len(), 1);
        assert_eq!(combos[0], items);
    }

    #[test]
    fn test_enumerate_all_simple() {
        // Create test dataset: 6 males, 4 females
        let mut animals = Vec::new();

        for i in 0..6 {
            animals.push(Animal {
                id: format!("M{}", i),
                sex: Sex::Male,
                indicators: HashMap::new(),
            });
        }

        for i in 0..4 {
            animals.push(Animal {
                id: format!("F{}", i),
                sex: Sex::Female,
                indicators: HashMap::new(),
            });
        }

        // Config: 2 groups, 5 animals each (3M+2F per group)
        let config = GroupConfig {
            num_groups: 2,
            animals_per_group: GroupSize::Uniform { value: 5 },
            sex_constraints: vec![
                SexConstraint {
                    group_index: 0,
                    male_count: 3,
                    female_count: 2,
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 3,
                    female_count: 2,
                },
            ],
        };

        let groupings = enumerate_all(&animals, &config).unwrap();

        // Expected: C(6,3) * C(4,2) = 20 * 6 = 120
        assert_eq!(groupings.len(), 120);

        // Validate first grouping
        let first = &groupings[0];
        assert_eq!(first.groups.len(), 2);
        assert_eq!(first.groups[0].len(), 5);
        assert_eq!(first.groups[1].len(), 5);

        // Validate no overlap between groups
        let group1_set: std::collections::HashSet<_> = first.groups[0].iter().collect();
        let group2_set: std::collections::HashSet<_> = first.groups[1].iter().collect();
        assert_eq!(group1_set.intersection(&group2_set).count(), 0);
    }

    #[test]
    fn test_validate_config_mismatch() {
        let animals = vec![
            Animal {
                id: "M1".to_string(),
                sex: Sex::Male,
                indicators: HashMap::new(),
            },
            Animal {
                id: "F1".to_string(),
                sex: Sex::Female,
                indicators: HashMap::new(),
            },
        ];

        // Config requires 3 males but only 1 available
        let config = GroupConfig {
            num_groups: 1,
            animals_per_group: GroupSize::Uniform { value: 2 },
            sex_constraints: vec![SexConstraint {
                group_index: 0,
                male_count: 3,
                female_count: 0,
            }],
        };

        let result = validate_config(&animals, &config);
        assert!(result.is_err());
    }

    #[test]
    fn test_enumerate_three_groups() {
        // Create test dataset: 6 males, 3 females (total 9 animals)
        let mut animals = Vec::new();

        for i in 0..6 {
            animals.push(Animal {
                id: format!("M{}", i),
                sex: Sex::Male,
                indicators: HashMap::new(),
            });
        }

        for i in 0..3 {
            animals.push(Animal {
                id: format!("F{}", i),
                sex: Sex::Female,
                indicators: HashMap::new(),
            });
        }

        // Config: 3 groups, each with 2M+1F
        let config = GroupConfig {
            num_groups: 3,
            animals_per_group: GroupSize::Uniform { value: 3 },
            sex_constraints: vec![
                SexConstraint {
                    group_index: 0,
                    male_count: 2,
                    female_count: 1,
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 2,
                    female_count: 1,
                },
                SexConstraint {
                    group_index: 2,
                    male_count: 2,
                    female_count: 1,
                },
            ],
        };

        let groupings = enumerate_all(&animals, &config).unwrap();

        // Expected: C(6,2) * C(3,1) * C(4,2) * C(2,1) * C(2,2) * C(1,1)
        //         = 15 * 3 * 6 * 2 * 1 * 1 = 540
        assert_eq!(groupings.len(), 540);

        // Validate first grouping structure
        let first = &groupings[0];
        assert_eq!(first.groups.len(), 3);
        assert_eq!(first.groups[0].len(), 3);
        assert_eq!(first.groups[1].len(), 3);
        assert_eq!(first.groups[2].len(), 3);

        // Validate no overlap
        let mut all_indices = std::collections::HashSet::new();
        for group in &first.groups {
            for &idx in group {
                assert!(
                    all_indices.insert(idx),
                    "Animal {} appears in multiple groups",
                    idx
                );
            }
        }

        // Validate all animals are assigned
        assert_eq!(all_indices.len(), 9);
    }

    #[test]
    fn test_binomial_coefficient() {
        assert_eq!(binomial_coefficient(5, 2), 10); // C(5,2) = 10
        assert_eq!(binomial_coefficient(6, 3), 20); // C(6,3) = 20
        assert_eq!(binomial_coefficient(10, 0), 1); // C(10,0) = 1
        assert_eq!(binomial_coefficient(10, 10), 1); // C(10,10) = 1
        assert_eq!(binomial_coefficient(4, 2), 6); // C(4,2) = 6
    }

    #[test]
    fn test_estimate_combination_count() {
        let males = vec![0, 1, 2, 3, 4, 5];
        let females = vec![6, 7, 8];

        let config = GroupConfig {
            num_groups: 3,
            animals_per_group: GroupSize::Uniform { value: 3 },
            sex_constraints: vec![
                SexConstraint {
                    group_index: 0,
                    male_count: 2,
                    female_count: 1,
                },
                SexConstraint {
                    group_index: 1,
                    male_count: 2,
                    female_count: 1,
                },
                SexConstraint {
                    group_index: 2,
                    male_count: 2,
                    female_count: 1,
                },
            ],
        };

        let count = estimate_combination_count(&males, &females, &config);

        // C(6,2) * C(3,1) * C(4,2) * C(2,1) * 1 * 1 = 15 * 3 * 6 * 2 = 540
        assert_eq!(count, 540);
    }

}
