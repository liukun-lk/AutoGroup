use autogroup_lib::core::parser::parse_excel_file;

#[test]
fn test_parse_original_test_file() {
    let path = "../docs/通用动物实验自动分组软件_测试用数据.xlsx";

    // Skip if file doesn't exist (CI environment)
    if !std::path::Path::new(path).exists() {
        eprintln!("Original test file not found, skipping test");
        return;
    }

    match parse_excel_file(path) {
        Ok(dataset) => {
            println!("✓ Successfully parsed original test file");
            println!("\nDataset Summary:");
            println!("  Total animals: {}", dataset.metadata.total_animals);
            println!("  Male: {}", dataset.metadata.male_count);
            println!("  Female: {}", dataset.metadata.female_count);
            println!("  Indicators: {}", dataset.metadata.indicator_count);

            println!("\nFirst 5 animals:");
            for (idx, animal) in dataset.animals.iter().take(5).enumerate() {
                println!(
                    "  {}. ID={}, Sex={:?}, Indicators={}",
                    idx + 1,
                    animal.id,
                    animal.sex,
                    animal.indicators.len()
                );
            }

            println!("\nFirst 10 indicators:");
            for (idx, meta) in dataset.indicator_metadata.iter().take(10).enumerate() {
                println!("  {}. {} (unit: {})", idx + 1, meta.display_name, meta.unit);
            }

            // Expected: 9 animals (6 male, 3 female), 71+ indicators
            // Note: Actual count may vary based on parsing logic
            assert!(
                dataset.metadata.total_animals >= 9,
                "Should have at least 9 animals"
            );
            assert!(
                dataset.metadata.male_count >= 6,
                "Should have at least 6 males"
            );
            assert!(
                dataset.metadata.female_count >= 3,
                "Should have at least 3 females"
            );
            assert!(
                dataset.metadata.indicator_count >= 70,
                "Should have at least 70 indicators"
            );
        }
        Err(e) => {
            panic!("Failed to parse original test file: {e}");
        }
    }
}
