//! JSON Schema generation utilities.
//!
//! This module provides helpers for building JSON Schema objects that
//! describe tool input parameters, following the JSON Schema specification
//! used by MCP for tool input validation.

use serde_json::Value;
use std::collections::BTreeMap;

/// A builder for constructing JSON Schema objects.
///
/// This builder produces schemas compatible with the MCP `inputSchema`
/// format expected by `tools/list`.
///
/// # Example
///
/// ```rust
/// use mcp_sdk::schema::JsonSchemaBuilder;
///
/// let schema = JsonSchemaBuilder::new()
///     .property("name", JsonSchemaBuilder::string().description("User name"))
///     .property("age", JsonSchemaBuilder::integer().description("User age"))
///     .required("name")
///     .build();
/// ```
#[derive(Debug, Clone)]
pub struct JsonSchemaBuilder {
    schema_type: Option<String>,
    description: Option<String>,
    properties: BTreeMap<String, Value>,
    required: Vec<String>,
    items: Option<Box<JsonSchemaBuilder>>,
    enum_values: Option<Vec<Value>>,
    additional: BTreeMap<String, Value>,
}

impl Default for JsonSchemaBuilder {
    fn default() -> Self {
        Self::new()
    }
}

impl JsonSchemaBuilder {
    /// Creates a new schema builder for an object type.
    #[must_use]
    pub fn new() -> Self {
        Self {
            schema_type: Some("object".to_string()),
            description: None,
            properties: BTreeMap::new(),
            required: Vec::new(),
            items: None,
            enum_values: None,
            additional: BTreeMap::new(),
        }
    }

    /// Creates a string schema.
    #[must_use]
    pub fn string() -> Self {
        Self {
            schema_type: Some("string".to_string()),
            ..Self::bare()
        }
    }

    /// Creates a number schema.
    #[must_use]
    pub fn number() -> Self {
        Self {
            schema_type: Some("number".to_string()),
            ..Self::bare()
        }
    }

    /// Creates an integer schema.
    #[must_use]
    pub fn integer() -> Self {
        Self {
            schema_type: Some("integer".to_string()),
            ..Self::bare()
        }
    }

    /// Creates a boolean schema.
    #[must_use]
    pub fn boolean() -> Self {
        Self {
            schema_type: Some("boolean".to_string()),
            ..Self::bare()
        }
    }

    /// Creates an array schema with the given item type.
    #[must_use]
    pub fn array(items: JsonSchemaBuilder) -> Self {
        Self {
            schema_type: Some("array".to_string()),
            items: Some(Box::new(items)),
            ..Self::bare()
        }
    }

    /// Creates a bare schema with no type set.
    fn bare() -> Self {
        Self {
            schema_type: None,
            description: None,
            properties: BTreeMap::new(),
            required: Vec::new(),
            items: None,
            enum_values: None,
            additional: BTreeMap::new(),
        }
    }

    /// Sets the schema description.
    #[must_use]
    pub fn description(mut self, desc: impl Into<String>) -> Self {
        self.description = Some(desc.into());
        self
    }

    /// Adds a property to the object schema.
    #[must_use]
    pub fn property(mut self, name: impl Into<String>, schema: JsonSchemaBuilder) -> Self {
        self.properties.insert(name.into(), schema.build());
        self
    }

    /// Marks a property as required.
    #[must_use]
    pub fn required(mut self, name: impl Into<String>) -> Self {
        self.required.push(name.into());
        self
    }

    /// Sets enum values for the schema.
    #[must_use]
    pub fn enum_values(mut self, values: Vec<Value>) -> Self {
        self.enum_values = Some(values);
        self
    }

    /// Sets a minimum value constraint.
    #[must_use]
    pub fn minimum(mut self, min: f64) -> Self {
        self.additional
            .insert("minimum".to_string(), Value::from(min));
        self
    }

    /// Sets a maximum value constraint.
    #[must_use]
    pub fn maximum(mut self, max: f64) -> Self {
        self.additional
            .insert("maximum".to_string(), Value::from(max));
        self
    }

