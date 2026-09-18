//! Implementation of the `#[mcp_resource]` proc macro.
//!
//! This macro transforms an async function into an MCP resource handler by:
//! 1. Extracting the URI template, description, and MIME type from attributes.
//! 2. Generating a `ResourceHandler` implementation.
//! 3. Setting up URI template matching for parameterized resources.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, ItemFn};

use crate::utils::{extract_doc_comments, extract_param_info, to_pascal_case, McpAttrArgs};

/// Expands the `#[mcp_resource]` attribute macro.
pub fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let input_fn: ItemFn = parse2(item)?;

    // Validate: must be async
    if input_fn.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            input_fn.sig.fn_token,
            "#[mcp_resource] can only be applied to async functions",
        ));
    }

    // Parse attributes using proper syn parsing
    let attrs = parse_resource_attrs(attr)?;

    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let fn_output = &input_fn.sig.output;

    let resource_name = attrs
        .name
        .unwrap_or_else(|| fn_name.to_string().replace('_', "-"));
    let uri = attrs
        .uri
        .unwrap_or_else(|| format!("resource:///{resource_name}"));
    let description = attrs
        .description
        .or_else(|| extract_doc_comments(&input_fn.attrs));
    let mime_type = attrs.mime_type;

    let is_template = uri.contains('{');

    let description_token = match description {
        Some(ref d) => quote! { Some(#d.to_string()) },
        None => quote! { None },
    };
    let mime_type_token = match mime_type {
        Some(ref m) => quote! { Some(#m.to_string()) },
        None => quote! { None },
    };

    let handler_struct_name = format_ident!(
        "__McpResource{}Handler",
        to_pascal_case(&fn_name.to_string())
    );
    let impl_fn_name = format_ident!("{}_impl", fn_name);

    // Extract parameters
    let params: Vec<_> = input_fn
        .sig
        .inputs
        .iter()
        .filter_map(extract_param_info)
        .collect();

    let impl_params: Vec<TokenStream> = params
        .iter()
        .map(|(name, ty, _)| {
            let field_name = format_ident!("{}", name);
            quote! { #field_name: #ty }
        })
        .collect();

    // For template resources, we need to extract parameters from the URI
    let uri_param_extraction = if is_template && !params.is_empty() {
        // Simple: for now, assume a single path parameter extracted from the URI
        let param_name = &params[0].0;
        let param_ident = format_ident!("{}", param_name);
        quote! {
            // Extract path parameter from URI by removing the template prefix
            let uri_str = uri;
            let #param_ident = {
                let template = #uri;
                // Find where the template variable starts
                let prefix = template.split('{').next().unwrap_or("");
                let value = if uri_str.starts_with(prefix) {
                    &uri_str[prefix.len()..]
                } else {
                    uri_str
                };
                value.to_string()
            };
        }
    } else {
        quote! {}
    };

    let call_args: Vec<TokenStream> = params
        .iter()
        .map(|(name, _, _)| {
            let ident = format_ident!("{}", name);
            quote! { #ident }
        })
        .collect();

    let template_info_impl = if is_template {
        quote! {
            fn template_info(&self) -> Option<mcp_sdk::protocol::messages::ResourceTemplateInfo> {
                Some(mcp_sdk::protocol::messages::ResourceTemplateInfo {
                    uri_template: #uri.to_string(),
                    name: #resource_name.to_string(),
                    description: #description_token,
                    mime_type: #mime_type_token,
                })
            }

            fn is_template(&self) -> bool {
                true
            }
        }
    } else {
        quote! {}
    };

    let expanded = quote! {
        #[allow(non_camel_case_types)]
        #fn_vis struct #handler_struct_name;

        #[async_trait::async_trait]
        impl mcp_sdk::handler::ResourceHandler for #handler_struct_name {
            fn info(&self) -> mcp_sdk::protocol::messages::ResourceInfo {
                mcp_sdk::protocol::messages::ResourceInfo {
                    uri: #uri.to_string(),
                    name: #resource_name.to_string(),
                    description: #description_token,
                    mime_type: #mime_type_token,
                }
            }

            #template_info_impl

            async fn read(&self, uri: &str) -> Result<mcp_sdk::types::content::ResourceContent, mcp_sdk::types::error::ResourceError> {
                #uri_param_extraction
                #impl_fn_name(#(#call_args),*).await
            }
        }

        async fn #impl_fn_name(#(#impl_params),*) #fn_output
            #fn_block

        #fn_vis fn #fn_name() -> #handler_struct_name {
            #handler_struct_name
        }
    };

    Ok(expanded)
}

/// Parsed resource attributes.
struct ResourceAttrs {
    uri: Option<String>,
    name: Option<String>,
    description: Option<String>,
    mime_type: Option<String>,
}

/// Parses resource attributes using proper `syn::Parse` implementation.
fn parse_resource_attrs(attr: TokenStream) -> syn::Result<ResourceAttrs> {
    if attr.is_empty() {
        return Ok(ResourceAttrs {
            uri: None,
            name: None,
            description: None,
            mime_type: None,
        });
    }

    let args: McpAttrArgs = syn::parse2(attr)?;

    Ok(ResourceAttrs {
        uri: args.get_str("uri").map(String::from),
        name: args.get_str("name").map(String::from),
        description: args.get_str("description").map(String::from),
        mime_type: args.get_str("mime_type").map(String::from),
    })
}
