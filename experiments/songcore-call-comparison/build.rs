use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn run(prog: &str, args: &[&str]) {
    let status = Command::new(prog)
        .args(args)
        .status()
        .unwrap_or_else(|e| panic!("failed to run {}: {}", prog, e));
    if !status.success() {
        panic!("{} {:?} exited with {}", prog, args, status);
    }
}

fn find_native_dir(start: &Path) -> Option<PathBuf> {
    let mut dir = Some(start.to_path_buf());
    while let Some(d) = dir {
        let candidate = d.join("native");
        if candidate
            .join("build")
            .join("artifacts")
            .join("libsongcore.a")
            .exists()
        {
            return Some(candidate);
        }
        dir = d.parent().map(|p| p.to_path_buf());
    }
    None
}

fn main() {
    let manifest_dir = PathBuf::from(env::var("CARGO_MANIFEST_DIR").unwrap());
    let native_dir = match env::var("QIANQIAN_NATIVE_DIR") {
        Ok(p) => PathBuf::from(p),
        Err(_) => match find_native_dir(&manifest_dir) {
            Some(d) => d,
            None => {
                panic!(
                    "SongCore native tree not found from {}; set QIANQIAN_NATIVE_DIR or build the native artifact first: cd native && xmake ffmpeg-import && xmake f -m release -y && xmake build songcore",
                    manifest_dir.display()
                );
            }
        },
    };

    let include_dir = native_dir.join("include");
    let artifacts_dir = native_dir.join("build").join("artifacts");
    let archive = artifacts_dir.join("libsongcore.a");
    if !archive.exists() {
        panic!(
            "SongCore static artifact not found at {}; build it first: cd native && xmake f -m release -y && xmake build songcore",
            archive.display()
        );
    }

    let out_dir = PathBuf::from(env::var("OUT_DIR").unwrap());
    let cc = env::var("CC").unwrap_or_else(|_| "gcc".to_string());
    let c_dir = manifest_dir.join("c");
    let include_flag = format!("-I{}", include_dir.display());
    let out_layout = out_dir.join("layout_probe");
    let out_obj = out_dir.join("caller_c.o");
    let out_archive = out_dir.join("libcaller_c.a");
    let out_bin = out_dir.join("caller_bin");

    run(
        &cc,
        &[
            "-O2",
            "-std=c11",
            &include_flag,
            c_dir.join("layout_probe.c").to_str().unwrap(),
            "-o",
            out_layout.to_str().unwrap(),
        ],
    );
    run(
        &cc,
        &[
            "-O2",
            "-std=c11",
            &include_flag,
            "-DCALLER_NO_MAIN",
            "-c",
            c_dir.join("caller.c").to_str().unwrap(),
            "-o",
            out_obj.to_str().unwrap(),
        ],
    );
    run(
        "ar",
        &[
            "rcs",
            out_archive.to_str().unwrap(),
            out_obj.to_str().unwrap(),
        ],
    );
    run(
        &cc,
        &[
            "-O2",
            "-std=c11",
            &include_flag,
            c_dir.join("caller.c").to_str().unwrap(),
            "-L",
            artifacts_dir.to_str().unwrap(),
            "-lsongcore",
            "-lm",
            "-lpthread",
            "-o",
            out_bin.to_str().unwrap(),
        ],
    );

    println!(
        "cargo:rustc-env=QN_LAYOUT_PROBE_BIN={}",
        out_layout.display()
    );
    println!("cargo:rustc-env=QN_CALLER_BIN={}", out_bin.display());
    println!("cargo:rustc-link-search=native={}", out_dir.display());
    println!("cargo:rustc-link-search=native={}", artifacts_dir.display());
    println!("cargo:rustc-link-lib=static=caller_c");
    println!("cargo:rustc-link-lib=static=songcore");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rerun-if-changed=build.rs");
    println!(
        "cargo:rerun-if-changed={}",
        c_dir.join("layout_probe.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        c_dir.join("caller.c").display()
    );
    println!(
        "cargo:rerun-if-changed={}",
        include_dir.join("songcore.h").display()
    );
    println!("cargo:rerun-if-changed={}", archive.display());
    println!("cargo:rerun-if-env-changed=QIANQIAN_NATIVE_DIR");
}
