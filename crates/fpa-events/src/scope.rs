//! Action vocabulary scoping: validates that actions are used within their declared scope.

use std::collections::HashMap;

/// Maps action identifiers to their declaring scope (contract crate name).
///
/// Scopes use dot-separated paths (e.g., "system.physics.aero").
/// An action declared in scope X is usable at scope Y if Y equals X
/// or Y is a child of X (i.e., Y starts with "X.").
#[derive(Debug, Default)]
pub struct ActionRegistry {
    actions: HashMap<String, String>,
}

impl ActionRegistry {
    /// Create an empty registry.
    pub fn new() -> Self {
        Self::default()
    }

    /// Register an action as belonging to the given scope.
    pub fn register(&mut self, action_id: &str, scope: &str) {
        self.actions.insert(action_id.to_string(), scope.to_string());
    }

    /// Validate that an action is usable at the given scope.
    ///
    /// An action declared in scope D is usable at usage scope U if:
    /// - U == D, or
    /// - U starts with D followed by "." (U is a child of D)
    pub fn validate(&self, action_id: &str, usage_scope: &str) -> Result<(), String> {
        let declaring_scope = self
            .actions
            .get(action_id)
            .ok_or_else(|| format!("action '{}' not registered", action_id))?;

        if usage_scope == declaring_scope
            || usage_scope.starts_with(&format!("{}.", declaring_scope))
        {
            Ok(())
        } else {
            Err(format!(
                "action '{}' declared in scope '{}' is not usable at scope '{}'",
                action_id, declaring_scope, usage_scope
            ))
        }
    }
}
