//! Pure helper functions for extracting typed parameters from a `serde_json::Value` object.
//!
//! Each helper takes a JSON value, a key name, and a default. If the key is
//! missing or the value is not the expected type, the default is returned.
//! These never fail — they always produce a usable value.

use serde_json::Value;

/// Extracts an `f64` from `params[name]`, returning `default` if missing or wrong type.
///
/// Accepts both JSON numbers (including integers) and converts them to f64.
pub fn param_f64(params: &Value, name: &str, default: f64) -> f64 {
    params.get(name).and_then(Value::as_f64).unwrap_or(default)
}

/// Extracts a `usize` from `params[name]`, returning `default` if missing or wrong type.
///
/// Only succeeds if the JSON value is a non-negative integer that fits in `u64`,
/// then converts to `usize`.
pub fn param_usize(params: &Value, name: &str, default: usize) -> usize {
    params
        .get(name)
        .and_then(Value::as_u64)
        .map(|v| v as usize)
        .unwrap_or(default)
}

/// Extracts a `bool` from `params[name]`, returning `default` if missing or wrong type.
pub fn param_bool(params: &Value, name: &str, default: bool) -> bool {
    params.get(name).and_then(Value::as_bool).unwrap_or(default)
}

/// Extracts a `String` from `params[name]`, returning `default` if missing or wrong type.
pub fn param_string(params: &Value, name: &str, default: &str) -> String {
    params
        .get(name)
        .and_then(Value::as_str)
        .map(String::from)
        .unwrap_or_else(|| default.to_owned())
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    // -- param_f64 --

    #[test]
    fn param_f64_extracts_existing_float() {
        let params = json!({"speed": 2.5});
        assert!((param_f64(&params, "speed", 1.0) - 2.5).abs() < f64::EPSILON);
    }

    #[test]
    fn param_f64_extracts_integer_as_float() {
        let params = json!({"count": 10});
        assert!((param_f64(&params, "count", 0.0) - 10.0).abs() < f64::EPSILON);
    }

    #[test]
    fn param_f64_returns_default_when_key_missing() {
        let params = json!({"other": 1.0});
        assert!((param_f64(&params, "speed", 3.0) - 3.0).abs() < f64::EPSILON);
    }

    #[test]
    fn param_f64_returns_default_when_wrong_type() {
        let params = json!({"speed": "fast"});
        assert!((param_f64(&params, "speed", 1.0) - 1.0).abs() < f64::EPSILON);
    }

    #[test]
    fn param_f64_returns_default_for_null_value() {
        let params = json!({"speed": null});
        assert!((param_f64(&params, "speed", 5.0) - 5.0).abs() < f64::EPSILON);
    }

    #[test]
    fn param_f64_returns_default_for_non_object() {
        let params = json!("not an object");
        assert!((param_f64(&params, "speed", 7.0) - 7.0).abs() < f64::EPSILON);
    }

    // -- param_usize --

    #[test]
    fn param_usize_extracts_existing_integer() {
        let params = json!({"count": 42});
        assert_eq!(param_usize(&params, "count", 0), 42);
    }

    #[test]
    fn param_usize_returns_default_when_key_missing() {
        let params = json!({});
        assert_eq!(param_usize(&params, "count", 10), 10);
    }

    #[test]
    fn param_usize_returns_default_for_float_value() {
        // 2.5 is not a valid u64, so should fall back to default
        let params = json!({"count": 2.5});
        assert_eq!(param_usize(&params, "count", 99), 99);
    }

    #[test]
    fn param_usize_returns_default_for_negative_integer() {
        let params = json!({"count": -1});
        assert_eq!(param_usize(&params, "count", 5), 5);
    }

    #[test]
    fn param_usize_returns_default_for_string_value() {
        let params = json!({"count": "many"});
        assert_eq!(param_usize(&params, "count", 8), 8);
    }

    // -- param_bool --

    #[test]
    fn param_bool_extracts_true() {
        let params = json!({"enabled": true});
        assert!(param_bool(&params, "enabled", false));
    }

    #[test]
    fn param_bool_extracts_false() {
        let params = json!({"enabled": false});
        assert!(!param_bool(&params, "enabled", true));
    }

    #[test]
    fn param_bool_returns_default_when_key_missing() {
        let params = json!({});
        assert!(param_bool(&params, "enabled", true));
    }

    #[test]
    fn param_bool_returns_default_for_wrong_type() {
        let params = json!({"enabled": 1});
        assert!(!param_bool(&params, "enabled", false));
    }

    // -- param_string --

    #[test]
    fn param_string_extracts_existing_string() {
        let params = json!({"name": "ocean"});
        assert_eq!(param_string(&params, "name", "default"), "ocean");
    }

    #[test]
    fn param_string_returns_default_when_key_missing() {
        let params = json!({});
        assert_eq!(param_string(&params, "name", "earth"), "earth");
    }

    #[test]
    fn param_string_returns_default_for_wrong_type() {
        let params = json!({"name": 42});
        assert_eq!(param_string(&params, "name", "fallback"), "fallback");
    }

    #[test]
    fn param_string_handles_empty_string_value() {
        let params = json!({"name": ""});
        assert_eq!(param_string(&params, "name", "default"), "");
    }
}
