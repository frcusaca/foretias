use std::env;
use std::path::PathBuf;
use std::process::Command;

fn main() {
    let manifest = env::var("CARGO_MANIFEST_DIR").unwrap();
    let core_dir = PathBuf::from(&manifest).join("../core");

    // ── Build liboqs from source (v0.13.0) ──────────────────────────────
    let liboqs_dir = PathBuf::from(&manifest).join("deps/liboqs");
    let liboqs_build_dir = liboqs_dir.join("build");
    let oqs_include = liboqs_build_dir.join("include");
    let oqs_lib_dir = liboqs_build_dir.join("lib");

    if !liboqs_dir.join("src").exists() {
        std::fs::create_dir_all(&liboqs_dir).ok();
        Command::new("git")
            .args(&[
                "clone", "--depth", "1", "--branch", "0.13.0",
                "https://github.com/open-quantum-safe/liboqs.git",
                liboqs_dir.to_str().unwrap(),
            ])
            .status()
            .expect("failed to clone liboqs");
    }

    std::fs::create_dir_all(&liboqs_build_dir).unwrap();

    let cmake_config_cmd = format!(
        "cmake -S {} -B {} -DCMAKE_BUILD_TYPE=Release \
         -Doqs_enable_open_ssl=OFF -Doqs_enable_s2n=OFF \
         -Denable_test=OFF -Denable_benchmark=OFF -Denable_samples=OFF \
         -DENABLE_EXPERIMENTAL=OFF -DCMAKE_POSITION_INDEPENDENT_CODE=ON",
        liboqs_dir.display(),
        liboqs_build_dir.display(),
    );
    println!("cargo:rerun-if-changed={}", liboqs_dir.join("src").display());
    println!("cargo:rerun-if-changed={}/CMakeLists.txt", liboqs_dir.display());

    Command::new("sh")
        .arg("-c")
        .arg(&cmake_config_cmd)
        .status()
        .expect("cmake configure failed");

    let parallel_jobs = env::var("CMAKE_BUILD_PARALLEL_LEVEL")
        .ok()
        .and_then(|s| s.parse::<u32>().ok())
        .unwrap_or_else(|| std::thread::available_parallelism().map(|n| n.get() as u32).unwrap_or(1));

    Command::new("cmake")
        .args(&["--build", liboqs_build_dir.to_str().unwrap(), "--config", "Release", "--parallel", &parallel_jobs.to_string()])
        .status()
        .expect("cmake build failed");

    println!("cargo:include={}", oqs_include.display());
    println!("cargo:rustc-link-search=native={}", oqs_lib_dir.display());
    println!("cargo:rustc-link-lib=oqs");

    // ── Compile C11 core ─────────────────────────────────────────────────
    let sources = [
        "src/version.c",
        "src/identity_ed25519.c", "src/identity_p256.c",
        "src/signing_ed25519.c", "src/signing_p256.c", "src/signing_sphincs.c", "src/signing_dilithium.c", "src/signing_tbid.c",
        "src/kem_mlkem.c",
        "src/hash_sha256.c", "src/hash_blake3.c",
        "src/hash_legacy_insecure_md5.c", "src/hash_legacy_insecure_sha1.c",
        "src/noise_xx.c", "src/merkle.c",
        "src/frost_ed25519.c", "src/nullifier.c", "src/privkey.c",
        "src/rng_mix.c", "src/memzero.c",
    ];
    for s in &sources {
        println!("cargo:rerun-if-changed={}", core_dir.join(s).display());
    }
    println!("cargo:rerun-if-changed={}/include/foretias_core.h", core_dir.display());
    println!("cargo:rerun-if-changed={}", core_dir.join("src/signing_tbid.c").display());

    let mut build = cc::Build::new();
    build.std("c11")
        .flag("-Wall").flag("-Wextra").flag("-Wpedantic")
        .flag("-O2").flag("-fno-strict-aliasing")
        .flag("-fstack-protector").flag("-fvisibility=hidden")
        .include(core_dir.join("include"))
        .include(core_dir.join("src"))
        .include(&oqs_include);
    for s in &sources {
        build.file(core_dir.join(s));
    }
    build.compile("foretias_core");
    println!("cargo:rustc-link-lib=sodium");
    println!("cargo:rustc-link-lib=crypto");

    let bindings = bindgen::Builder::default()
        .header(core_dir.join("include/foretias_core.h").to_str().unwrap())
        .clang_arg(format!("-I{}", core_dir.join("include").display()))
        .allowlist_type("Foretias.*")
        .allowlist_function("foretias_.*")
        .allowlist_var("FORETIAS_.*")
        .derive_debug(true).derive_copy(true)
        .parse_callbacks(Box::new(bindgen::CargoCallbacks::new()))
        .generate()
        .expect("bindgen failed");

    let out = PathBuf::from(env::var("OUT_DIR").unwrap());
    bindings.write_to_file(out.join("core_bindings.rs")).unwrap();

    // Post-process: rename C-generated identifiers to foretias/Foretias naming
    let raw = std::fs::read_to_string(out.join("core_bindings.rs")).unwrap();
    let mut src = raw
        .replace("ForetiasResult_FORETIAS_OK", "ForetiasResult_FORETIAS_OK")
        .replace("ForetiasResult_FORETIAS_ERR_", "ForetiasResult_FORETIAS_ERR_")
        .replace("ForetiasCurve_FORETIAS_CURVE_", "ForetiasCurve_FORETIAS_CURVE_")
        .replace("FORETIAS_CORE_VERSION_MAJOR", "FORETIAS_CORE_VERSION_MAJOR")
        .replace("FORETIAS_CORE_VERSION_MINOR", "FORETIAS_CORE_VERSION_MINOR")
        .replace("FORETIAS_NOISE_MAX_MSG", "FORETIAS_NOISE_MAX_MSG")
        .replace("FORETIAS_MERKLE_MAX_DEPTH", "FORETIAS_MERKLE_MAX_DEPTH");

    // Rename Foretias* types to Foretias* (but not Foretis which is a domain struct)
    let type_renames = [
        ("ForetiasCoreVersion", "ForetiasCoreVersion"),
        ("ForetiasResult", "ForetiasResult"),
        ("ForetiasCurve", "ForetiasCurve"),
        ("ForetiasPubKey32", "ForetiasPubKey32"),
        ("ForetiasPubKey33", "ForetiasPubKey33"),
        ("ForetiasPrivKey32", "ForetiasPrivKey32"),
        ("ForetiasPeerID", "ForetiasPeerID"),
        ("ForetiasSig64", "ForetiasSig64"),
        ("ForetiasHash32", "ForetiasHash32"),
        ("ForetiasHash16", "ForetiasHash16"),
        ("ForetiasHash20", "ForetiasHash20"),
        ("ForetiasNullifier", "ForetiasNullifier"),
        ("ForetiasFrostShare", "ForetiasFrostShare"),
        ("ForetiasMerkleProof", "ForetiasMerkleProof"),
        ("ForetiasFrostRound1", "ForetiasFrostRound1"),
        ("ForetiasPrivKey", "ForetiasPrivKey"),
        ("ForetiasNoiseState", "ForetiasNoiseState"),
    ];
    for (from, to) in type_renames {
        src = src.replace(from, to);
    }

    // Add #[link_name] attributes and rename foretias_* → foretias_* function names
    let mut out_lines: Vec<String> = src.lines().map(|s| s.to_string()).collect();
    let mut final_lines = Vec::new();
    let mut i = 0;
    while i < out_lines.len() {
        let line = &out_lines[i];
        if line.contains("pub fn foretias_") {
            let indent = line.chars().take_while(|c| c.is_whitespace()).collect::<String>();
            let fn_part = &line[line.find("pub fn foretias_").unwrap()..];

            let fn_name = fn_part.trim_start()
                .strip_prefix("pub fn ").unwrap()
                .split('(').next().unwrap()
                .trim().to_string();
            let c_name = fn_name.replace("foretias_", "foretias_");
            let rust_name = format!("foretias_{}", fn_name.strip_prefix("foretias_").unwrap());

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
