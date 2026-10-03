//! Implementation of the `#[mcp_tool]` proc macro.
//!
//! This macro transforms an async function into an MCP tool handler by:
//! 1. Extracting the function name, parameters, and return type.
//! 2. Generating a parameters struct with serde derives.
//! 3. Building a JSON Schema from the parameter types.
//! 4. Generating a `ToolHandler` implementation.

use proc_macro2::TokenStream;
use quote::{format_ident, quote};
use syn::parse::{Parse, ParseStream};
use syn::{parse2, Expr, Ident, ItemFn, Lit, Token};

use crate::schema::{extract_param_schemas, generate_input_schema};
use crate::utils::{extract_doc_comments, extract_param_info, to_pascal_case};

/// Attribute arguments for `#[mcp_tool]`.
#[derive(Default)]
struct ToolAttrs {
    name: Option<String>,
    description: Option<String>,
    title: Option<String>,
    read_only_hint: Option<bool>,
    destructive_hint: Option<bool>,
    idempotent_hint: Option<bool>,
    open_world_hint: Option<bool>,
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

    let attrs: ToolAttrs = parse2(attr)?;
    let annotations_token = generate_annotations(&attrs);

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
                    annotations: #annotations_token,
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

impl Parse for ToolAttrs {
    fn parse(input: ParseStream<'_>) -> syn::Result<Self> {
        let mut attrs = Self::default();

        while !input.is_empty() {
            let key: Ident = input.parse()?;
            match key.to_string().as_str() {
                "name" => parse_string_option(input, &key, &mut attrs.name)?,
                "description" => parse_string_option(input, &key, &mut attrs.description)?,
                "title" => parse_string_option(input, &key, &mut attrs.title)?,
                "read_only_hint" => parse_bool_option(input, &key, &mut attrs.read_only_hint)?,
                "destructive_hint" => parse_bool_option(input, &key, &mut attrs.destructive_hint)?,
                "idempotent_hint" => parse_bool_option(input, &key, &mut attrs.idempotent_hint)?,
                "open_world_hint" => parse_bool_option(input, &key, &mut attrs.open_world_hint)?,
                "destructive" => {
                    if input.peek(Token![=]) {
                        return Err(syn::Error::new_spanned(
                            key,
                            "`destructive` is a bare flag; use `destructive_hint = true` or `destructive_hint = false`",
                        ));
                    }
                    reject_duplicate(&key, &attrs.destructive_hint)?;
                    attrs.destructive_hint = Some(true);
                }
                _ => {
                    return Err(syn::Error::new_spanned(
                        &key,
                        format!("unknown #[mcp_tool] option `{key}`"),
                    ));
                }
            }

            if !input.is_empty() {
                input.parse::<Token![,]>()?;
            }
        }

        Ok(attrs)
    }
}

fn reject_duplicate<T>(key: &Ident, slot: &Option<T>) -> syn::Result<()> {
    if slot.is_some() {
        let message = if key == "destructive" || key == "destructive_hint" {
            "duplicate `destructive_hint` option; `destructive` is its legacy alias".to_string()
        } else {
            format!("duplicate `{key}` option")
        };
        return Err(syn::Error::new_spanned(key, message));
    }
    Ok(())
}

fn parse_option_value(input: ParseStream<'_>, key: &Ident) -> syn::Result<Expr> {
    if !input.peek(Token![=]) {
        return Err(syn::Error::new_spanned(
            key,
            format!("expected a value for `{key}` using `{key} = ...`"),
        ));
    }
    input.parse::<Token![=]>()?;
    input.parse()
}

fn parse_string_option(
    input: ParseStream<'_>,
    key: &Ident,
    slot: &mut Option<String>,
) -> syn::Result<()> {
    reject_duplicate(key, slot)?;
    let value = parse_option_value(input, key)?;
    if let Expr::Lit(expr) = &value {
        if let Lit::Str(value) = &expr.lit {
            *slot = Some(value.value());
            return Ok(());
        }
    }
    Err(syn::Error::new_spanned(
        value,
        format!("expected a string literal for `{key}`"),
    ))
}

fn parse_bool_option(
    input: ParseStream<'_>,
    key: &Ident,
    slot: &mut Option<bool>,
) -> syn::Result<()> {
    reject_duplicate(key, slot)?;
    let value = parse_option_value(input, key)?;
    if let Expr::Lit(expr) = &value {
        if let Lit::Bool(value) = &expr.lit {
            *slot = Some(value.value);
            return Ok(());
        }
    }
    Err(syn::Error::new_spanned(
        value,
        format!("expected a boolean literal (`true` or `false`) for `{key}`"),
    ))
}

fn generate_annotations(attrs: &ToolAttrs) -> TokenStream {
    if attrs.title.is_none()
        && attrs.read_only_hint.is_none()
        && attrs.destructive_hint.is_none()
        && attrs.idempotent_hint.is_none()
        && attrs.open_world_hint.is_none()
    {
        return quote! { None };
    }

    let title = match &attrs.title {
        Some(title) => quote! { Some(#title.to_string()) },
        None => quote! { None },
    };
    let read_only_hint = option_bool_tokens(attrs.read_only_hint);
    let destructive_hint = option_bool_tokens(attrs.destructive_hint);
    let idempotent_hint = option_bool_tokens(attrs.idempotent_hint);
    let open_world_hint = option_bool_tokens(attrs.open_world_hint);

    quote! {
        Some(mcp_sdk::protocol::messages::ToolAnnotations {
            title: #title,
            read_only_hint: #read_only_hint,
            destructive_hint: #destructive_hint,
            idempotent_hint: #idempotent_hint,
            open_world_hint: #open_world_hint,
        })
    }
}

fn option_bool_tokens(value: Option<bool>) -> TokenStream {
    match value {
        Some(value) => quote! { Some(#value) },
        None => quote! { None },
    }
}
