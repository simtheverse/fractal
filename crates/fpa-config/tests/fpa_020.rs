//! FPA-020: Fragment Inheritance — extends chains and deep merge.

use fpa_config::{load_from_str, resolve_extends, ConfigError};

#[test]
fn child_overrides_physics_keeps_base_renderer() {
    let base_toml = r#"
[partitions.physics]
implementation = "base_physics"

[partitions.renderer]
implementation = "base_renderer"
"#;

    let child_toml = r#"
extends = "base"

[partitions.physics]
implementation = "child_physics"
"#;

    let base = load_from_str(base_toml).unwrap();
    let child = load_from_str(child_toml).unwrap();

    let resolved = resolve_extends(child, |name| {
        assert_eq!(name, "base");
        Ok(base.clone())
    })
    .unwrap();

    // Child's physics override
    assert_eq!(
        resolved.partitions["physics"].implementation.as_deref(),
        Some("child_physics")
    );
    // Base's renderer preserved
    assert_eq!(
        resolved.partitions["renderer"].implementation.as_deref(),
        Some("base_renderer")
    );
}

#[test]
fn deep_merge_preserves_nested_keys() {
    let base_toml = r#"
[system]
timestep = 0.016
transport = "InProcess"
"#;

    let child_toml = r#"
extends = "base"

[system]
timestep = 0.033
"#;

    let base = load_from_str(base_toml).unwrap();
    let child = load_from_str(child_toml).unwrap();

    let resolved = resolve_extends(child, |_| Ok(base.clone())).unwrap();

    // Child overrides timestep
    let timestep = resolved.system.get("timestep").unwrap();
    assert!((timestep.as_float().unwrap() - 0.033).abs() < 1e-6);

    // Base's transport preserved
    let transport = resolved.system.get("transport").unwrap();
    assert_eq!(transport.as_str(), Some("InProcess"));
}

#[test]
fn circular_extends_detected() {
    let a_toml = r#"
extends = "b"

[system]
x = 1
"#;
    let b_toml = r#"
extends = "a"

[system]
y = 2
"#;

    let a = load_from_str(a_toml).unwrap();
    let b = load_from_str(b_toml).unwrap();

    let result = resolve_extends(a, |name| match name {
        "b" => Ok(b.clone()),
        "a" => Ok(load_from_str(a_toml).unwrap()),
        _ => Err(ConfigError::UnknownFragment(name.to_string())),
    });

    assert!(matches!(result, Err(ConfigError::CircularExtends(_))));
}

#[test]
fn child_adds_new_partition() {
    let base_toml = r#"
[partitions.physics]
implementation = "base_physics"
"#;

    let child_toml = r#"
extends = "base"

[partitions.audio]
implementation = "child_audio"
"#;

    let base = load_from_str(base_toml).unwrap();
    let child = load_from_str(child_toml).unwrap();

    let resolved = resolve_extends(child, |_| Ok(base.clone())).unwrap();

    // Both partitions present
    assert_eq!(
        resolved.partitions["physics"].implementation.as_deref(),
        Some("base_physics")
    );
    assert_eq!(
        resolved.partitions["audio"].implementation.as_deref(),
        Some("child_audio")
    );
}

#[test]
fn multi_level_extends_chain() {
    let grandparent_toml = r#"
[system]
timestep = 0.016
transport = "InProcess"
log_level = "info"

[partitions.physics]
implementation = "gp_physics"

[partitions.renderer]
implementation = "gp_renderer"
"#;

    let parent_toml = r#"
extends = "grandparent"

[system]
timestep = 0.033

[partitions.physics]
implementation = "parent_physics"
"#;

    let child_toml = r#"
extends = "parent"

[system]
log_level = "debug"

[partitions.audio]
implementation = "child_audio"
"#;

    let grandparent = load_from_str(grandparent_toml).unwrap();
    let parent = load_from_str(parent_toml).unwrap();
    let child = load_from_str(child_toml).unwrap();

    let resolved = resolve_extends(child, |name| match name {
        "parent" => Ok(parent.clone()),
        "grandparent" => Ok(grandparent.clone()),
        _ => Err(ConfigError::UnknownFragment(name.to_string())),
    })
    .unwrap();

    // Parent overrides grandparent timestep; child does not override it
    let timestep = resolved.system.get("timestep").unwrap();
    assert!((timestep.as_float().unwrap() - 0.033).abs() < 1e-6);

    // Grandparent transport passes through both levels unmodified
    let transport = resolved.system.get("transport").unwrap();
    assert_eq!(transport.as_str(), Some("InProcess"));

    // Child overrides grandparent log_level
    let log_level = resolved.system.get("log_level").unwrap();
    assert_eq!(log_level.as_str(), Some("debug"));

    // Parent overrides grandparent physics
    assert_eq!(
        resolved.partitions["physics"].implementation.as_deref(),
        Some("parent_physics")
    );

    // Grandparent renderer passes through unmodified
    assert_eq!(
        resolved.partitions["renderer"].implementation.as_deref(),
        Some("gp_renderer")
    );

    // Child adds a new partition
    assert_eq!(
        resolved.partitions["audio"].implementation.as_deref(),
        Some("child_audio")
    );
}
