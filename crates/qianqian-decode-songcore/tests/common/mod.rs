//! Test helpers: reference-manifest loading and a self-contained SHA-256
//! (same implementation lineage as the experiment harness; no external
//! dependency).

use std::path::Path;

pub struct ReferenceFixture {
    pub file: String,
    pub sample_rate: u32,
    pub channels: u32,
    pub pcm_frames: usize,
    pub pcm_sha256: String,
    fixture_sha256: String,
}

impl ReferenceFixture {
    pub fn load(reference_json: &Path, id: &str) -> Self {
        let raw = std::fs::read_to_string(reference_json)
            .unwrap_or_else(|e| panic!("reference manifest {}: {e}", reference_json.display()));
        let value: serde_json::Value = serde_json::from_str(&raw).expect("reference JSON");
        let entry = value["fixtures"]
            .as_array()
            .expect("fixtures array")
            .iter()
            .find(|f| f["id"] == id)
            .unwrap_or_else(|| panic!("fixture '{id}' not in reference manifest"));
        Self {
            file: entry["file"].as_str().expect("file").to_owned(),
            sample_rate: entry["sample_rate"].as_u64().expect("sample_rate") as u32,
            channels: entry["channels"].as_u64().expect("channels") as u32,
            pcm_frames: entry["pcm_frames"].as_u64().expect("pcm_frames") as usize,
            pcm_sha256: entry["pcm_sha256"].as_str().expect("pcm_sha256").to_owned(),
            fixture_sha256: entry["fixture_sha256"]
                .as_str()
                .expect("fixture_sha256")
                .to_owned(),
        }
    }

    /// Fail the test before any decode assertion if the corpus file itself
    /// drifted from the reference it anchors.
    pub fn verify_file_identity(&self, path: &Path) {
        let bytes =
            std::fs::read(path).unwrap_or_else(|e| panic!("fixture {}: {e}", path.display()));
        let mut hasher = Sha256::new();
        hasher.update(&bytes);
        assert_eq!(
            hasher.hex(),
            self.fixture_sha256,
            "fixture file drifted from reference identity: {}",
            path.display()
        );
    }
}

// --- minimal SHA-256 (FIPS 180-4) ----------------------------------------

const K256: [u32; 64] = [
    0x428a2f98, 0x71374491, 0xb5c0fbcf, 0xe9b5dba5, 0x3956c25b, 0x59f111f1, 0x923f82a4, 0xab1c5ed5,
    0xd807aa98, 0x12835b01, 0x243185be, 0x550c7dc3, 0x72be5d74, 0x80deb1fe, 0x9bdc06a7, 0xc19bf174,
    0xe49b69c1, 0xefbe4786, 0x0fc19dc6, 0x240ca1cc, 0x2de92c6f, 0x4a7484aa, 0x5cb0a9dc, 0x76f988da,
    0x983e5152, 0xa831c66d, 0xb00327c8, 0xbf597fc7, 0xc6e00bf3, 0xd5a79147, 0x06ca6351, 0x14292967,
    0x27b70a85, 0x2e1b2138, 0x4d2c6dfc, 0x53380d13, 0x650a7354, 0x766a0abb, 0x81c2c92e, 0x92722c85,
    0xa2bfe8a1, 0xa81a664b, 0xc24b8b70, 0xc76c51a3, 0xd192e819, 0xd6990624, 0xf40e3585, 0x106aa070,
    0x19a4c116, 0x1e376c08, 0x2748774c, 0x34b0bcb5, 0x391c0cb3, 0x4ed8aa4a, 0x5b9cca4f, 0x682e6ff3,
    0x748f82ee, 0x78a5636f, 0x84c87814, 0x8cc70208, 0x90befffa, 0xa4506ceb, 0xbef9a3f7, 0xc67178f2,
];

fn ror(x: u32, n: u32) -> u32 {
    (x >> n) | (x << (32 - n))
}

pub struct Sha256 {
    h: [u32; 8],
    len: u64,
    buf: [u8; 64],
    buf_len: usize,
}

