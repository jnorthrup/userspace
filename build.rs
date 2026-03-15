use std::env;
use std::path::PathBuf;

fn main() {
    // Only run bindgen when the `mlir` cargo feature is enabled.
    // Cargo exposes features to build scripts via environment variables
    // as CARGO_FEATURE_<NAME> where NAME is uppercased and '-' -> '_'.
    if env::var("CARGO_FEATURE_MLIR").is_err() {
        println!("cargo:warning=MLIR feature not enabled; skipping bindgen");
        return;
    }

    // Re-run build if these env vars change
    println!("cargo:rerun-if-env-changed=MLIR_HEADER");
    println!("cargo:rerun-if-env-changed=MLIR_INCLUDE");

    // MLIR_HEADER must point to an MLIR C header file (e.g. /usr/local/include/mlir-c/IR.h)
    // MLIR_INCLUDE may be set to the directory containing MLIR headers (optional).
    let header = env::var("MLIR_HEADER").expect(
        "Set MLIR_HEADER to path to an MLIR C header (e.g. /usr/local/include/mlir-c/IR.h)",
    );

    let include = env::var("MLIR_INCLUDE").unwrap_or_else(|_| {
        PathBuf::from(&header)
            .parent()
            .map(|p| p.to_string_lossy().into_owned())
            .unwrap_or_else(|| "/usr/include".into())
    });

    // Generate bindings with bindgen. This requires bindgen in build-dependencies.
    let bindings = bindgen::Builder::default()
        .header(header)
        .clang_arg(format!("-I{}", include))
        .generate_comments(false)
        .generate()
        .expect("Unable to generate MLIR bindings");

    let out_path = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings
        .write_to_file(out_path.join("mlir_bindings.rs"))
        .expect("Couldn't write MLIR bindings");
}