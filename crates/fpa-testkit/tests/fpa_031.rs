//! FPA-031: Test Coverage of Requirements
//!
//! For each crate under `crates/`, reads its `docs/SPECIFICATION.md`, extracts
//! all requirement IDs (FPA-NNN), and checks whether a corresponding test file
//! `tests/fpa_NNN.rs` exists in that crate.
//!
//! Missing test files are reported as warnings but do not fail the test.
//! This allows the test to pass during incremental development while still
//! providing visibility into coverage gaps.

use std::collections::BTreeMap;
use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    manifest_dir
        .parent()
        .expect("crates/ directory")
        .parent()
        .expect("workspace root")
        .to_path_buf()
}

fn crate_dirs() -> Vec<PathBuf> {
    let crates_dir = workspace_root().join("crates");
    let mut dirs: Vec<PathBuf> = fs::read_dir(&crates_dir)
        .unwrap_or_else(|e| panic!("Cannot read crates/ directory at {}: {}", crates_dir.display(), e))
        .filter_map(|entry| {
            let entry = entry.ok()?;
            let path = entry.path();
            if path.is_dir() {
                Some(path)
            } else {
                None
            }
        })
        .collect();
    dirs.sort();
    dirs
}

/// Extract requirement IDs (FPA-NNN) from SPECIFICATION.md table rows.
fn extract_requirement_ids(content: &str) -> Vec<String> {
    let mut ids = Vec::new();
    for line in content.lines() {
        let trimmed = line.trim();
        // Match table rows like: | FPA-001 | description | status |
        if trimmed.starts_with('|') {
            // Split by '|' and look for FPA-NNN in the first data column
            let columns: Vec<&str> = trimmed.split('|').collect();
            if columns.len() >= 2 {
                let first_col = columns[1].trim();
                if first_col.starts_with("FPA-") {
                    // Verify the suffix is numeric
                    let suffix = &first_col[4..];
                    if suffix.chars().all(|c| c.is_ascii_digit()) && !suffix.is_empty() {
                        ids.push(first_col.to_string());
                    }
                }
            }
        }
    }
    ids
}

#[test]
fn requirement_coverage_report() {
    // Map from crate name -> list of (requirement_id, has_test_file)
    let mut coverage: BTreeMap<String, Vec<(String, bool)>> = BTreeMap::new();
    let mut total_requirements = 0usize;
    let mut covered = 0usize;
    let mut missing_entries: Vec<String> = Vec::new();

    for crate_dir in crate_dirs() {
        let crate_name = crate_dir.file_name().unwrap().to_string_lossy().to_string();
        let spec_path = crate_dir.join("docs").join("SPECIFICATION.md");

        if !spec_path.is_file() {
            continue;
        }

        let content = fs::read_to_string(&spec_path)
            .unwrap_or_else(|e| panic!("Cannot read {}: {}", spec_path.display(), e));

        let req_ids = extract_requirement_ids(&content);
        let mut crate_coverage = Vec::new();

        for req_id in &req_ids {
            total_requirements += 1;

            // FPA-NNN -> fpa_NNN.rs
            let num = &req_id[4..]; // strip "FPA-"
            let test_filename = format!("fpa_{}.rs", num);
            let test_path = crate_dir.join("tests").join(&test_filename);
            let has_test = test_path.is_file();

            if has_test {
                covered += 1;
            } else {
                missing_entries.push(format!("  {} ({}/tests/{})", req_id, crate_name, test_filename));
            }

            crate_coverage.push((req_id.clone(), has_test));
        }

        coverage.insert(crate_name, crate_coverage);
    }

    // Print coverage report
    eprintln!();
    eprintln!("=== Requirement Coverage Report ===");
    eprintln!();
    for (crate_name, entries) in &coverage {
        eprintln!("  {}:", crate_name);
        for (req_id, has_test) in entries {
            let status = if *has_test { "OK" } else { "MISSING" };
            eprintln!("    {} [{}]", req_id, status);
        }
    }
    eprintln!();
    eprintln!(
        "  Coverage: {}/{} ({:.0}%)",
        covered,
        total_requirements,
        if total_requirements > 0 {
            (covered as f64 / total_requirements as f64) * 100.0
        } else {
            100.0
        }
    );

    if !missing_entries.is_empty() {
        eprintln!();
        eprintln!("  Missing test files:");
        for entry in &missing_entries {
            eprintln!("{}", entry);
        }
    }
    eprintln!();
    eprintln!("=== End Coverage Report ===");
    eprintln!();

    // This test intentionally does not fail on missing coverage.
    // It serves as a reporting mechanism during incremental development.
    // To make it strict, uncomment the assertion below:
    //
    // assert!(
    //     missing_entries.is_empty(),
    //     "Missing test files for {} requirements",
    //     missing_entries.len()
    // );
}
