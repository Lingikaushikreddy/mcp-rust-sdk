//! Shared utilities for proc macro expansion.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{Attribute, Expr, FnArg, Lit, Meta, Pat, PatType, Type};

/// Extracts doc comments from attributes and returns them as a single string.
pub fn extract_doc_comments(attrs: &[Attribute]) -> Option<String> {
    let docs: Vec<String> = attrs
        .iter()
        .filter_map(|attr| {
            if !attr.path().is_ident("doc") {
                return None;
            }
            match &attr.meta {
                Meta::NameValue(nv) => {
                    if let Expr::Lit(expr_lit) = &nv.value {
                        if let Lit::Str(s) = &expr_lit.lit {
                            return Some(s.value().trim().to_string());
                        }
                    }
                    None
                }
                _ => None,
            }
        })
        .collect();

    if docs.is_empty() {
        None
    } else {
        Some(docs.join(" "))
    }
}

/// Extracts parameter name and type from a function argument.
pub fn extract_param_info(arg: &FnArg) -> Option<(String, Type, Vec<Attribute>)> {
    match arg {
        FnArg::Typed(PatType { pat, ty, attrs, .. }) => {
            if let Pat::Ident(ident) = pat.as_ref() {
                Some((ident.ident.to_string(), *ty.clone(), attrs.clone()))
            } else {
                None
            }
        }
        FnArg::Receiver(_) => None,
    }
}

/// Generates a JSON Schema type string for a Rust type.
pub fn type_to_schema(ty: &Type) -> TokenStream {
    let type_str = quote!(#ty).to_string();
    let cleaned = type_str.replace(' ', "");

    // Handle Option<T> -- makes the field not required
    if cleaned.starts_with("Option<") {
        let inner = &cleaned[7..cleaned.len() - 1];
        return primitive_schema(inner);
    }

    // Handle Vec<T>
    if cleaned.starts_with("Vec<") {
        let inner = &cleaned[4..cleaned.len() - 1];
        let inner_schema = primitive_schema(inner);
        return quote! {
            {
                let mut schema = serde_json::Map::new();
                schema.insert("type".to_string(), serde_json::Value::from("array"));
                schema.insert("items".to_string(), #inner_schema);
                serde_json::Value::Object(schema)
            }
        };
    }

    primitive_schema(&cleaned)
}

/// Generates schema for a primitive type name.
fn primitive_schema(type_name: &str) -> TokenStream {
    let schema_type = match type_name {
        "String" | "&str" | "str" => "string",
        "i8" | "i16" | "i32" | "i64" | "i128" | "isize" | "u8" | "u16" | "u32" | "u64" | "u128"
        | "usize" => "integer",
        "f32" | "f64" => "number",
        "bool" => "boolean",
        _ => "object",
    };

    let schema_type_str = schema_type;
    quote! {
        serde_json::json!({ "type": #schema_type_str })
    }
}

/// Returns true if the type is `Option<T>`.
pub fn is_option_type(ty: &Type) -> bool {
    let type_str = quote!(#ty).to_string().replace(' ', "");
    type_str.starts_with("Option<")
}

/// Converts a snake_case string to PascalCase.
pub fn to_pascal_case(s: &str) -> String {
    s.split('_')
        .map(|word| {
            let mut chars = word.chars();
            match chars.next() {
                None => String::new(),
                Some(c) => c.to_uppercase().to_string() + chars.as_str(),
            }
        })
        .collect()
}

/// Parsed key-value attribute arguments for MCP macros.
///
/// This uses proper `syn` parsing to handle values containing commas,
/// escaped quotes, and other special characters.
pub struct McpAttrArgs {
    /// Parsed key-value pairs.
    pub entries: Vec<(String, AttrValue)>,
}

/// A single attribute value -- either a string literal or a bare identifier.
pub enum AttrValue {
    /// A string literal value (from `key = "value"`).
    Str(String),
    /// A bare identifier (like `destructive`).
    Ident,
}

impl McpAttrArgs {
    /// Gets a string value by key.
    pub fn get_str(&self, key: &str) -> Option<&str> {
        self.entries.iter().find_map(|(k, v)| {
            if k == key {
                match v {
                    AttrValue::Str(s) => Some(s.as_str()),
                    AttrValue::Ident => None,
                }
            } else {
                None
            }
        })
    }

    /// Returns whether a bare identifier is present.
    pub fn has_flag(&self, key: &str) -> bool {
        self.entries
            .iter()
            .any(|(k, v)| k == key && matches!(v, AttrValue::Ident))
    }
}

impl syn::parse::Parse for McpAttrArgs {
    fn parse(input: syn::parse::ParseStream<'_>) -> syn::Result<Self> {
        let mut entries = Vec::new();

        while !input.is_empty() {
            let key: syn::Ident = input.parse()?;
            let key_str = key.to_string();

            if input.peek(syn::Token![=]) {
                // key = "value"
                let _eq: syn::Token![=] = input.parse()?;
                let value: syn::LitStr = input.parse()?;
                entries.push((key_str, AttrValue::Str(value.value())));
            } else {
                // bare identifier (flag)
                entries.push((key_str, AttrValue::Ident));
            }

            // Consume optional comma
            if input.peek(syn::Token![,]) {
                let _comma: syn::Token![,] = input.parse()?;
            }
        }

        Ok(McpAttrArgs { entries })
    }
}
