//! Where a plugin's settings apply, and how scopes combine.

use serde_json::{Map, Value};

/// Where values apply.
#[derive(Debug, Clone, Copy, PartialEq, Eq, Hash)]
pub enum Scope {
    Server,
    Machine,
    Project,
}

impl Scope {
    pub fn as_str(self) -> &'static str {
        match self {
            Self::Server => "server",
            Self::Machine => "machine",
            Self::Project => "project",
        }
    }

    pub fn parse(text: &str) -> Option<Self> {
        match text {
            "server" => Some(Self::Server),
            "machine" => Some(Self::Machine),
            "project" => Some(Self::Project),
            _ => None,
        }
    }
}

/// The defaults a settings schema gives its fields.
pub fn defaults(schema: Option<&Value>) -> Map<String, Value> {
    schema
        .and_then(|schema| schema["properties"].as_object())
        .map(|properties| {
            properties
                .iter()
                .filter_map(|(name, field)| {
                    field
                        .get("default")
                        .map(|value| (name.clone(), value.clone()))
                })
                .collect()
        })
        .unwrap_or_default()
}

/// The values in effect: the defaults, then each scope's values over them,
/// widest first (a field set in a narrower scope wins).
pub fn effective(schema: Option<&Value>, layers: &[Value]) -> Value {
    let mut values = defaults(schema);
    for layer in layers {
        if let Some(fields) = layer.as_object() {
            for (name, value) in fields {
                values.insert(name.clone(), value.clone());
            }
        }
    }
    Value::Object(values)
}

/// What is wrong with the values one scope sets, against the plugin's
/// settings schema: each problem with the field it is about, such as
/// `pollSeconds` or `contexts.prod.protection`. A scope sets only some
/// fields, so none is required; `null` unsets a field. The keywords zod
/// writes are checked (types, `enum`, `const`, bounds, lengths, nested
/// objects and lists, `anyOf`); others are not.
pub fn problems(schema: Option<&Value>, values: &Map<String, Value>) -> Vec<(String, String)> {
    let mut found = Vec::new();
    let Some(schema) = schema else {
        return found;
    };
    let properties = schema["properties"].as_object();
    for (name, value) in values {
        if value.is_null() {
            continue;
        }
        match properties.and_then(|properties| properties.get(name)) {
            Some(field) => check(field, value, name, &mut found),
            None => found.push((
                name.clone(),
                "is not one of this plugin's settings".to_owned(),
            )),
        }
    }
    found
}

fn check(schema: &Value, value: &Value, at: &str, found: &mut Vec<(String, String)>) {
    let Some(schema) = schema.as_object() else {
        return;
    };
    let mut fail = |message: String| found.push((at.to_owned(), message));
    if let Some(options) = schema.get("anyOf").and_then(Value::as_array) {
        let fits = options.iter().any(|option| {
            let mut inner = Vec::new();
            check(option, value, at, &mut inner);
            inner.is_empty()
        });
        if !fits {
            fail("does not match any of the forms it may take".to_owned());
        }
        return;
    }
    if let Some(types) = schema.get("type") {
        let names: Vec<&str> = match types {
            Value::String(name) => vec![name.as_str()],
            Value::Array(names) => names.iter().filter_map(Value::as_str).collect(),
            _ => Vec::new(),
        };
        if !names.is_empty() && !names.iter().any(|name| is_type(value, name)) {
            fail(format!("must be {}", names.join(" or ")));
            return;
        }
    }
    if let Some(allowed) = schema.get("enum").and_then(Value::as_array)
        && !allowed.contains(value)
    {
        let listed: Vec<String> = allowed.iter().map(Value::to_string).collect();
        fail(format!("must be one of {}", listed.join(", ")));
    }
    if let Some(constant) = schema.get("const")
        && constant != value
    {
        fail(format!("must be {constant}"));
    }
    if let Some(number) = value.as_f64() {
        let bound = |key: &str| schema.get(key).and_then(Value::as_f64);
        if let Some(minimum) = bound("minimum").filter(|minimum| number < *minimum) {
            fail(format!("must be at least {minimum}"));
        }
        if let Some(maximum) = bound("maximum").filter(|maximum| number > *maximum) {
            fail(format!("must be at most {maximum}"));
        }
        if let Some(minimum) = bound("exclusiveMinimum").filter(|minimum| number <= *minimum) {
            fail(format!("must be more than {minimum}"));
        }
        if let Some(maximum) = bound("exclusiveMaximum").filter(|maximum| number >= *maximum) {
            fail(format!("must be less than {maximum}"));
        }
    }
    if let Some(text) = value.as_str() {
        let length = text.chars().count() as u64;
        let bound = |key: &str| schema.get(key).and_then(Value::as_u64);
        if let Some(minimum) = bound("minLength").filter(|minimum| length < *minimum) {
            fail(format!("must be at least {minimum} characters"));
        }
        if let Some(maximum) = bound("maxLength").filter(|maximum| length > *maximum) {
            fail(format!("must be at most {maximum} characters"));
        }
    }
    if let Some(items) = value.as_array() {
        let bound = |key: &str| schema.get(key).and_then(Value::as_u64);
        let count = items.len() as u64;
        if let Some(minimum) = bound("minItems").filter(|minimum| count < *minimum) {
            fail(format!("must have at least {minimum} items"));
        }
        if let Some(maximum) = bound("maxItems").filter(|maximum| count > *maximum) {
            fail(format!("must have at most {maximum} items"));
        }
        if let Some(each) = schema.get("items") {
            for (index, item) in items.iter().enumerate() {
                check(each, item, &format!("{at}.{index}"), found);
            }
        }
    }
    if let Some(fields) = value.as_object() {
        let properties = schema.get("properties").and_then(Value::as_object);
        if let Some(required) = schema.get("required").and_then(Value::as_array) {
            for name in required.iter().filter_map(Value::as_str) {
                if !fields.contains_key(name) {
                    found.push((format!("{at}.{name}"), "is required".to_owned()));
                }
            }
        }
        for (name, field) in fields {
            let path = format!("{at}.{name}");
            match properties.and_then(|properties| properties.get(name)) {
                Some(property) => check(property, field, &path, found),
                None => match schema.get("additionalProperties") {
                    Some(Value::Bool(false)) => {
                        found.push((path, "is not expected here".to_owned()));
                    }
                    Some(extra @ Value::Object(_)) => check(extra, field, &path, found),
                    _ => {}
                },
            }
        }
    }
}

