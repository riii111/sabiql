use std::path::PathBuf;

pub fn write_er_failure_log_blocking(
    failed_tables: Vec<(String, String)>,
    cache_dir: PathBuf,
) -> std::io::Result<()> {
    use std::io::Write;

    let log_path = cache_dir.join("er_diagram.log");
    let mut file = std::fs::File::create(&log_path)?;

    writeln!(file, "ER Diagram Generation Failed")?;
    writeln!(file, "Timestamp: {:?}", std::time::SystemTime::now())?;
    writeln!(file)?;
    writeln!(file, "Failed tables ({}):", failed_tables.len())?;

    for (table, error) in &failed_tables {
        writeln!(file, "  - {}: {}", table, error)?;
    }

    Ok(())
}

#[cfg(test)]
mod tests {
    use super::*;

    mod write_er_failure_log_blocking {
        use super::*;

        #[test]
        fn multiple_failures_writes_all_details() {
            let temp_dir = tempfile::tempdir().unwrap();
            let failed_tables = vec![
                ("public.users".to_string(), "connection timeout".to_string()),
                ("public.orders".to_string(), "permission denied".to_string()),
            ];

            write_er_failure_log_blocking(failed_tables, temp_dir.path().to_path_buf()).unwrap();

            let content = std::fs::read_to_string(temp_dir.path().join("er_diagram.log")).unwrap();
            assert!(content.contains("ER Diagram Generation Failed"));
            assert!(content.contains("Failed tables (2):"));
            assert!(content.contains("public.users: connection timeout"));
            assert!(content.contains("public.orders: permission denied"));
        }

        #[test]
        fn empty_list_writes_zero_count() {
            let temp_dir = tempfile::tempdir().unwrap();

            write_er_failure_log_blocking(vec![], temp_dir.path().to_path_buf()).unwrap();

            let content = std::fs::read_to_string(temp_dir.path().join("er_diagram.log")).unwrap();
            assert!(content.contains("Failed tables (0):"));
        }

        #[test]
        fn output_includes_timestamp() {
            let temp_dir = tempfile::tempdir().unwrap();

            write_er_failure_log_blocking(
                vec![("t".to_string(), "e".to_string())],
                temp_dir.path().to_path_buf(),
            )
            .unwrap();

            let content = std::fs::read_to_string(temp_dir.path().join("er_diagram.log")).unwrap();
            assert!(content.contains("Timestamp:"));
        }
    }
}
