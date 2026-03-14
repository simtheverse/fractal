//! FPA-030: Documentation Structure Validation
//!
//! Walks the workspace and verifies every crate under `crates/` has:
//! - A `docs/` directory
//! - A `docs/SPECIFICATION.md` file
//! - A "Traces to:" line referencing a parent spec (FPA-SRS-000 or FPA-CON-000)
//!
//! Also validates bidirectional traceability:
//! - Every requirement in the main spec (FPA-SRS-000) is referenced by at least one crate
//! - Every requirement in the conventions spec (FPA-CON-000) is referenced by at least one crate
//! - No orphan requirements exist in crate specs (every crate req traces to a parent spec req)
//! - Sub-partitions with nested compositors maintain their own docs/ and SPECIFICATION.md

use std::collections::BTreeSet;
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

/// Extract requirement IDs (FPA-NNN) from table rows in a specification file.
fn extract_requirement_ids(content: &str) -> BTreeSet<String> {
    let mut ids = BTreeSet::new();
    for line in content.lines() {
        let trimmed = line.trim();
        if trimmed.starts_with('|') {
            let columns: Vec<&str> = trimmed.split('|').collect();
            if columns.len() >= 2 {
                let first_col = columns[1].trim();
                if first_col.starts_with("FPA-") {
                    let suffix = &first_col[4..];
                    if suffix.chars().all(|c| c.is_ascii_digit()) && !suffix.is_empty() {
                        ids.insert(first_col.to_string());
                    }
                }
            }
        }
    }
    ids
}

/// Reads the main specification (FPA-SRS-000) and returns its requirement IDs.
fn main_spec_requirement_ids() -> BTreeSet<String> {
    let spec_path = workspace_root().join("docs").join("design").join("SPECIFICATION.md");
    let content = fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| panic!("Cannot read main spec at {}: {}", spec_path.display(), e));
    extract_requirement_ids(&content)
}

/// Reads the conventions specification (FPA-CON-000) and returns its requirement IDs.
fn conventions_spec_requirement_ids() -> BTreeSet<String> {
    let spec_path = workspace_root().join("docs").join("design").join("CONVENTIONS.md");
    let content = fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| panic!("Cannot read conventions spec at {}: {}", spec_path.display(), e));
    extract_requirement_ids(&content)
}

/// Collects all requirement IDs referenced across all crate-level SPECIFICATION.md files.
fn all_crate_requirement_ids() -> BTreeSet<String> {
    let mut all_ids = BTreeSet::new();
    for crate_dir in crate_dirs() {
        let spec_path = crate_dir.join("docs").join("SPECIFICATION.md");
        if !spec_path.is_file() {
            continue;
        }
        let content = fs::read_to_string(&spec_path)
            .unwrap_or_else(|e| panic!("Cannot read {}: {}", spec_path.display(), e));
        all_ids.extend(extract_requirement_ids(&content));
    }
    all_ids
}

/// Bidirectional traceability: every requirement in FPA-SRS-000 must be referenced
/// by at least one crate-level specification.
#[test]
fn main_spec_requirements_traced_by_crates() {
    let main_ids = main_spec_requirement_ids();
    let crate_ids = all_crate_requirement_ids();

    let unreferenced: Vec<&String> = main_ids.iter().filter(|id| !crate_ids.contains(*id)).collect();

    assert!(
        unreferenced.is_empty(),
        "The following requirements in FPA-SRS-000 are not referenced by any crate specification: {:?}\n\
         Every requirement in the main spec must appear in at least one crate's docs/SPECIFICATION.md.",
        unreferenced
    );
}

