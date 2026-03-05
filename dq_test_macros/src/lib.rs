use proc_macro::TokenStream;
use proc_macro2::{Span, TokenStream as TokenStream2};
use quote::quote;
use std::collections::HashSet;
use std::fs;
use std::path::{Path, PathBuf};
use syn::{parse_macro_input, LitStr};

#[proc_macro]
pub fn fixture_tests(input: TokenStream) -> TokenStream {
    let fixture_lit = parse_macro_input!(input as LitStr);
    let fixture_rel_path = fixture_lit.value();
    let mut used = HashSet::<String>::new();

    match generate_tests_for_fixture(&fixture_rel_path, fixture_lit.span(), &mut used) {
        Ok(tests) => quote! { #(#tests)* }.into(),
        Err(err) => compile_error(err),
    }
}

#[proc_macro]
pub fn fixture_tests_dir(input: TokenStream) -> TokenStream {
    let fixture_dir_lit = parse_macro_input!(input as LitStr);
    let fixture_dir_rel_path = fixture_dir_lit.value();

    let fixture_files = match list_fixture_files_in_dir(&fixture_dir_rel_path) {
        Ok(files) => files,
        Err(err) => return compile_error(err),
    };

    if fixture_files.is_empty() {
        return compile_error(format!(
            "fixture_tests_dir!: no .toml fixtures found in `{}`",
            fixture_dir_rel_path
        ));
    }

    let mut used = HashSet::<String>::new();
    let mut all_tests = Vec::<TokenStream2>::new();

    for fixture_rel_path in fixture_files {
        match generate_tests_for_fixture(&fixture_rel_path, fixture_dir_lit.span(), &mut used) {
            Ok(mut tests) => all_tests.append(&mut tests),
            Err(err) => return compile_error(err),
        }
    }

    quote! {
        #(#all_tests)*
    }
    .into()
}

fn generate_tests_for_fixture(
    fixture_rel_path: &str,
    span: Span,
    used: &mut HashSet<String>,
) -> Result<Vec<TokenStream2>, String> {
    let fixture_abs_path = fixture_abs_path(fixture_rel_path)?;
    let suite = dq_test_fixtures::load_suite_from_path(&fixture_abs_path)
        .map_err(|err| format!("fixture_tests!: {err}"))?;

    let fixture_stem = fixture_stem(fixture_rel_path);
    let fixture_rel_lit = LitStr::new(fixture_rel_path, span);
    let mut tests = Vec::with_capacity(suite.cases.len());

    for case in suite.cases {
        let case_name = case.name;
        let fn_name_raw = format!("{}_{}", fixture_stem, case_name);
        let fn_name_sanitized = sanitize_ident(&fn_name_raw);
        if !used.insert(fn_name_sanitized.clone()) {
            return Err(format!(
                "fixture_tests!: `{}` duplicate generated test fn `{}`",
                fixture_abs_path.display(),
                fn_name_sanitized
            ));
        }

        let fn_ident = syn::parse_str::<syn::Ident>(&fn_name_sanitized).map_err(|err| {
            format!(
                "fixture_tests!: invalid generated test name `{}`: {err}",
                fn_name_sanitized
            )
        })?;

        let case_lit = LitStr::new(&case_name, span);

        tests.push(quote! {
            #[test]
            fn #fn_ident() {
                let _ = include_str!(concat!(env!("CARGO_MANIFEST_DIR"), "/", #fixture_rel_lit));
                crate::common::run_suite_fixture_case(#fixture_rel_lit, #case_lit);
            }
        });
    }

    Ok(tests)
}

fn list_fixture_files_in_dir(fixture_dir_rel_path: &str) -> Result<Vec<String>, String> {
    let fixture_dir_abs_path = fixture_abs_path(fixture_dir_rel_path)?;
    let entries = fs::read_dir(&fixture_dir_abs_path).map_err(|err| {
        format!(
            "fixture_tests_dir!: failed reading `{}`: {err}",
            fixture_dir_abs_path.display()
        )
    })?;

    let mut fixture_files = Vec::new();

    for entry in entries {
        let entry = entry.map_err(|err| {
            format!(
                "fixture_tests_dir!: failed reading entry in `{}`: {err}",
                fixture_dir_abs_path.display()
            )
        })?;

        let file_type = entry.file_type().map_err(|err| {
            format!(
                "fixture_tests_dir!: failed getting file type for `{}`: {err}",
                entry.path().display()
            )
        })?;

        if !file_type.is_file() {
            continue;
        }

        let path = entry.path();
        if path.extension().and_then(|ext| ext.to_str()) != Some("toml") {
            continue;
        }

        let file_name = entry.file_name();
        let rel_path = Path::new(fixture_dir_rel_path).join(file_name);
        fixture_files.push(path_to_string(&rel_path));
    }

    fixture_files.sort();
    Ok(fixture_files)
}

fn path_to_string(path: &Path) -> String {
    path.to_string_lossy().replace('\\', "/")
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
