//! FPA-019: Composition Fragments — parsing TOML into CompositionFragment.

use fpa_config::load_from_str;

const FRAGMENT_TOML: &str = r#"
[system]
timestep = 0.016667
transport = "InProcess"

[partitions.physics]
implementation = "default_physics"

[partitions.renderer]
implementation = "opengl_renderer"
"#;

#[test]
fn parse_toml_fragment() {
    let fragment = load_from_str(FRAGMENT_TOML).expect("should parse");
    assert!(fragment.extends.is_none());
    assert_eq!(fragment.partitions.len(), 2);
}

#[test]
fn partition_selections_accessible() {
    let fragment = load_from_str(FRAGMENT_TOML).expect("should parse");
    let physics = fragment.partitions.get("physics").expect("physics partition");
    assert_eq!(
        physics.implementation.as_deref(),
        Some("default_physics")
    );
    let renderer = fragment.partitions.get("renderer").expect("renderer partition");
    assert_eq!(
        renderer.implementation.as_deref(),
        Some("opengl_renderer")
    );
}

#[test]
fn system_parameters_accessible() {
    let fragment = load_from_str(FRAGMENT_TOML).expect("should parse");
    let timestep = fragment.system.get("timestep").expect("timestep");
    assert!((timestep.as_float().unwrap() - 0.016667).abs() < 1e-6);
    let transport = fragment.system.get("transport").expect("transport");
    assert_eq!(transport.as_str(), Some("InProcess"));
}

#[test]
fn fragment_with_no_partitions_is_valid() {
    let toml_str = r#"
[system]
timestep = 0.033
"#;
    let fragment = load_from_str(toml_str).expect("should parse");
    assert!(fragment.partitions.is_empty());
}

#[test]
fn fragment_round_trip_serialization() {
    let original = load_from_str(FRAGMENT_TOML).expect("should parse");
    let serialized = toml::to_string(&original).expect("should serialize back to TOML");
    let reparsed = load_from_str(&serialized).expect("should parse serialized TOML");

    // Same partitions
    assert_eq!(original.partitions.len(), reparsed.partitions.len());
    for (name, orig_part) in &original.partitions {
        let re_part = reparsed.partitions.get(name).unwrap_or_else(|| {
            panic!("reparsed fragment missing partition '{}'", name)
        });
        assert_eq!(orig_part.implementation, re_part.implementation);
    }

    // Same system params
    assert_eq!(original.system.len(), reparsed.system.len());
    for (key, orig_val) in &original.system {
        let re_val = reparsed.system.get(key).unwrap_or_else(|| {
            panic!("reparsed fragment missing system key '{}'", key)
        });
        assert_eq!(orig_val, re_val);
    }
}