    /// Sets a default value.
    #[must_use]
    pub fn default_value(mut self, default: Value) -> Self {
        self.additional.insert("default".to_string(), default);
        self
    }

    /// Sets a regex pattern for string validation.
    #[must_use]
    pub fn pattern(mut self, pattern: impl Into<String>) -> Self {
        self.additional
            .insert("pattern".to_string(), Value::from(pattern.into()));
        self
    }

    /// Builds the schema into a `serde_json::Value`.
    #[must_use]
    pub fn build(self) -> Value {
        let mut obj = serde_json::Map::new();

        if let Some(schema_type) = self.schema_type {
            obj.insert("type".to_string(), Value::from(schema_type));
        }

        if let Some(description) = self.description {
            obj.insert("description".to_string(), Value::from(description));
        }

        if !self.properties.is_empty() {
            let props: serde_json::Map<String, Value> = self.properties.into_iter().collect();
            obj.insert("properties".to_string(), Value::Object(props));
        }

        if !self.required.is_empty() {
            obj.insert(
                "required".to_string(),
                Value::Array(self.required.into_iter().map(Value::from).collect()),
            );
        }

        if let Some(items) = self.items {
            obj.insert("items".to_string(), items.build());
        }

        if let Some(enum_values) = self.enum_values {
            obj.insert("enum".to_string(), Value::Array(enum_values));
        }

        for (key, value) in self.additional {
            obj.insert(key, value);
        }

        Value::Object(obj)
    }
}

/// Generates a JSON Schema `Value` for common Rust primitive types.
///
/// This is used by the proc macros to generate schemas from function parameter types.
#[must_use]
pub fn schema_for_type(type_name: &str) -> Value {
    match type_name {
        "String" | "&str" | "str" => JsonSchemaBuilder::string().build(),
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => JsonSchemaBuilder::integer().build(),
        "f32" | "f64" => JsonSchemaBuilder::number().build(),
        "bool" => JsonSchemaBuilder::boolean().build(),
        _ => serde_json::json!({}),
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn test_simple_object_schema() {
        let schema = JsonSchemaBuilder::new()
            .property("name", JsonSchemaBuilder::string().description("The name"))
            .property("age", JsonSchemaBuilder::integer())
            .required("name")
            .build();

        assert_eq!(schema["type"], "object");
        assert_eq!(schema["properties"]["name"]["type"], "string");
        assert_eq!(schema["properties"]["name"]["description"], "The name");
        assert_eq!(schema["properties"]["age"]["type"], "integer");
        assert_eq!(schema["required"][0], "name");
    }

    #[test]
    fn test_array_schema() {
        let schema = JsonSchemaBuilder::array(JsonSchemaBuilder::string()).build();
        assert_eq!(schema["type"], "array");
        assert_eq!(schema["items"]["type"], "string");
    }

    #[test]
    fn test_number_constraints() {
        let schema = JsonSchemaBuilder::number()
            .minimum(0.0)
            .maximum(100.0)
            .build();
        assert_eq!(schema["type"], "number");
        assert_eq!(schema["minimum"], 0.0);
        assert_eq!(schema["maximum"], 100.0);
    }

    #[test]
    fn test_enum_schema() {
        let schema = JsonSchemaBuilder::string()
            .enum_values(vec![
                Value::from("red"),
                Value::from("green"),
                Value::from("blue"),
            ])
            .build();
        assert_eq!(schema["type"], "string");
        assert_eq!(schema["enum"][0], "red");
    }

    #[test]
    fn test_schema_for_type() {
        assert_eq!(schema_for_type("String")["type"], "string");
        assert_eq!(schema_for_type("i32")["type"], "integer");
        assert_eq!(schema_for_type("f64")["type"], "number");
        assert_eq!(schema_for_type("bool")["type"], "boolean");
    }

    #[test]
    fn test_pattern_schema() {
        let schema = JsonSchemaBuilder::string().pattern(r"^\d+$").build();
        assert_eq!(schema["pattern"], r"^\d+$");
    }
}
