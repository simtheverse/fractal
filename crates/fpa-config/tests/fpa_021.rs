//! FPA-021: Named Fragments — registry and override resolution.

use fpa_config::{load_from_str, ConfigError, FragmentRegistry};

#[test]
fn register_and_resolve_by_name() {
    let toml_str = r#"
[partitions.physics]
implementation = "default_physics"
"#;
    let fragment = load_from_str(toml_str).unwrap();

    let mut registry = FragmentRegistry::new();
    registry.register("my_preset", fragment);

    let resolved = registry.resolve("my_preset").expect("should find fragment");
    assert_eq!(
        resolved.partitions["physics"].implementation.as_deref(),
        Some("default_physics")
    );
}

#[test]
fn resolve_with_overrides_merges() {
    let base_toml = r#"
[partitions.physics]
implementation = "default_physics"

[partitions.renderer]
implementation = "default_renderer"
"#;
    let override_toml = r#"
[partitions.physics]
implementation = "custom_physics"
"#;

    let base = load_from_str(base_toml).unwrap();
    let overrides = load_from_str(override_toml).unwrap();

    let mut registry = FragmentRegistry::new();
    registry.register("preset", base);

    let result = registry
        .resolve_with_overrides("preset", &overrides)
        .unwrap();

    // Override applied
    assert_eq!(
        result.partitions["physics"].implementation.as_deref(),
        Some("custom_physics")
    );
    // Base preserved
    assert_eq!(
        result.partitions["renderer"].implementation.as_deref(),
        Some("default_renderer")
    );
}

#[test]
fn unknown_fragment_name_errors() {
    let registry = FragmentRegistry::new();
    let dummy = load_from_str("[system]\nx = 1").unwrap();

    let result = registry.resolve_with_overrides("nonexistent", &dummy);
    assert!(matches!(result, Err(ConfigError::UnknownFragment(_))));
}

#[test]
fn named_fragment_any_scope() {
    // A fragment can represent system-level config
    let system_toml = r#"
[system]
timestep = 0.016667
transport = "InProcess"
"#;
    let system_fragment = load_from_str(system_toml).unwrap();

    // Or partition-level config
    let partition_toml = r#"
[partitions.physics]
implementation = "physics_impl"
gravity = -9.81
"#;
    let partition_fragment = load_from_str(partition_toml).unwrap();

    let mut registry = FragmentRegistry::new();
    registry.register("system_preset", system_fragment);
    registry.register("partition_preset", partition_fragment);

    // Both are usable
    let sys = registry.resolve("system_preset").unwrap();
    assert!(sys.system.contains_key("timestep"));

    let part = registry.resolve("partition_preset").unwrap();
    assert!(part.partitions.contains_key("physics"));
}
