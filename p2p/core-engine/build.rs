use std::env;
use std::path::PathBuf;

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let core_dir = PathBuf::from(&manifest).join("../core");

    let sources = [
        "src/version.c",
        "src/identity_ed25519.c", "src/identity_p256.c",
        "src/signing_ed25519.c", "src/signing_p256.c",
        "src/hash_sha256.c", "src/hash_blake3.c",
        "src/hash_legacy_insecure_md5.c", "src/hash_legacy_insecure_sha1.c",
        "src/noise_xx.c", "src/merkle.c",
        "src/frost_ed25519.c", "src/nullifier.c", "src/privkey.c",
        "src/rng_mix.c", "src/memzero.c",
    ];
    for s in &sources {
        println!("cargo:rerun-if-changed={}", core_dir.join(s).display());
    }
    println!("cargo:rerun-if-changed={}/include/fortias_core.h", core_dir.display());

    let mut build = cc::Build::new();
    build.std("c11")
        .flag("-Wall").flag("-Wextra").flag("-Wpedantic")
        .flag("-O2").flag("-fno-strict-aliasing")
        .flag("-fstack-protector").flag("-fvisibility=hidden")
        .include(core_dir.join("include"))
        .include(core_dir.join("src"));
    for s in &sources {
        build.file(core_dir.join(s));
    }
    build.compile("fortias_core");
    println!("cargo:rustc-link-lib=sodium");

    let bindings = bindgen::Builder::default()
        .header(core_dir.join("include/fortias_core.h").to_str().unwrap())
        .clang_arg(format!("-I{}", core_dir.join("include").display()))
        .allowlist_type("Fortias.*")
        .allowlist_function("fortias_.*")
        .allowlist_var("FORTIAS_.*")
        .derive_debug(true).derive_copy(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("bindgen failed");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("core_bindings.rs")).unwrap();

    // Copy to src for compilation
    let dst = PathBuf::from(manifest).join("src/core/bindings.rs");
    std::fs::create_dir_all(dst.parent().unwrap()).ok();
    std::fs::write(&dst, std::fs::read_to_string(out.join("core_bindings.rs")).unwrap()).unwrap();
}
