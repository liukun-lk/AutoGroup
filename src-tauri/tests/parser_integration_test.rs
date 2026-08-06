use autogroup_lib::core::parser::parse_excel_file;

#[test]
fn test_parse_user_file() {
    // Repo-relative fixture: this used to point at a file in one developer's Downloads
    // folder, so it silently skipped everywhere else and protected nothing.
    let path = &format!(
        "{}/tests/fixtures/e2e_input.xlsx",
        env!("CARGO_MANIFEST_DIR")
    );

    match parse_excel_file(path) {
        Ok(dataset) => {
            println!("✓ Successfully parsed Excel file");
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
            for (idx, name) in dataset.indicator_names.iter().take(10).enumerate() {
                println!("  {}. {}", idx + 1, name);
            }

            // Basic assertions
            assert!(dataset.metadata.total_animals > 0);
            assert!(dataset.metadata.indicator_count > 0);
        }
        Err(e) => {
            panic!("Failed to parse: {e}");
        }
    }
}
