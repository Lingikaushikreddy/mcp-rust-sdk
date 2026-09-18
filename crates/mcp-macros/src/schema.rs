//! Compile-time JSON Schema generation from function parameters.
//!
//! This module generates the `inputSchema` JSON object that MCP tools
//! expose in their metadata. The schema is built at compile time from
//! the function signature.

use proc_macro2::TokenStream;
use quote::quote;
use syn::{FnArg, Type};

use crate::utils::{extract_doc_comments, extract_param_info, is_option_type, type_to_schema};

/// Represents a parameter extracted from a function signature,
/// ready for schema generation.
pub struct ParamSchema {
    /// The parameter name.
    pub name: String,
    /// The parameter type.
    pub ty: Type,
    /// Documentation comment for this parameter.
    pub description: Option<String>,
    /// Whether this parameter is required (non-Option).
    pub required: bool,
}

/// Extracts parameter schemas from a function's arguments.
pub fn extract_param_schemas(args: &[FnArg]) -> Vec<ParamSchema> {
    args.iter()
        .filter_map(|arg| {
            let (name, ty, attrs) = extract_param_info(arg)?;
            let description = extract_doc_comments(&attrs);
            let required = !is_option_type(&ty);
            Some(ParamSchema {
                name,
                ty,
                description,
                required,
            })
        })
        .collect()
}

/// Generates a JSON Schema `serde_json::Value` expression from parameter schemas.
pub fn generate_input_schema(params: &[ParamSchema]) -> TokenStream {
    if params.is_empty() {
        return quote! {
            serde_json::json!({
                "type": "object",
                "properties": {},
                "required": []
            })
        };
    }

    let property_inserts: Vec<TokenStream> = params
        .iter()
        .map(|param| {
            let name = &param.name;
            let type_schema = type_to_schema(&param.ty);
            if let Some(ref desc) = param.description {
                quote! {
                    {
                        let mut prop = #type_schema;
                        if let Some(obj) = prop.as_object_mut() {
                            obj.insert("description".to_string(), serde_json::Value::from(#desc));
                        }
                        properties.insert(#name.to_string(), prop);
                    }
                }
            } else {
                quote! {
                    properties.insert(#name.to_string(), #type_schema);
                }
            }
        })
        .collect();

    let required_names: Vec<&str> = params
        .iter()
        .filter(|p| p.required)
        .map(|p| p.name.as_str())
        .collect();

    quote! {
        {
            let mut properties = serde_json::Map::new();
            #(#property_inserts)*

            let required: Vec<serde_json::Value> = vec![
                #(serde_json::Value::from(#required_names)),*
            ];

            let mut schema = serde_json::Map::new();
            schema.insert("type".to_string(), serde_json::Value::from("object"));
            schema.insert("properties".to_string(), serde_json::Value::Object(properties));
            if !required.is_empty() {
                schema.insert("required".to_string(), serde_json::Value::Array(required));
            }
            serde_json::Value::Object(schema)
        }
    }
}
