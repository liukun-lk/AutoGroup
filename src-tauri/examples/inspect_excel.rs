use calamine::{open_workbook, Reader, Xlsx};

fn main() {
    let path = std::env::args()
        .nth(1)
        .expect("Usage: inspect_excel <path>");

    let mut workbook: Xlsx<_> = open_workbook(&path).expect("Failed to open");
    let sheet_names = workbook.sheet_names().to_vec();

    println!("Sheet names: {sheet_names:?}");

    if let Ok(range) = workbook.worksheet_range(&sheet_names[0]) {
        let rows: Vec<_> = range.rows().collect();
        println!("\nTotal rows: {}", rows.len());
        println!("\n=== First 10 rows ===");

        for (idx, row) in rows.iter().take(10).enumerate() {
            println!("\nRow {}: {} cells", idx, row.len());
            for (col_idx, cell) in row.iter().take(5).enumerate() {
                println!("  [{col_idx}]: {cell:?}");
            }
        }
    }
}