fn is_type(value: &Value, name: &str) -> bool {
    match name {
        "string" => value.is_string(),
        "number" => value.is_number(),
        "integer" => {
            value.is_i64() || value.is_u64() || value.as_f64().is_some_and(|n| n.fract() == 0.0)
        }
        "boolean" => value.is_boolean(),
        "object" => value.is_object(),
        "array" => value.is_array(),
        "null" => value.is_null(),
        _ => true,
    }
}

#[cfg(test)]
mod tests {
    use serde_json::json;

    use super::{effective, problems};

    #[test]
    fn values_are_checked_against_the_schema() {
        let schema = json!({
            "type": "object",
            "properties": {
                "pollSeconds": { "type": "integer", "minimum": 1, "maximum": 60, "default": 3 },
                "command": { "type": "string", "minLength": 1 },
                "contexts": {
                    "type": "object",
                    "additionalProperties": {
                        "type": "object",
                        "properties": {
                            "protection": { "type": "string", "enum": ["normal", "readonly"] }
                        }
                    }
                }
            }
        });
        let check = |values: serde_json::Value| {
            problems(Some(&schema), values.as_object().unwrap())
                .into_iter()
                .map(|(field, _)| field)
                .collect::<Vec<_>>()
        };
        assert!(
            check(
                json!({ "pollSeconds": 5, "contexts": { "prod": { "protection": "readonly" } } })
            )
            .is_empty()
        );
        assert!(check(json!({ "pollSeconds": null })).is_empty());
        assert_eq!(check(json!({ "pollSeconds": 0 })), ["pollSeconds"]);
        assert_eq!(check(json!({ "pollSeconds": 2.5 })), ["pollSeconds"]);
        assert_eq!(check(json!({ "pollSeconds": "3" })), ["pollSeconds"]);
        assert_eq!(check(json!({ "command": "" })), ["command"]);
        assert_eq!(check(json!({ "colour": "red" })), ["colour"]);
        assert_eq!(
            check(json!({ "contexts": { "prod": { "protection": "locked" } } })),
            ["contexts.prod.protection"]
        );
        assert!(problems(None, json!({ "any": 1 }).as_object().unwrap()).is_empty());
    }

    #[test]
    fn narrower_scopes_win_over_wider_ones_and_defaults() {
        let schema = json!({
            "type": "object",
            "properties": { "greeting": { "default": "hello" }, "loud": { "default": false } }
        });
        let server = json!({ "greeting": "hi" });
        let project = json!({ "loud": true });
        assert_eq!(
            effective(Some(&schema), &[server, json!({}), project]),
            json!({ "greeting": "hi", "loud": true })
        );
        assert_eq!(effective(None, &[]), json!({}));
    }
}
