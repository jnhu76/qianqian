use std::env;
use std::path::{Path, PathBuf};
use std::process::Command;

fn sha256_hex(path: &Path) -> String {
    let out = Command::new("sha256sum")
        .arg(path)
        .output()
        .expect("sha256sum failed");
    String::from_utf8(out.stdout)
        .expect("sha256sum output not utf8")
        .split_whitespace()
        .next()
        .expect("sha256sum output malformed")
        .to_string()
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

    let header = native_dir.join("include").join("songcore.h");
    let artifacts_dir = native_dir.join("build").join("artifacts");
    let archive = artifacts_dir.join("libsongcore.a");
    if !archive.exists() {
        panic!(
            "SongCore static artifact not found at {}; build it first: cd native && xmake f -m release -y && xmake build songcore",
            archive.display()
        );
    }
    if !header.exists() {
        panic!("SongCore header not found at {}", header.display());
    }

    let header_sha = sha256_hex(&header);
    let archive_sha = sha256_hex(&archive);

    println!("cargo:rustc-env=QN_SONGCORE_HEADER_SHA256={}", header_sha);
    println!(
        "cargo:rustc-env=QN_SONGCORE_ARTIFACT_SHA256={}",
        archive_sha
    );
    println!("cargo:rustc-link-search=native={}", artifacts_dir.display());
    println!("cargo:rustc-link-lib=static=songcore");
    println!("cargo:rustc-link-lib=m");
    println!("cargo:rustc-link-lib=pthread");
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", archive.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=QIANQIAN_NATIVE_DIR");
}
