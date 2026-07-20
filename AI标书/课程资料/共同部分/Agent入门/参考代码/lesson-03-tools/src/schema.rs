use serde::{Deserialize, Serialize};
use serde_json::Value;
use std::collections::HashSet;

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct FieldSpec {
    pub name: String,
    pub schema: Schema,
    pub required: bool,
}

impl FieldSpec {
    pub fn required(name: impl Into<String>, schema: Schema) -> Self {
        Self {
            name: name.into(),
            schema,
            required: true,
        }
    }

    pub fn optional(name: impl Into<String>, schema: Schema) -> Self {
        Self {
            name: name.into(),
            schema,
            required: false,
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Schema {
    Any,
    String {
        min_len: usize,
        max_len: usize,
    },
    Integer,
    Boolean,
    Array {
        items: Box<Schema>,
        max_items: usize,
    },
    Object {
        fields: Vec<FieldSpec>,
        allow_unknown: bool,
    },
}

impl Schema {
    pub fn non_empty_string(max_len: usize) -> Self {
        Self::String {
            min_len: 1,
            max_len,
        }
    }

    pub fn object(fields: Vec<FieldSpec>) -> Self {
        Self::Object {
            fields,
            allow_unknown: false,
        }
    }

    pub fn array(items: Schema, max_items: usize) -> Self {
        Self::Array {
            items: Box::new(items),
            max_items,
        }
    }

    pub fn validate(&self, value: &Value) -> Result<(), String> {
        self.validate_at(value, "$")
    }

    fn validate_at(&self, value: &Value, path: &str) -> Result<(), String> {
        match self {
            Schema::Any => Ok(()),
            Schema::String { min_len, max_len } => {
                let text = value
                    .as_str()
                    .ok_or_else(|| format!("{path} 必须是字符串"))?;
                let length = text.chars().count();
                if length < *min_len {
                    return Err(format!("{path} 长度不能小于 {min_len}"));
                }
                if length > *max_len {
                    return Err(format!("{path} 长度不能超过 {max_len}"));
                }
                Ok(())
            }
            Schema::Integer => value
                .as_i64()
                .map(|_| ())
                .ok_or_else(|| format!("{path} 必须是整数")),
            Schema::Boolean => value
                .as_bool()
                .map(|_| ())
                .ok_or_else(|| format!("{path} 必须是布尔值")),
            Schema::Array { items, max_items } => {
                let values = value
                    .as_array()
                    .ok_or_else(|| format!("{path} 必须是数组"))?;
                if values.len() > *max_items {
                    return Err(format!("{path} 最多包含 {max_items} 项"));
                }
                for (index, item) in values.iter().enumerate() {
                    items.validate_at(item, &format!("{path}[{index}]"))?;
                }
                Ok(())
            }
            Schema::Object {
                fields,
                allow_unknown,
            } => {
                let object = value
                    .as_object()
                    .ok_or_else(|| format!("{path} 必须是对象"))?;
                for field in fields {
                    match object.get(&field.name) {
                        Some(field_value) => field
                            .schema
                            .validate_at(field_value, &format!("{path}.{}", field.name))?,
                        None if field.required => {
                            return Err(format!("{path} 缺少必填字段 {}", field.name));
                        }
                        None => {}
                    }
                }
                if !allow_unknown {
                    let known: HashSet<_> =
                        fields.iter().map(|field| field.name.as_str()).collect();
                    if let Some(unknown) = object.keys().find(|name| !known.contains(name.as_str()))
                    {
                        return Err(format!("{path} 包含未知字段 {unknown}"));
                    }
                }
                Ok(())
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use serde_json::json;

    #[test]
    fn strict_object_rejects_unknown_fields() {
        let schema = Schema::object(vec![FieldSpec::required(
            "text",
            Schema::non_empty_string(8),
        )]);
        assert!(schema.validate(&json!({"text":"ok"})).is_ok());
        assert!(schema
            .validate(&json!({"text":"ok","surprise":true}))
            .is_err());
    }
}
