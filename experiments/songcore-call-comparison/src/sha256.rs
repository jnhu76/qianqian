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
        let mut a = self.h[0];
        let mut b = self.h[1];
        let mut d = self.h[2];
        let mut e = self.h[3];
        let mut f = self.h[4];
        let mut g = self.h[5];
        let mut hh = self.h[6];
        let mut i = self.h[7];
        for t in 0..64 {
            let s1 = ror(f, 6) ^ ror(f, 11) ^ ror(f, 25);
            let ch = (f & g) ^ (!f & hh);
            let t1 = i
                .wrapping_add(s1)
                .wrapping_add(ch)
                .wrapping_add(K256[t])
                .wrapping_add(w[t]);
            let s0 = ror(a, 2) ^ ror(a, 13) ^ ror(a, 22);
            let mj = (a & b) ^ (a & d) ^ (b & d);
            let t2 = s0.wrapping_add(mj);
            i = hh;
            hh = g;
            g = f;
            f = e.wrapping_add(t1);
            e = d;
            d = b;
            b = a;
            a = t1.wrapping_add(t2);
        }
        self.h[0] = self.h[0].wrapping_add(a);
        self.h[1] = self.h[1].wrapping_add(b);
        self.h[2] = self.h[2].wrapping_add(d);
        self.h[3] = self.h[3].wrapping_add(e);
        self.h[4] = self.h[4].wrapping_add(f);
        self.h[5] = self.h[5].wrapping_add(g);
        self.h[6] = self.h[6].wrapping_add(hh);
        self.h[7] = self.h[7].wrapping_add(i);
    }

    pub fn update(&mut self, data: &[u8]) {
        self.len = self.len.wrapping_add(data.len() as u64);
        let mut p = data;
        while !p.is_empty() {
            let take = (64 - self.buf_len).min(p.len());
            self.buf[self.buf_len..self.buf_len + take].copy_from_slice(&p[..take]);
            self.buf_len += take;
            p = &p[take..];
            if self.buf_len == 64 {
                let block = self.buf;
                self.compress(&block);
                self.buf_len = 0;
            }
        }
    }

    pub fn finish_hex(&mut self) -> String {
        let bits = self.len.wrapping_mul(8);
        let pad = [0x80u8];
        self.update(&pad);
        let zero = [0u8];
        while self.buf_len != 56 {
            self.update(&zero);
        }
        let mut tail = [0u8; 8];
        for i in 0..8 {
            tail[i] = (bits >> (56 - 8 * i)) as u8;
        }
        self.buf_len = 56;
        self.update(&tail);
        let mut out = String::with_capacity(64);
        for i in 0..8 {
            out.push_str(&format!("{:08x}", self.h[i]));
        }
        out
    }
}
