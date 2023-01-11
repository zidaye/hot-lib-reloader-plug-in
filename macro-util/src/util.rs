use proc_macro2::{Span, TokenTree};
use std::path::PathBuf;
use syn::{Attribute, Error, ForeignItemFn, LitStr, Result};

pub fn ident_from_pat(
    pat: &syn::Pat,
    func_name: &proc_macro2::Ident,
    span: proc_macro2::Span,
) -> syn::Result<syn::Ident> {
    match pat {
        syn::Pat::Ident(pat) => Ok(pat.ident.clone()),
        _ => Err(syn::Error::new(
            span,
            format!("generating call for library function: signature of function {func_name} cannot be converted"),
        )),
    }
}

/// Reads the contents of a Rust source file and finds the top-level functions that have
/// - visibility public
/// - #[no_mangle] attribute
/// It converts these functions into a [syn::ForeignItemFn] so that those can
/// serve as lib function declarations of the lib reloader.
pub fn read_functions_from_file(
    file_name: LitStr,
    ignore_no_mangle: bool,
) -> Result<Vec<(ForeignItemFn, Span)>> {
    let span = file_name.span();
    let path: PathBuf = file_name.value().into();

    if !path.exists() {
        return Err(Error::new(span, format!("Could not find Rust source file {path:?}. Please make sure that you specify the file path from the project root directory. Please not that this has been changed in hot-lib-reloader v0.5 -> v0.6. See https://github.com/rksm/hot-lib-reloader-rs/issues/13.")));
    }

    let content = std::fs::read_to_string(&path)
        .map_err(|err| Error::new(span, format!("Error reading file {path:?}: {err}")))?;

    let ast = syn::parse_file(&content)?;

    let mut functions = Vec::new();

    for item in ast.items {
        match item {
            syn::Item::Fn(fun) => {
                match fun.vis {
                    syn::Visibility::Public(_) => {}
                    _ => continue,
                };

                // we can optionally assume that the function will be unmangled
                // by other means than a direct attribute
                if !ignore_no_mangle {
                    let no_mangle = fun
                        .attrs
                        .iter()
                        .filter_map(|attr| attr.path.get_ident())
                        .any(|ident| *ident == "no_mangle");

                    if !no_mangle {
                        continue;
                    };
                }

                let fun = ForeignItemFn {
                    attrs: Vec::new(),
                    vis: fun.vis,
                    sig: fun.sig,
                    semi_token: syn::token::Semi(span),
                };

                functions.push((fun, span));
            }
            _ => continue,
        }
    }

    Ok(functions)
}

pub fn get_lib_name_by_attr_args(span: Span, attr: &Attribute, func_name: &str) -> Result<String> {
    let mut tokens = attr.tokens.clone().into_iter();
    if let Some(token) = tokens.next() {
        if let proc_macro2::TokenTree::Group(group) = token {
            let mut args_token = group.stream().into_iter();
            // x = "Y"
            match (args_token.next(), args_token.next(), args_token.next()) {
                (Some(args_name), Some(punct), Some(args_value)) => {
                    let match_token_tree = |token_tree: &TokenTree| -> Result<String> {
                        match token_tree {
                            TokenTree::Ident(ident) => Ok(ident.to_string()),
                            TokenTree::Punct(punct) => Ok(punct.to_string()),
                            TokenTree::Literal(lit_str) => Ok(lit_str.to_string()),
                            _ => {
                                return Err(syn::Error::new(
                                        span,
                                        format!("generating call for library function: signature of function {func_name} No find library file specified path"),
                                    ));
                            }
                        }
                    };
                    let args_name = match_token_tree(&args_name)?;
                    let punct = match_token_tree(&punct)?;
                    let args_value = match_token_tree(&args_value)?;
                    let args_value = args_value.trim_end_matches('"').trim_start_matches('"');

                    if args_name == "lib_name" && punct == "=" {
                        // sort out os dependent file name
                        #[cfg(target_os = "macos")]
                        let (prefix, ext) = ("lib", "dylib");
                        #[cfg(target_os = "linux")]
                        let (prefix, _) = ("lib", "so");
                        #[cfg(target_os = "windows")]
                        let (prefix, ext) = ("", "dll");

                        let lib_name = format!("{prefix}{args_value}");
                        return Ok(lib_name);
                    }
                }
                _ => {}
            }
        }
    }
    Err(syn::Error::new(
            span,
            format!("generating call for library function: signature of function {func_name} No find library file specified path"),
        ))
}
