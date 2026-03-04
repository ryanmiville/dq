use proc_macro::TokenStream;
use quote::quote;
use std::path::{Path, PathBuf};
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn fixture_tests(input: TokenStream) -> TokenStream {
    let fixture_lit = parse_macro_input!(input as LitStr);
    let fixture_rel_path = fixture_lit.value();

    let fixture_abs_path = match fixture_abs_path(&fixture_rel_path) {
        Ok(path) => path,
        Err(err) => return compile_error(err),
    };

    let suite = match dq_test_fixtures::load_suite_from_path(&fixture_abs_path) {
        Ok(suite) => suite,
        Err(err) => return compile_error(format!("fixture_tests!: {err}")),
    };

    let fixture_stem = fixture_stem(&fixture_rel_path);
    let fixture_rel_lit = LitStr::new(&fixture_rel_path, fixture_lit.span());
    let mut tests = Vec::with_capacity(suite.cases.len());
    let mut used = std::collections::HashSet::<String>::new();

    for case in suite.cases {
        let case_name = case.name;
        let fn_name_raw = format!("{}_{}", fixture_stem, case_name);
        let fn_name_sanitized = sanitize_ident(&fn_name_raw);
        if !used.insert(fn_name_sanitized.clone()) {
            return compile_error(format!(
                "fixture_tests!: `{}` duplicate generated test fn `{}`",
                fixture_abs_path.display(),
                fn_name_sanitized
            ));
        }

        let fn_ident = match syn::parse_str::<syn::Ident>(&fn_name_sanitized) {
            Ok(ident) => ident,
            Err(err) => {
                return compile_error(format!(
                    "fixture_tests!: invalid generated test name `{}`: {err}",
                    fn_name_sanitized
                ));
            }
        };
        let case_lit = LitStr::new(&case_name, fixture_lit.span());

        tests.push(quote! {
            #[test]
            fn #fn_ident() {
                let _ = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #fixture_rel_lit));
                crate::common::run_suite_fixture_case(#fixture_rel_lit, #case_lit);
            }
        });
    }

    quote! {
        #(#tests)*
    }
    .into()
}

fn fixture_abs_path(fixture_rel_path: &str) -> Result<PathBuf, String> {
    let manifest_dir = std::env::var("CARGO_MANIFEST_DIR")
        .map_err(|err| format!("fixture_tests!: missing CARGO_MANIFEST_DIR: {err}"))?;
    Ok(Path::new(&manifest_dir).join(fixture_rel_path))
}

fn fixture_stem(path: &str) -> String {
    Path::new(path)
        .file_stem()
        .and_then(|stem| stem.to_str())
        .map(sanitize_ident)
        .unwrap_or_else(|| "fixture".to_string())
}

fn sanitize_ident(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut last_underscore = false;

    for ch in input.chars() {
        let mapped = if ch.is_ascii_alphanumeric() || ch == '_' {
            ch.to_ascii_lowercase()
        } else {
            '_'
        };

        if mapped == '_' {
            if !last_underscore {
                out.push('_');
            }
            last_underscore = true;
        } else {
            out.push(mapped);
            last_underscore = false;
        }
    }

    let trimmed = out.trim_matches('_');
    let mut normalized = if trimmed.is_empty() {
        "case".to_string()
    } else {
        trimmed.to_string()
    };

    if normalized
        .chars()
        .next()
        .is_some_and(|first| first.is_ascii_digit())
    {
        normalized.insert(0, '_');
    }

    normalized
}

fn compile_error(message: String) -> TokenStream {
    quote! { compile_error!(#message); }.into()
}
