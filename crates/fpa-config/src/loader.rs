use crate::error::ConfigError;
use crate::fragment::CompositionFragment;
use crate::merge::deep_merge;
use std::collections::HashSet;

/// Parse a TOML string into a CompositionFragment.
pub fn load_from_str(toml_str: &str) -> Result<CompositionFragment, ConfigError> {
    toml::from_str(toml_str).map_err(|e| ConfigError::ParseError(e.to_string()))
}

/// Resolve the `extends` chain of a fragment.
///
/// The `loader` function is called to fetch base fragments by name.
/// Circular references are detected and produce a `CircularExtends` error.
pub fn resolve_extends(
    fragment: CompositionFragment,
    loader: impl Fn(&str) -> Result<CompositionFragment, ConfigError>,
) -> Result<CompositionFragment, ConfigError> {
    let mut seen = HashSet::new();
    resolve_extends_inner(fragment, &loader, &mut seen)
}

fn resolve_extends_inner(
    fragment: CompositionFragment,
    loader: &impl Fn(&str) -> Result<CompositionFragment, ConfigError>,
    seen: &mut HashSet<String>,
) -> Result<CompositionFragment, ConfigError> {
    let extends = match &fragment.extends {
        Some(name) => name.clone(),
        None => return Ok(fragment),
    };

    if !seen.insert(extends.clone()) {
        return Err(ConfigError::CircularExtends(format!(
            "circular extends detected: '{}' already visited",
            extends
        )));
    }

    let base = loader(&extends)?;
    // Recursively resolve the base's extends chain first
    let resolved_base = resolve_extends_inner(base, loader, seen)?;

    // Merge: fragment overlays on top of resolved base
    let base_value = toml::Value::try_from(&resolved_base)
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;
    let overlay_value = toml::Value::try_from(&fragment)
        .map_err(|e| ConfigError::ParseError(e.to_string()))?;

    let merged = deep_merge(base_value, overlay_value);

    // Remove the extends key from the merged result since it's been resolved
    let mut result: CompositionFragment =
        merged.try_into().map_err(|e: toml::de::Error| ConfigError::ParseError(e.to_string()))?;
    result.extends = None;

    Ok(result)
}