impl Sha256 {
    pub fn new() -> Sha256 {
        Sha256 {
            h: [
                0x6a09e667, 0xbb67ae85, 0x3c6ef372, 0xa54ff53a, 0x510e527f, 0x9b05688c, 0x1f83d9ab,
                0x5be0cd19,
            ],
            len: 0,
            buf: [0; 64],
            buf_len: 0,
        }
    }

    fn compress(&mut self, p: &[u8]) {
        let mut w = [0u32; 64];
        for i in 0..16 {
            w[i] = ((p[4 * i] as u32) << 24)
                | ((p[4 * i + 1] as u32) << 16)
                | ((p[4 * i + 2] as u32) << 8)
                | (p[4 * i + 3] as u32);
        }
        for i in 16..64 {
            let s0 = ror(w[i - 15], 7) ^ ror(w[i - 15], 18) ^ (w[i - 15] >> 3);
            let s1 = ror(w[i - 2], 17) ^ ror(w[i - 2], 19) ^ (w[i - 2] >> 10);
            w[i] = w[i - 16]
                .wrapping_add(s0)
                .wrapping_add(w[i - 7])
                .wrapping_add(s1);
        }
        let [mut a, mut b, mut c, mut d, mut e, mut f, mut g, mut h] = self.h;
        for i in 0..64 {
            let s1 = ror(e, 6) ^ ror(e, 11) ^ ror(e, 25);
            let ch = (e & f) ^ ((!e) & g);
            let t1 = h
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K256[i])
                .wrapping_add(w[i]);
            let s0 = ror(a, 2) ^ ror(a, 13) ^ ror(a, 22);
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
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(c);
        self.h[3] = self.h[3].wrapping_add(d);
        self.h[4] = self.h[4].wrapping_add(e);
        self.h[5] = self.h[5].wrapping_add(f);
        self.h[6] = self.h[6].wrapping_add(g);
        self.h[7] = self.h[7].wrapping_add(h);
    }

    pub fn update(&mut self, mut data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        while !data.is_empty() {
            if self.buf_len == 0 && data.len() >= 64 {
                let (block, rest) = data.split_at(64);
                self.compress(block);
                data = rest;
            } else {
                let take = (64 - self.buf_len).min(data.len());
                self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&data[..take]);
                self.buf_len += take;
                data = &data[take..];
                if self.buf_len == 64 {
                    let block = self.buf;
                    self.compress(&block);
                    self.buf_len = 0;
                }
            }
        }
    }

    pub fn hex(mut self) -> String {
        let bit_len = self.len.wrapping_mul(8);
        let mut padded = self.buf[..self.buf_len].to_vec();
        padded.push(0x80);
        while padded.len() % 64 != 56 {
            padded.push(0);
        }
        padded.extend_from_slice(&bit_len.to_be_bytes());
        for block in padded.chunks(64) {
            self.compress(block);
        }
        let mut s = String::with_capacity(64);
        for v in self.h {
            s.push_str(&format!("{v:08x}"));
        }
        s
    }
}

#[cfg(test)]
mod tests {
    use super::Sha256;

    #[test]
    fn matches_known_vectors() {
        let mut h = Sha256::new();
        h.update(b"");
        assert_eq!(
            h.hex(),
            "e3b0c44298fc1c149afbf4c8996fb92427ae41e4649b934ca495991b7852b855"
        );

        let mut h = Sha256::new();
        h.update(b"abc");
        assert_eq!(
            h.hex(),
            "ba7816bf8f01cfea414140de5dae2223b00361a396177a9cb410ff61f20015ad"
        );

        // Multi-update with a length that crosses block boundaries.
        let mut h = Sha256::new();
        let data = vec![0x61u8; 1_000_000];
        h.update(&data[..999_937]);
        h.update(&data[999_937..]);
        assert_eq!(
            h.hex(),
            "cdc76e5c9914fb9281a1c7e284d73e67f1809a48a497200e046d39ccc7112cd0"
        );
    }
}
