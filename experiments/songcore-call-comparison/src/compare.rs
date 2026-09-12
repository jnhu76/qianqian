pub fn sorted_copy(v: &[f64]) -> Vec<f64> {
    let mut s = v.to_vec();
    s.sort_by(|a, b| a.partial_cmp(b).unwrap());
    s
}

pub fn median(v: &[f64]) -> f64 {
    let s = sorted_copy(v);
    s[s.len() / 2]
}

pub fn percentile_sorted(s: &[f64], p: f64) -> f64 {
    let n = s.len();
    if n == 0 {
        return 0.0;
    }
    let idx = ((p / 100.0) * n as f64 + 0.999999) as usize;
    let idx = idx.clamp(1, n);
    s[idx - 1]
}

pub struct LatencyStats {
    pub p50: f64,
    pub p90: f64,
    pub p99: f64,
    pub max: f64,
    pub count: usize,
}

pub fn latency_stats(v: &[f64]) -> LatencyStats {
    let s = sorted_copy(v);
    LatencyStats {
        p50: percentile_sorted(&s, 50.0),
        p90: percentile_sorted(&s, 90.0),
        p99: percentile_sorted(&s, 99.0),
        max: s[s.len() - 1],
        count: s.len(),
    }
}

pub struct SteadyStats {
    pub med_c: f64,
    pub med_rust: f64,
    pub delta_pct: f64,
    pub noise_band_pct: f64,
    pub label: String,
}

pub fn steady_stats(c: &[f64], rust: &[f64]) -> SteadyStats {
    let cs = sorted_copy(c);
    let rs = sorted_copy(rust);
    let med_c = cs[cs.len() / 2];
    let med_rust = rs[rs.len() / 2];
    let delta_pct = (med_rust - med_c) / med_c * 100.0;
    let iqr_c = percentile_sorted(&cs, 75.0) - percentile_sorted(&cs, 25.0);
    let iqr_rust = percentile_sorted(&rs, 75.0) - percentile_sorted(&rs, 25.0);
    let noise_band_pct = (iqr_c + iqr_rust) / (2.0 * med_c) * 100.0;
    let label = if delta_pct > 5.0 {
        "INVESTIGATE".to_string()
    } else if delta_pct.abs() <= noise_band_pct {
        "NO_MEASURABLE_FFI_TAX".to_string()
    } else {
        format!("MEASURABLE_FFI_TAX = {:+.2}%", delta_pct)
    };
    SteadyStats {
        med_c,
        med_rust,
        delta_pct,
        noise_band_pct,
        label,
    }
}
