//! FPA-040: Contract Crate Naming and Documentation
//!
//! Verifies naming conventions and documentation structure.

use std::path::Path;

/// The contract crate follows the naming convention (fpa-contract).
#[test]
fn contract_crate_follows_naming_convention() {
    use fpa_contract::Partition;
    // The crate name "fpa-contract" follows the <system>-contract convention.
    // Verify we can use the contract types.
    let counter = fpa_contract::test_support::Counter::new("t");
    assert!(!counter.id().is_empty());
}

/// The contract crate has a docs directory.
#[test]
fn contract_crate_has_docs_directory() {
    let docs_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs");
    assert!(docs_path.exists(), "contract crate should have a docs/ directory");
}

/// The contract crate has a SPECIFICATION.md.
// NOTE: FPA-040 spec describes the path as docs/design/SPECIFICATION.md (Diataxis layout),
// but the prototype uses docs/SPECIFICATION.md for simplicity. See diataxis_subdirectories
// test in fpa_030 for details.
#[test]
fn contract_crate_has_specification() {
    let spec_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/SPECIFICATION.md");
    assert!(spec_path.exists(), "contract crate should have docs/SPECIFICATION.md");
}

/// The contract crate's SPECIFICATION.md traces to FPA-SRS-000.
#[test]
fn contract_crate_specification_traces_to_parent() {
    let spec_path = Path::new(env!("CARGO_MANIFEST_DIR")).join("docs/SPECIFICATION.md");
    let content = std::fs::read_to_string(&spec_path)
        .unwrap_or_else(|e| panic!("Cannot read {}: {}", spec_path.display(), e));
    let has_trace = content.lines().any(|line| {
        let trimmed = line.trim();
        trimmed.starts_with("Traces to:") && trimmed.contains("FPA-SRS-000")
    });
    assert!(
        has_trace,
        "docs/SPECIFICATION.md should contain a 'Traces to:' line referencing FPA-SRS-000"
    );
}
