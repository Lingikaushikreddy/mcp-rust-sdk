//! Implementation of the `#[mcp_prompt]` proc macro.
//!
//! This macro transforms an async function into an MCP prompt handler by:
//! 1. Extracting the function name, parameters, and doc comments.
//! 2. Building the prompt argument definitions.
//! 3. Generating a `PromptHandler` implementation.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::{parse2, ItemFn};

use crate::utils::{
    extract_doc_comments, extract_param_info, is_option_type, to_pascal_case, McpAttrArgs,
};

/// Expands the `#[mcp_prompt]` attribute macro.
pub fn expand(attr: TokenStream, item: TokenStream) -> syn::Result<TokenStream> {
    let input_fn: ItemFn = parse2(item)?;

    // Validate: must be async
    if input_fn.sig.asyncness.is_none() {
        return Err(syn::Error::new_spanned(
            input_fn.sig.fn_token,
            "#[mcp_prompt] can only be applied to async functions",
        ));
    }

    let attrs = parse_prompt_attrs(attr)?;

    let fn_name = &input_fn.sig.ident;
    let fn_vis = &input_fn.vis;
    let fn_block = &input_fn.block;
    let fn_output = &input_fn.sig.output;

    let prompt_name = attrs
        .name
        .unwrap_or_else(|| fn_name.to_string().replace('_', "-"));
    let description = attrs
        .description
        .or_else(|| extract_doc_comments(&input_fn.attrs));

    let description_token = match description {
        Some(ref d) => quote! { Some(#d.to_string()) },
        None => quote! { None },
    };

    let handler_struct_name =
        format_ident!("__McpPrompt{}Handler", to_pascal_case(&fn_name.to_string()));
    let impl_fn_name = format_ident!("{}_impl", fn_name);

    // Extract parameters
    let params: Vec<_> = input_fn
        .sig
        .inputs
        .iter()
        .filter_map(extract_param_info)
        .collect();

    // Generate prompt argument metadata
    let argument_defs: Vec<TokenStream> = params
        .iter()
        .map(|(name, ty, attrs)| {
            let desc = extract_doc_comments(attrs);
            let desc_token = match desc {
                Some(ref d) => quote! { Some(#d.to_string()) },
                None => quote! { None },
            };
            let is_required = !is_option_type(ty);
            quote! {
                mcp_sdk::protocol::messages::PromptArgument {
                    name: #name.to_string(),
                    description: #desc_token,
                    required: Some(#is_required),
                }
            }
        })
        .collect();

    let has_arguments = !argument_defs.is_empty();
    let arguments_token = if has_arguments {
        quote! { Some(vec![#(#argument_defs),*]) }
    } else {
        quote! { None }
    };

    // Generate parameter extraction from HashMap<String, String>
    let param_extractions: Vec<TokenStream> = params
        .iter()
        .map(|(name, ty, _)| {
            let ident = format_ident!("{}", name);
            if is_option_type(ty) {
                quote! {
                    let #ident = arguments.get(#name).cloned();
                }
            } else {
                quote! {
                    let #ident = arguments.get(#name)
                        .ok_or_else(|| mcp_sdk::types::error::PromptError::InvalidArguments(
                            format!("Missing required argument: {}", #name)
                        ))?
                        .clone();
                }
            }
        })
        .collect();

    let impl_params: Vec<TokenStream> = params
        .iter()
        .map(|(name, ty, _)| {
            let ident = format_ident!("{}", name);
            if is_option_type(ty) {
                quote! { #ident: #ty }
            } else {
                quote! { #ident: String }
            }
        })
        .collect();

    let call_args: Vec<TokenStream> = params
        .iter()
        .map(|(name, _, _)| {
            let ident = format_ident!("{}", name);
            quote! { #ident }
        })
        .collect();

    let expanded = quote! {
        #[allow(non_camel_case_types)]
        #fn_vis struct #handler_struct_name;

        #[async_trait::async_trait]
        impl mcp_sdk::handler::PromptHandler for #handler_struct_name {
            fn info(&self) -> mcp_sdk::protocol::messages::PromptInfo {
                mcp_sdk::protocol::messages::PromptInfo {
                    name: #prompt_name.to_string(),
                    description: #description_token,
                    arguments: #arguments_token,
                }
            }

            async fn get(
                &self,
                arguments: std::collections::HashMap<String, String>,
            ) -> Result<Vec<mcp_sdk::types::content::PromptMessage>, mcp_sdk::types::error::PromptError> {
                #(#param_extractions)*
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

/// Parsed prompt attributes.
struct PromptAttrs {
    name: Option<String>,
    description: Option<String>,
}

/// Parses prompt attributes using proper `syn::Parse` implementation.
fn parse_prompt_attrs(attr: TokenStream) -> syn::Result<PromptAttrs> {
    if attr.is_empty() {
        return Ok(PromptAttrs {
            name: None,
            description: None,
        });
    }

    let args: McpAttrArgs = syn::parse2(attr)?;

    Ok(PromptAttrs {
        name: args.get_str("name").map(String::from),
        description: args.get_str("description").map(String::from),
    })
}
