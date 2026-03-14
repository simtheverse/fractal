use std::collections::HashMap;

use crate::error::ConfigError;
use crate::fragment::CompositionFragment;
use crate::loader::resolve_extends;
use crate::merge::deep_merge;

/// A registry of named composition fragments.
pub struct FragmentRegistry {
    fragments: HashMap<String, CompositionFragment>,
}

impl FragmentRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self {
            fragments: HashMap::new(),
        }
    }

    /// Register a fragment under the given name.
    pub fn register(&mut self, name: &str, fragment: CompositionFragment) {
        self.fragments.insert(name.to_string(), fragment);
    }

    /// Look up a fragment by name.
    pub fn resolve(&self, name: &str) -> Option<&CompositionFragment> {
        self.fragments.get(name)
    }

    /// Resolve a named fragment and apply overrides on top of it.
    ///
    /// The named fragment's `extends` chain is resolved first using the
    /// registry as the loader, then `overrides` are deep-merged on top.
    pub fn resolve_with_overrides(
        &self,
        name: &str,
        overrides: &CompositionFragment,
    ) -> Result<CompositionFragment, ConfigError> {
        let base = self
            .fragments
            .get(name)
            .ok_or_else(|| ConfigError::UnknownFragment(name.to_string()))?
            .clone();

        // Resolve extends chain using registry as the loader
        let resolved = resolve_extends(base, |n| {
            self.fragments
                .get(n)
                .cloned()
                .ok_or_else(|| ConfigError::UnknownFragment(n.to_string()))
        })?;

        // Deep-merge overrides on top
        let base_value = toml::Value::try_from(&resolved)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;
        let overlay_value = toml::Value::try_from(overrides)
            .map_err(|e| ConfigError::ParseError(e.to_string()))?;

        let merged = deep_merge(base_value, overlay_value);

        let mut result: CompositionFragment = merged
            .try_into()
            .map_err(|e: toml::de::Error| ConfigError::ParseError(e.to_string()))?;

        // The extends chain is already resolved; clear any extends key that
        // may have been introduced by the overrides merge.
        result.extends = None;

        Ok(result)
    }
}

impl Default for FragmentRegistry {
    fn default() -> Self {
        Self::new()
    }
}
