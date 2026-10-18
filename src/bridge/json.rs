use serde_json::{Map, Value};

pub fn text(source: Option<&Value>, name: &str, fallback: &str) -> String {
    match source.and_then(|value| value.get(name)) {
        Some(Value::String(value)) => value.clone(),
        _ => fallback.to_string(),
    }
}

pub fn number(source: Option<&Value>, name: &str, fallback: i64) -> i64 {
    match source.and_then(|value| value.get(name)) {
        Some(Value::Number(value)) => value.as_i64().unwrap_or(fallback),
        Some(Value::String(value)) => value.parse::<i64>().unwrap_or(fallback),
        _ => fallback,
    }
}

pub fn text_array(
    source: Option<&Value>,
    name: &str,
    maximum_count: usize,
) -> Result<Vec<String>, String> {
    let Some(Value::Array(items)) = source.and_then(|value| value.get(name)) else {
        return Ok(Vec::new());
    };
    if items.len() > maximum_count {
        return Err("Array exceeds the allowed item count.".to_string());
    }
    let mut result = Vec::with_capacity(items.len());
    for item in items {
        match item {
            Value::String(value) => result.push(value.clone()),
            _ => return Err("Array contains a non-string value.".to_string()),
        }
    }
    Ok(result)
}

pub fn payload_of(root: &Value) -> Option<&Value> {
    root.get("payload")
}

pub fn object(entries: Vec<(&str, Value)>) -> Value {
    let mut map = Map::new();
    for (key, value) in entries {
        map.insert(key.to_string(), value);
    }
    Value::Object(map)
}
