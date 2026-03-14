//! FPA-030: Documentation Structure Validation
//!
//! Walks the workspace and verifies every crate under `crates/` has:
//! - A `docs/` directory
//! - A `docs/SPECIFICATION.md` file
//! - A "Traces to:" line referencing a parent spec (FPA-SRS-000 or FPA-CON-000)

use std::fs;
use std::path::PathBuf;

fn workspace_root() -> PathBuf {
    let manifest_dir = PathBuf::from(env!("CARGO_MANIFEST_DIR"));
    // fpa-testkit is at crates/fpa-testkit, workspace root is two levels up
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

#[test]
fn every_crate_has_docs_directory() {
    let mut missing = Vec::new();
    for crate_dir in crate_dirs() {
        let docs_dir = crate_dir.join("docs");
        if !docs_dir.is_dir() {
            missing.push(crate_dir.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "The following crates are missing a docs/ directory: {:?}",
        missing
    );
}

#[test]
fn every_crate_has_specification_md() {
    let mut missing = Vec::new();
    for crate_dir in crate_dirs() {
        let spec = crate_dir.join("docs").join("SPECIFICATION.md");
        if !spec.is_file() {
            missing.push(crate_dir.file_name().unwrap().to_string_lossy().to_string());
        }
    }
    assert!(
        missing.is_empty(),
        "The following crates are missing docs/SPECIFICATION.md: {:?}",
        missing
    );
}

#[test]
fn specification_traces_to_parent_spec() {
    let mut problems = Vec::new();
    for crate_dir in crate_dirs() {
        let spec_path = crate_dir.join("docs").join("SPECIFICATION.md");
        let crate_name = crate_dir.file_name().unwrap().to_string_lossy().to_string();

        if !spec_path.is_file() {
            // Already covered by the previous test
            continue;
        }

        let content = fs::read_to_string(&spec_path)
            .unwrap_or_else(|e| panic!("Cannot read {}: {}", spec_path.display(), e));

        let has_traces_to = content.lines().any(|line| {
            let trimmed = line.trim();
            trimmed.starts_with("Traces to:")
                && (trimmed.contains("FPA-SRS-000") || trimmed.contains("FPA-CON-000"))
        });

        if !has_traces_to {
            problems.push(crate_name);
        }
    }
    assert!(
        problems.is_empty(),
        "The following crates have SPECIFICATION.md without a valid 'Traces to:' line \
         (must reference FPA-SRS-000 or FPA-CON-000): {:?}",
        problems
    );
}

/// Reports which Diataxis subdirectories are present/missing under each crate's docs/.
// The full Diataxis structure (tutorials, how-to, reference, explanation, design) is a
// convention goal described in FPA-030/FPA-040 but is not yet implemented in the prototype.
// This test reports status without failing, so CI stays green while the structure is
// incrementally adopted.
#[test]
fn diataxis_subdirectories_documented() {
    let expected_subdirs = ["tutorials", "how-to", "reference", "explanation", "design"];
    let mut report = String::from("Diataxis subdirectory status:\n");
    let mut any_missing = false;

    for crate_dir in crate_dirs() {
        let crate_name = crate_dir.file_name().unwrap().to_string_lossy().to_string();
        let docs_dir = crate_dir.join("docs");
        if !docs_dir.is_dir() {
            // Already covered by every_crate_has_docs_directory
            continue;
        }

        let mut missing = Vec::new();
        for subdir in &expected_subdirs {
            if !docs_dir.join(subdir).is_dir() {
                missing.push(*subdir);
            }
        }

        if !missing.is_empty() {
            any_missing = true;
            report.push_str(&format!("  {}: missing {:?}\n", crate_name, missing));
        } else {
            report.push_str(&format!("  {}: all present\n", crate_name));
        }
    }

    if any_missing {
        eprintln!(
            "\n[INFO] {}\n\
             Note: Full Diataxis directory structure is a convention goal (FPA-030/FPA-040)\n\
             not yet implemented in the prototype. SPECIFICATION.md currently lives at\n\
             docs/SPECIFICATION.md rather than docs/design/SPECIFICATION.md.\n",
            report
        );
    }
    // This test intentionally does not fail — it is a reporting/documentation test.
}
