//! Implementation of the `#[mcp_tool]` proc macro.
//!
//! This macro transforms an async function into an MCP tool handler by:
//! 1. Extracting the function name, parameters, and return type.
//! 2. Generating a parameters struct with serde derives.
//! 3. Building a JSON Schema from the parameter types.
//! 4. Generating a `ToolHandler` implementation.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, ItemFn};

use crate::schema::{extract_param_schemas, generate_input_schema};
use crate::utils::{extract_doc_comments, extract_param_info, to_pascal_case, McpAttrArgs};

/// Attribute arguments for `#[mcp_tool]`.
#[allow(dead_code)]
struct ToolAttrs {
    name: Option<String>,
    description: Option<String>,
    destructive: bool,
}

/// Expands the `#[mcp_tool]` attribute macro.
pub fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let input_fn: ItemFn = parse2(item)?;

    // Validate: must be async
    if input_fn.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            input_fn.sig.fn_token,
            "#[mcp_tool] can only be applied to async functions",
        ));
    }

    // Validate: no generics
    if !input_fn.sig.generics.params.is_empty() {
        return Err(syn::Error::new_spanned(
            &input_fn.sig.generics,
            "#[mcp_tool] does not support generic functions",
        ));
    }

    // Parse attributes using proper syn parsing
    let attrs = parse_tool_attrs(attr)?;

    let fn_name = &input_fn.sig.ident;
    let fn_name_str = attrs.name.unwrap_or_else(|| fn_name.to_string());
    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let fn_output = &input_fn.sig.output;

    // Get description from attribute or doc comments
    let description = attrs
        .description
        .or_else(|| extract_doc_comments(&input_fn.attrs));

    let description_token = match description {
        Some(ref d) => quote! { Some(#d.to_string()) },
        None => quote! { None },
    };

    // Generate parameter struct name
    let params_struct_name = format_ident!("__McpTool{}Params", to_pascal_case(&fn_name_str));
    let handler_struct_name = format_ident!("__McpTool{}Handler", to_pascal_case(&fn_name_str));
    let impl_fn_name = format_ident!("{}_impl", fn_name);

    // Extract parameters
    let params: Vec<_> = input_fn
        .sig
        .inputs
        .iter()
        .filter_map(extract_param_info)
        .collect();

    // Generate the params struct fields
    let param_fields: Vec<TokenStream> = params
        .iter()
        .map(|(name, ty, attrs)| {
            let field_name = format_ident!("{}", name);
            let doc_attrs: Vec<_> = attrs.iter().filter(|a| a.path().is_ident("doc")).collect();
            quote! {
                #(#doc_attrs)*
                pub #field_name: #ty,
            }
        })
        .collect();

    // Generate the call arguments (destructuring from params struct)
    let call_args: Vec<TokenStream> = params
        .iter()
        .map(|(name, _, _)| {
            let field_name = format_ident!("{}", name);
            quote! { params.#field_name }
        })
        .collect();

    // Generate the parameter names and types for the impl function signature
    let impl_params: Vec<TokenStream> = params
        .iter()
        .map(|(name, ty, _)| {
            let field_name = format_ident!("{}", name);
            quote! { #field_name: #ty }
        })
        .collect();

    // Generate input schema
    let param_schemas =
        extract_param_schemas(&input_fn.sig.inputs.iter().cloned().collect::<Vec<_>>());
    let input_schema = generate_input_schema(&param_schemas);

    let expanded = quote! {
        // The params struct for deserialization
        #[derive(serde::Deserialize)]
        #[allow(non_camel_case_types)]
        struct #params_struct_name {
            #(#param_fields)*
        }

        // The handler struct
        #[allow(non_camel_case_types)]
        #fn_vis struct #handler_struct_name;

        #[async_trait::async_trait]
        impl mcp_sdk::handler::ToolHandler for #handler_struct_name {
            fn info(&self) -> mcp_sdk::protocol::messages::ToolInfo {
                mcp_sdk::protocol::messages::ToolInfo {
                    name: #fn_name_str.to_string(),
                    description: #description_token,
                    input_schema: #input_schema,
                }
            }

            async fn call(
                &self,
                arguments: serde_json::Value,
                context: &mcp_sdk::context::ToolContext,
            ) -> Result<mcp_sdk::types::content::CallToolResult, mcp_sdk::types::error::ToolError> {
                let params: #params_struct_name = serde_json::from_value(arguments)
                    .map_err(|e| mcp_sdk::types::error::ToolError::InvalidParams(e.to_string()))?;
                let result = #impl_fn_name(#(#call_args),*).await?;
                Ok(result.into())
            }
        }

        // The original function implementation (renamed)
        async fn #impl_fn_name(#(#impl_params),*) #fn_output
            #fn_block

        // Public function that returns the handler
        #fn_vis fn #fn_name() -> #handler_struct_name {
            #handler_struct_name
        }
    };

    Ok(expanded)
}

/// Parses tool attributes using proper `syn::Parse` implementation.
///
/// This correctly handles descriptions containing commas, quotes,
/// and other special characters.
fn parse_tool_attrs(attr: TokenStream) -> syn::Result<ToolAttrs> {
    if attr.is_empty() {
        return Ok(ToolAttrs {
            name: None,
            description: None,
            destructive: false,
        });
    }

    let args: McpAttrArgs = syn::parse2(attr)?;

    Ok(ToolAttrs {
        name: args.get_str("name").map(String::from),
        description: args.get_str("description").map(String::from),
        destructive: args.has_flag("destructive"),
    })
}
