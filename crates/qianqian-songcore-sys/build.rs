use std::env;
use std::path::{Path, PathBuf};

// The provenance digests are computed in-process (FIPS 180-4 SHA-256, std
// only). The previous implementation shelled out to `sha256sum` and took the
// first whitespace token of its output; GNU coreutils prints the digest line
// in escaped format ("\ <digest>  <escaped-name>") whenever the file name
// contains a backslash — which is every native Windows absolute path — so
// QN_SONGCORE_HEADER_SHA256 / QN_SONGCORE_ARTIFACT_SHA256 carried a leading
// "\" on Windows (#324 observation, fixed for #172). Hashing in-process also
// removes the external-tool dependency: the build no longer fails when
// coreutils is not on PATH. No build-dependency is introduced, so the crate
// keeps building offline under any host toolchain.
//
// Fail-closed: the hasher is checked against the four NIST FIPS 180-4 test
// vectors on every build-script invocation, before any file is hashed; a
// mismatch panics the build instead of emitting a wrong provenance digest.

const SHA256_K: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn sha256(data: &[u8]) -> [u8; 32] {
    let mut state: [u32; 8] = [
        0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
        0x5be0cd19,
    ];

    // Padding: 0x80, zeros to 56 mod 64, then the 64-bit big-endian bit length.
    let bit_len = (data.len() as u64).wrapping_mul(8);
    let mut padded = data.to_vec();
    padded.push(0x80);
    while padded.len() % 64 != 56 {
        padded.push(0);
    }
    padded.extend_from_slice(&bit_len.to_be_bytes());

    for block in padded.chunks_exact(64) {
        let mut w = [0u32; 64];
        for (i, word) in w.iter_mut().enumerate().take(16) {
            *word = u32::from_be_bytes([
                block[i * 4],
                block[i * 4 + 1],
                block[i * 4 + 2],
                block[i * 4 + 3],
            ]);
        }
        for i in 16..64 {
            let s0 = w[i - 15].rotate_right(7) ^ w[i - 15].rotate_right(18) ^ (w[i - 15] >> 3);
            let s1 = w[i - 2].rotate_right(17) ^ w[i - 2].rotate_right(19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }

        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = state;
        for i in 0..64 {
            let s1 = e.rotate_right(6) ^ e.rotate_right(11) ^ e.rotate_right(25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(SHA256_K[i])
                .wrapping_add(w[i]);
            let s0 = a.rotate_right(2) ^ a.rotate_right(13) ^ a.rotate_right(22);
            let maj = (a & b) ^ (a & c) ^ (b & c);
            let t2 = s0.wrapping_add(maj);
            h = g;
            g = f;
            f = e;
            e = d.wrapping_add(t1);
            d = c;
            c = b;
            b = a;
            a = t1.wrapping_add(t2);
        }

        for (word, rotated) in state.iter_mut().zip([a, b, c, d, e, f, g, h]) {
            *word = word.wrapping_add(rotated);
        }
    }

    let mut digest = [0u8; 32];
    for (i, word) in state.iter().enumerate() {
        digest[i * 4..i * 4 + 4].copy_from_slice(&word.to_be_bytes());
    }
    digest
}

fn sha256_self_test() {
    const VECTORS: [(&[u8], &str); 4] = [
        (
            b"",
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855",
        ),
        (
            b"abc",
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad",
        ),
        (
            // 56-byte NIST example: padding must spill into a second block.
            b"abcdbcdecdefdefgefghfghighijhijkijkljklmklmnlmnomnopnopq",
            "248d6a61d20638b8e5c026930c3e6039a33ce45964ff2167f6ecedd419db06c1",
        ),
        (
            // 112-byte NIST example: two-block message.
            b"abcdefghbcdefghicdefghijdefghijkefghijklfghijklmghijklmnhijklmnoijklmnopjklmnopqklmnopqrlmnopqrsmnopqrstnopqrstu",
            "cf5b16a778af8380036ce59e7b0492370b249b11e8f07a51afac45037afee9d1",
        ),
    ];
    for (message, expected) in VECTORS {
        let hex: String = sha256(message).iter().map(|b| format!("{b:02x}")).collect();
        assert!(
            hex == expected,
            "SHA-256 self-test failed for {}-byte message: got {hex}, want {expected}",
            message.len()
        );
    }
}

fn sha256_hex(path: &Path) -> String {
    // The inputs are a header and the merged static archive (~4 MiB for the
    // FFmpeg-closure build), so reading whole is bounded and keeps the hasher
    // trivial to audit.
    let bytes = std::fs::read(path)
        .unwrap_or_else(|e| panic!("failed to read {} for hashing: {e}", path.display()));
    sha256(&bytes).iter().map(|b| format!("{b:02x}")).collect()
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
    sha256_self_test();

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
    // The merged archive is self-contained; only POSIX hosts need system
    // libs beyond it. The Windows (mingw/MSVC) artifact links none.
    if env::var("CARGO_CFG_TARGET_OS").as_deref() != Ok("windows") {
        println!("cargo:rustc-link-lib=m");
        println!("cargo:rustc-link-lib=pthread");
    }
    println!("cargo:rerun-if-changed={}", header.display());
    println!("cargo:rerun-if-changed={}", archive.display());
    println!("cargo:rerun-if-changed=build.rs");
    println!("cargo:rerun-if-env-changed=QIANQIAN_NATIVE_DIR");
}