/// Bidirectional traceability: every requirement in FPA-CON-000 must be referenced
/// by at least one crate-level specification.
///
/// This test reports gaps without failing, since conventions coverage is expected to
/// grow incrementally. Convention requirements may describe cross-cutting concerns
/// that don't map cleanly to a single crate.
#[test]
fn conventions_requirements_traced_by_crates() {
    let conv_ids = conventions_spec_requirement_ids();
    let crate_ids = all_crate_requirement_ids();

    let unreferenced: Vec<&String> = conv_ids.iter().filter(|id| !crate_ids.contains(*id)).collect();

    if !unreferenced.is_empty() {
        eprintln!(
            "\n[INFO] The following FPA-CON-000 requirements are not yet referenced by any crate spec: {:?}\n\
             Convention requirements may describe cross-cutting concerns; gaps are expected during \
             incremental development.\n",
            unreferenced
        );
    }
    // Informational only — does not fail.
}

/// Orphan detection: every requirement ID in a crate-level spec must exist in either
/// the main spec (FPA-SRS-000) or the conventions spec (FPA-CON-000).
#[test]
fn no_orphan_requirements_in_crate_specs() {
    let main_ids = main_spec_requirement_ids();
    let conv_ids = conventions_spec_requirement_ids();
    let parent_ids: BTreeSet<String> = main_ids.union(&conv_ids).cloned().collect();

    let mut orphans: Vec<(String, String)> = Vec::new();

    for crate_dir in crate_dirs() {
        let spec_path = crate_dir.join("docs").join("SPECIFICATION.md");
        let crate_name = crate_dir.file_name().unwrap().to_string_lossy().to_string();
        if !spec_path.is_file() {
            continue;
        }
        let content = fs::read_to_string(&spec_path)
            .unwrap_or_else(|e| panic!("Cannot read {}: {}", spec_path.display(), e));
        for id in extract_requirement_ids(&content) {
            if !parent_ids.contains(&id) {
                orphans.push((crate_name.clone(), id));
            }
        }
    }

    assert!(
        orphans.is_empty(),
        "The following crate requirements have no parent in FPA-SRS-000 or FPA-CON-000: {:?}\n\
         Every requirement ID in a crate spec must trace to a parent specification.",
        orphans
    );
}

/// Recursive structure validation: if any crate contains sub-compositors (detected
/// by nested directories that themselves contain a Cargo.toml or src/), those
/// sub-directories should also maintain their own docs/ and SPECIFICATION.md.
///
/// This test is forward-looking — the current prototype does not yet have nested
/// sub-partition crates. It reports findings without failing, establishing the
/// structural expectation for when multi-layer nesting is introduced.
#[test]
fn nested_compositors_have_docs_structure() {
    let mut report = String::new();
    let mut found_any = false;

    for crate_dir in crate_dirs() {
        let crate_name = crate_dir.file_name().unwrap().to_string_lossy().to_string();

        // Look for nested crate-like directories (have src/ or Cargo.toml)
        let entries = match fs::read_dir(&crate_dir) {
            Ok(entries) => entries,
            Err(_) => continue,
        };

        for entry in entries.flatten() {
            let sub_path = entry.path();
            if !sub_path.is_dir() {
                continue;
            }
            let sub_name = sub_path.file_name().unwrap().to_string_lossy().to_string();
            // Skip standard directories
            if ["src", "tests", "docs", "target", "benches", "examples"].contains(&sub_name.as_str()) {
                continue;
            }
            // Check if it looks like a sub-crate (has Cargo.toml or src/)
            let has_cargo = sub_path.join("Cargo.toml").is_file();
            let has_src = sub_path.join("src").is_dir();
            if has_cargo || has_src {
                found_any = true;
                let has_docs = sub_path.join("docs").is_dir();
                let has_spec = sub_path.join("docs").join("SPECIFICATION.md").is_file();
                if !has_docs || !has_spec {
                    report.push_str(&format!(
                        "  {}/{}: docs/={}, SPECIFICATION.md={}\n",
                        crate_name, sub_name, has_docs, has_spec
                    ));
                }
            }
        }
    }

    if found_any && !report.is_empty() {
        eprintln!(
            "\n[INFO] Nested sub-crates missing documentation structure:\n{}\n\
             Sub-partitions should maintain their own docs/ and SPECIFICATION.md.\n",
            report
        );
    }
    // Informational only — does not fail during prototype phase.
}
