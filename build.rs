use std::{env, fs, path::PathBuf};

fn main() {
    let directory = PathBuf::from(env::var_os("CARGO_MANIFEST_DIR").unwrap()).join("plugins");
    println!("cargo:rerun-if-changed={}", directory.display());
    let mut paths = fs::read_dir(&directory)
        .unwrap()
        .map(|entry| entry.unwrap().path())
        .filter(|path| {
            path.extension()
                .is_some_and(|extension| extension == "toml")
        })
        .collect::<Vec<_>>();
    paths.sort();
    let mut source = String::from("const BUNDLED: &[&str] = &[\n");
    for path in paths {
        source.push_str(&format!("include_str!({:?}),\n", path.to_str().unwrap()));
    }
    source.push_str("];\n");
    let output = PathBuf::from(env::var_os("OUT_DIR").unwrap()).join("bundled_plugins.rs");
    fs::write(output, source).unwrap();
}
