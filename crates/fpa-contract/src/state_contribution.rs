//! Uniform envelope for partition state contributions (FPA-009).
//!
//! Both lock-step and supervisory compositors wrap each partition's
//! `contribute_state()` output in this type, ensuring the outer layer
//! sees the same format regardless of execution strategy.

/// Uniform envelope for partition state contributions (FPA-009).
///
/// Both lock-step and supervisory compositors wrap each partition's
/// `contribute_state()` output in this type, ensuring the outer layer
/// sees the same format regardless of execution strategy.
#[derive(Debug, Clone)]
pub struct StateContribution {
    /// The partition's actual state.
    pub state: toml::Value,
    /// Whether this state was computed for the current invocation.
    /// Lock-step compositors always set this to `true`.
    /// Supervisory compositors derive this from heartbeat checks.
    pub fresh: bool,
    /// Age of the data in milliseconds.
    /// 0 for synchronously computed state (lock-step).
    pub age_ms: u64,
}

impl StateContribution {
    /// Serialize to a TOML value.
    ///
    /// Produces a table with keys `state`, `fresh`, and `age_ms`.
    pub fn to_toml(&self) -> toml::Value {
        let mut table = toml::map::Map::new();
        table.insert("state".to_string(), self.state.clone());
        table.insert("fresh".to_string(), toml::Value::Boolean(self.fresh));
        let age_i64 = i64::try_from(self.age_ms)
            .expect("age_ms exceeds i64::MAX — state contribution cannot be serialized");
        table.insert(
            "age_ms".to_string(),
            toml::Value::Integer(age_i64),
        );
        toml::Value::Table(table)
    }

    /// Deserialize from a TOML value.
    ///
    /// Expects a table with keys `state`, `fresh`, and `age_ms`.
    /// Returns `None` if the value is not a valid StateContribution envelope.
    pub fn from_toml(value: &toml::Value) -> Option<Self> {
        let table = value.as_table()?;
        let state = table.get("state")?.clone();
        let fresh = table.get("fresh")?.as_bool()?;
        let age_ms = table.get("age_ms")?.as_integer()?;
        if age_ms < 0 {
            return None;
        }
        Some(Self {
            state,
            fresh,
            age_ms: age_ms as u64,
        })
    }
}
