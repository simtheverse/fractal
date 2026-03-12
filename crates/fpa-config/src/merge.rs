use toml::Value;

/// Deep-merge two TOML values. `overlay` takes precedence over `base`.
///
/// - Tables are merged recursively: overlay keys override base keys,
///   base-only keys are preserved.
/// - Non-table values: overlay replaces base.
/// - Arrays: overlay replaces base (not concatenated).
pub fn deep_merge(base: Value, overlay: Value) -> Value {
    match (base, overlay) {
        (Value::Table(mut base_table), Value::Table(overlay_table)) => {
            for (key, overlay_val) in overlay_table {
                let merged = if let Some(base_val) = base_table.remove(&key) {
                    deep_merge(base_val, overlay_val)
                } else {
                    overlay_val
                };
                base_table.insert(key, merged);
            }
            Value::Table(base_table)
        }
        // For all non-table cases, overlay wins
        (_, overlay) => overlay,
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use toml::toml;

    #[test]
    fn overlay_replaces_scalar() {
        let base = Value::Integer(1);
        let overlay = Value::Integer(2);
        assert_eq!(deep_merge(base, overlay), Value::Integer(2));
    }

    #[test]
    fn tables_merge_recursively() {
        let base = Value::Table(toml! {
            a = 1
            b = 2
        });
        let overlay = Value::Table(toml! {
            b = 3
            c = 4
        });
        let result = deep_merge(base, overlay);
        let table = result.as_table().unwrap();
        assert_eq!(table["a"].as_integer(), Some(1));
        assert_eq!(table["b"].as_integer(), Some(3));
        assert_eq!(table["c"].as_integer(), Some(4));
    }

    #[test]
    fn arrays_replaced_not_concatenated() {
        let base = Value::Array(vec![Value::Integer(1), Value::Integer(2)]);
        let overlay = Value::Array(vec![Value::Integer(3)]);
        let result = deep_merge(base, overlay);
        let arr = result.as_array().unwrap();
        assert_eq!(arr.len(), 1);
        assert_eq!(arr[0].as_integer(), Some(3));
    }
}
