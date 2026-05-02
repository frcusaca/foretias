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
    println!("cargo:rustc-link-lib=crypto");

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

    // Post-process: rename C-generated identifiers to foretias/Foretias naming
    let raw = std::fs::read_to_string(out.join("core_bindings.rs")).unwrap();
    let mut src = raw
        .replace("FortiasResult_FORTIAS_OK", "ForetiasResult_FORETIAS_OK")
        .replace("FortiasResult_FORTIAS_ERR_", "ForetiasResult_FORETIAS_ERR_")
        .replace("FortiasCurve_FORTIAS_CURVE_", "ForetiasCurve_FORETIAS_CURVE_")
        .replace("FORTIAS_CORE_VERSION_MAJOR", "FORETIAS_CORE_VERSION_MAJOR")
        .replace("FORTIAS_CORE_VERSION_MINOR", "FORETIAS_CORE_VERSION_MINOR")
        .replace("FORTIAS_NOISE_MAX_MSG", "FORETIAS_NOISE_MAX_MSG")
        .replace("FORTIAS_MERKLE_MAX_DEPTH", "FORETIAS_MERKLE_MAX_DEPTH");

    // Rename Fortias* types to Foretias* (but not Fortis which is a domain struct)
    let type_renames = [
        ("FortiasCoreVersion", "ForetiasCoreVersion"),
        ("FortiasResult", "ForetiasResult"),
        ("FortiasCurve", "ForetiasCurve"),
        ("FortiasPubKey32", "ForetiasPubKey32"),
        ("FortiasPubKey33", "ForetiasPubKey33"),
        ("FortiasPrivKey32", "ForetiasPrivKey32"),
        ("FortiasPeerID", "ForetiasPeerID"),
        ("FortiasSig64", "ForetiasSig64"),
        ("FortiasHash32", "ForetiasHash32"),
        ("FortiasHash16", "ForetiasHash16"),
        ("FortiasHash20", "ForetiasHash20"),
        ("FortiasNullifier", "ForetiasNullifier"),
        ("FortiasFrostShare", "ForetiasFrostShare"),
        ("FortiasMerkleProof", "ForetiasMerkleProof"),
        ("FortiasFrostRound1", "ForetiasFrostRound1"),
        ("FortiasPrivKey", "ForetiasPrivKey"),
        ("FortiasNoiseState", "ForetiasNoiseState"),
    ];
    for (from, to) in type_renames {
        src = src.replace(from, to);
    }

    // Add #[link_name] attributes and rename fortias_* → foretias_* function names
    let mut out_lines: Vec<String> = src.lines().map(|s| s.to_string()).collect();
    let mut final_lines = Vec::new();
    let mut i = 0;
    while i < out_lines.len() {
        let line = &out_lines[i];
        if line.contains("pub fn fortias_") {
            let indent = line.chars().take_while(|c| c.is_whitespace()).collect::<String>();
            let fn_part = &line[line.find("pub fn fortias_").unwrap()..];

            let fn_name = fn_part.trim_start()
                .strip_prefix("pub fn ").unwrap()
                .split('(').next().unwrap()
                .trim().to_string();
            let c_name = fn_name.replace("foretias_", "fortias_");
            let rust_name = format!("foretias_{}", fn_name.strip_prefix("fortias_").unwrap());

            let mut fn_lines = vec![line.clone()];
            if !line.contains(';') {
                let mut j = i + 1;
                while j < out_lines.len() && !out_lines[j].contains(';') {
                    fn_lines.push(out_lines[j].clone());
                    j += 1;
                }
                if j < out_lines.len() {
                    fn_lines.push(out_lines[j].clone());
                }
                i = j + 1;
            } else {
                i += 1;
            }

            let mut rewritten = fn_lines.join("");
            rewritten = rewritten.replace(&format!("pub fn {fn_name}("), &format!("pub fn {rust_name}("));

            final_lines.push(format!("{indent}#[link_name = \"{}\"]", c_name));
            for rw_line in rewritten.lines() {
                final_lines.push(rw_line.to_string());
            }
        } else {
            final_lines.push(line.clone());
            i += 1;
        }
    }

    let final_src = final_lines.join("\n");

    let dst = PathBuf::from(manifest).join("src/core/bindings.rs");
    std::fs::create_dir_all(dst.parent().unwrap()).ok();
    std::fs::write(&dst, final_src).unwrap();
}
