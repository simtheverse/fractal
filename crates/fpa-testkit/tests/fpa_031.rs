//! FPA-031: Test Coverage of Requirements
//!
//! For each crate under `crates/`, reads its `docs/SPECIFICATION.md`, extracts
//! all requirement IDs (FPA-NNN), and checks whether a corresponding test file
//! `tests/fpa_NNN.rs` exists in that crate.
//!
//! Missing test files are reported as warnings but do not fail the test.
//! This allows the test to pass during incremental development while still
//! providing visibility into coverage gaps.

use std::collections::{BTreeMap, BTreeSet};
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

/// Validates test file naming conventions across all crates.
///
/// Every `fpa_NNN.rs` test file (and `fpa_NNN_*.rs` variant) should correspond to a
/// requirement ID that exists in the owning crate's SPECIFICATION.md or in a parent
/// spec. This catches stale test files that reference requirements that have been
/// removed or renumbered.
#[test]
fn test_file_naming_matches_requirements() {
    let mut stale_files: Vec<String> = Vec::new();

    // Collect all known requirement IDs across all specs (main + conventions + crates)
    let mut all_known_ids = BTreeSet::new();

    // Main spec
    let main_spec = workspace_root().join("docs").join("design").join("SPECIFICATION.md");
    if main_spec.is_file() {
        let content = fs::read_to_string(&main_spec).unwrap();
        all_known_ids.extend(extract_requirement_ids(&content));
    }

    // Conventions spec
    let conv_spec = workspace_root().join("docs").join("design").join("CONVENTIONS.md");
    if conv_spec.is_file() {
        let content = fs::read_to_string(&conv_spec).unwrap();
        all_known_ids.extend(extract_requirement_ids(&content));
    }

    // Crate specs
    for crate_dir in crate_dirs() {
        let spec_path = crate_dir.join("docs").join("SPECIFICATION.md");
        if spec_path.is_file() {
            let content = fs::read_to_string(&spec_path).unwrap();
            all_known_ids.extend(extract_requirement_ids(&content));
        }
    }

    // Now scan all test files and check naming
    for crate_dir in crate_dirs() {
        let crate_name = crate_dir.file_name().unwrap().to_string_lossy().to_string();
        let tests_dir = crate_dir.join("tests");
        if !tests_dir.is_dir() {
            continue;
        }

        let entries = match fs::read_dir(&tests_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.starts_with("fpa_") || !file_name.ends_with(".rs") {
                continue;
            }

            // Extract the numeric portion: fpa_NNN.rs or fpa_NNN_suffix.rs -> NNN
            let stem = &file_name[4..file_name.len() - 3]; // strip "fpa_" and ".rs"
            let num_part: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();

            if num_part.is_empty() {
                continue;
            }

            let req_id = format!("FPA-{}", num_part);

            if !all_known_ids.contains(&req_id) {
                stale_files.push(format!("  {}/tests/{} -> {} (not in any spec)", crate_name, file_name, req_id));
            }
        }
    }

    if !stale_files.is_empty() {
        eprintln!(
            "\n[WARNING] Test files reference requirement IDs not found in any specification:\n{}\n\
             These may be stale tests for removed/renumbered requirements.\n",
            stale_files.join("\n")
        );
    }

    // Strict assertion: test files must reference known requirement IDs
    assert!(
        stale_files.is_empty(),
        "Test files reference unknown requirement IDs:\n{}\n\
         Every fpa_NNN.rs test file must correspond to a requirement in some specification.",
        stale_files.join("\n")
    );
}

/// Cross-crate coverage: verifies that across the entire workspace, every requirement
/// ID that appears in any crate's SPECIFICATION.md has at least one corresponding test
/// file somewhere in the workspace (not necessarily in the same crate).
///
/// This is a broader check than the per-crate report above. A requirement might be
/// tested in a different crate (e.g., integration tests in fpa-compositor testing
/// requirements defined in fpa-contract).
#[test]
fn cross_crate_requirement_coverage_report() {
    // Collect all requirement IDs from all crate specs
    let mut all_req_ids = BTreeSet::new();
    for crate_dir in crate_dirs() {
        let spec_path = crate_dir.join("docs").join("SPECIFICATION.md");
        if !spec_path.is_file() {
            continue;
        }
        let content = fs::read_to_string(&spec_path).unwrap();
        all_req_ids.extend(extract_requirement_ids(&content));
    }

    // Collect all test file IDs from all crates (fpa_NNN.rs or fpa_NNN_*.rs)
    let mut tested_ids = BTreeSet::new();
    for crate_dir in crate_dirs() {
        let tests_dir = crate_dir.join("tests");
        if !tests_dir.is_dir() {
            continue;
        }
        let entries = match fs::read_dir(&tests_dir) {
            Ok(e) => e,
            Err(_) => continue,
        };
        for entry in entries.flatten() {
            let file_name = entry.file_name().to_string_lossy().to_string();
            if !file_name.starts_with("fpa_") || !file_name.ends_with(".rs") {
                continue;
            }
            let stem = &file_name[4..file_name.len() - 3];
            let num_part: String = stem.chars().take_while(|c| c.is_ascii_digit()).collect();
            if !num_part.is_empty() {
                tested_ids.insert(format!("FPA-{}", num_part));
            }
        }
    }

    let untested: Vec<&String> = all_req_ids.iter().filter(|id| !tested_ids.contains(*id)).collect();

    eprintln!();
    eprintln!("=== Cross-Crate Coverage Report ===");
    eprintln!();
    eprintln!(
        "  Requirements with test files: {}/{}",
        all_req_ids.len() - untested.len(),
        all_req_ids.len()
    );

    if !untested.is_empty() {
        eprintln!("  Requirements without any test file in the workspace:");
        for id in &untested {
            eprintln!("    {}", id);
        }
    }
    eprintln!();
    eprintln!("=== End Cross-Crate Coverage Report ===");
    eprintln!();

    // Informational only — does not fail during incremental development.
}
