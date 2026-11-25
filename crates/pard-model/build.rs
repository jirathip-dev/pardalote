//! Compiles the provider files in `providers/` into the crate.

use std::error::Error;
use std::fmt::Write as _;
use std::path::PathBuf;
use std::{env, fs};

fn main() -> Result<(), Box<dyn Error>> {
    let dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR")?).join("providers");
    println!("cargo::rerun-if-changed={}", dir.display());
    let mut files = Vec::new();
    for entry in fs::read_dir(&dir).map_err(|e| format!("{}: {e}", dir.display()))? {
        let path = entry?.path();
        if path.extension().is_some_and(|x| x == "toml") {
            let name = path
                .file_stem()
                .and_then(|s| s.to_str())
                .ok_or("provider file name is not UTF-8")?;
            files.push((name.to_owned(), path.display().to_string()));
        }
    }
    if files.is_empty() {
        return Err(format!("no provider files in {}", dir.display()).into());
    }
    files.sort();
    let mut out = String::from("pub(crate) const BUILT_IN: &[(&str, &str)] = &[\n");
    for (name, path) in files {
        writeln!(out, "    ({name:?}, include_str!({path:?})),")?;
    }
    out.push_str("];\n");
    fs::write(
        PathBuf::from(env::var("OUT_DIR")?).join("providers.rs"),
        out,
    )?;
    Ok(())
}
