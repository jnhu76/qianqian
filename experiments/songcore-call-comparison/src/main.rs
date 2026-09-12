mod compare;
mod layout;
mod rust_caller;
mod sha256;

use std::env;
use std::ffi::{CStr, CString, c_char, c_int};
use std::path::PathBuf;
use std::process::Command;

use serde_json::{Value, json};

use qianqian_songcore_sys::{QN_SONGCORE_ARTIFACT_SHA256, QN_SONGCORE_HEADER_SHA256, SONG_EOF};

unsafe extern "C" {
    fn caller_correct(
        path: *const c_char,
        block: u64,
        out_frames: *mut u64,
        out_terminal: *mut c_int,
        out_sha_hex: *mut c_char,
    ) -> c_int;
    fn caller_surface(
        path: *const c_char,
        out_select_status: *mut c_int,
        out_seek_status: *mut c_int,
        out_seek_actual_us: *mut i64,
        out_first_frames: *mut u64,
    ) -> c_int;
    fn caller_steady(
        path: *const c_char,
        block: u64,
        out_wall_us: *mut i64,
        out_frames: *mut u64,
        out_terminal: *mut c_int,
    ) -> c_int;
    fn caller_latency(
        path: *const c_char,
        block: u64,
        call_us: *mut f64,
        call_cap: i32,
        out_call_count: *mut i32,
        out_terminal: *mut c_int,
    ) -> c_int;
    fn caller_ttfp(
        path: *const c_char,
        first_block: u64,
        out_open_us: *mut i64,
        out_probe_us: *mut i64,
        out_first_us: *mut i64,
    ) -> c_int;
    fn caller_floor(iterations: i64, out_wall_ns: *mut i64) -> c_int;
}

struct Fixture {
    id: String,
    file: String,
    pcm_sha256: String,
    pcm_frames: u64,
    codec: String,
}

fn repo_root() -> PathBuf {
    PathBuf::from(env!("CARGO_MANIFEST_DIR"))
        .parent()
        .unwrap()
        .parent()
        .unwrap()
        .to_path_buf()
}

fn fixture_path(fx: &Fixture) -> PathBuf {
    repo_root()
        .join("native/experiments/songcore-equivalence/fixtures")
        .join(&fx.file)
}

fn path_cstring(path: &PathBuf) -> CString {
    CString::new(path.to_str().unwrap()).unwrap()
}

fn load_fixtures() -> Vec<Fixture> {
    let ref_path = repo_root().join("native/experiments/songcore-equivalence/reference.json");
    let txt = std::fs::read_to_string(&ref_path)
        .unwrap_or_else(|e| panic!("cannot read {}: {}", ref_path.display(), e));
    let v: Value = serde_json::from_str(&txt)
        .unwrap_or_else(|e| panic!("cannot parse {}: {}", ref_path.display(), e));
    v["fixtures"]
        .as_array()
        .expect("reference fixtures")
        .iter()
        .map(|f| Fixture {
            id: f["id"].as_str().unwrap().to_string(),
            file: f["file"].as_str().unwrap().to_string(),
            pcm_sha256: f["pcm_sha256"].as_str().unwrap().to_string(),
            pcm_frames: f["pcm_frames"].as_u64().unwrap(),
            codec: f["codec"].as_str().unwrap().to_string(),
        })
        .collect()
}

fn selected_fixtures(args: &[String]) -> Vec<Fixture> {
    let all = load_fixtures();
    if args.is_empty() {
        return all;
    }
    let selected: Vec<Fixture> = all
        .into_iter()
        .filter(|f| args.iter().any(|a| a == &f.id))
        .collect();
    if selected.is_empty() {
        panic!("no fixture id matches {:?}", args);
    }
    selected
}

fn capture(prog: &str, args: &[&str]) -> String {
    Command::new(prog)
        .args(args)
        .output()
        .ok()
        .map(|o| String::from_utf8_lossy(&o.stdout).trim().to_string())
        .unwrap_or_default()
}

fn host_info() -> Value {
    let cpuinfo = std::fs::read_to_string("/proc/cpuinfo").unwrap_or_default();
    let cpu_model = cpuinfo
        .lines()
        .find(|l| l.starts_with("model name"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let kernel = std::fs::read_to_string("/proc/sys/kernel/osrelease")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let loadavg = std::fs::read_to_string("/proc/loadavg")
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    let status = std::fs::read_to_string("/proc/self/status").unwrap_or_default();
    let affinity = status
        .lines()
        .find(|l| l.starts_with("Cpus_allowed_list"))
        .and_then(|l| l.split(':').nth(1))
        .map(|s| s.trim().to_string())
        .unwrap_or_default();
    json!({
        "cpu_model": cpu_model,
        "kernel": kernel,
        "loadavg": loadavg,
        "affinity_cpus": affinity,
        "logical_cpus": std::thread::available_parallelism().map(|n| n.get()).unwrap_or(0),
        "rustc": capture("rustc", &["--version"]),
        "cargo": capture("cargo", &["--version"]),
        "c_compiler": capture("gcc", &["--version"]).lines().next().unwrap_or("").to_string(),
        "c_opt": "-O2",
        "rust_profile": "release, opt-level 3, no special LTO",
    })
}

fn run_layout(results_dir: Option<&PathBuf>) -> (bool, Value) {
    let c_out = Command::new(env!("QN_LAYOUT_PROBE_BIN"))
        .output()
        .expect("layout probe failed to start");
    if !c_out.status.success() {
        panic!("c layout probe exited with {}", c_out.status);
    }
    let c_probe = String::from_utf8_lossy(&c_out.stdout).to_string();
    let rust_probe = layout::emit();
    let pass = c_probe.trim_end() == rust_probe.trim_end();
    if let Some(dir) = results_dir {
        std::fs::write(dir.join("layout-c.txt"), &c_probe).unwrap();
        std::fs::write(dir.join("layout-rust.txt"), &rust_probe).unwrap();
    }
    if !pass {
        let c_lines: Vec<&str> = c_probe.trim_end().lines().collect();
        let r_lines: Vec<&str> = rust_probe.trim_end().lines().collect();
        eprintln!("LAYOUT MISMATCH");
        for i in 0..c_lines.len().max(r_lines.len()) {
            let cl = c_lines.get(i).copied().unwrap_or("<missing>");
            let rl = r_lines.get(i).copied().unwrap_or("<missing>");
            if cl != rl {
                eprintln!("  line {}: C[{}] vs Rust[{}]", i + 1, cl, rl);
            }
        }
    }
    let v = json!({
        "pass": pass,
        "c_probe_lines": c_probe.trim_end().lines().count(),
        "rust_probe_lines": rust_probe.trim_end().lines().count(),
    });
    (pass, v)
}

fn c_correct(cstr: &CString, block: u64) -> (u64, i32, String) {
    let mut frames: u64 = 0;
    let mut terminal: i32 = 0;
    let mut sha_buf = [0 as c_char; 65];
    let rc = unsafe {
        caller_correct(
            cstr.as_ptr(),
            block,
            &mut frames,
            &mut terminal,
            sha_buf.as_mut_ptr(),
        )
    };
    if rc != 0 {
        panic!("c correct decode failed");
    }
    let sha = unsafe { CStr::from_ptr(sha_buf.as_ptr()) }
        .to_str()
        .unwrap()
        .to_string();
    (frames, terminal, sha)
}

fn run_correct(fixtures: &[Fixture]) -> (bool, Value) {
    let mut pass = true;
    let mut per_fixture = serde_json::Map::new();
    for fx in fixtures {
        let path = fixture_path(fx);
        let cstr = path_cstring(&path);
        let (c_frames, c_terminal, c_sha) = c_correct(&cstr, 1024);
        let r = rust_caller::correct(&path, 1024)
            .unwrap_or_else(|e| panic!("rust correct decode failed: {}", e));
        let c_ok =
            c_frames == fx.pcm_frames && c_terminal == SONG_EOF as i32 && c_sha == fx.pcm_sha256;
        let r_ok = r.frames == fx.pcm_frames
            && r.terminal == SONG_EOF as i32
            && r.sha_hex == fx.pcm_sha256;
        let equal = c_frames == r.frames && c_terminal == r.terminal && c_sha == r.sha_hex;
        let fx_pass = c_ok && r_ok && equal;
        pass = pass && fx_pass;
        println!(
            "correct {} codec={} frames={}/{} terminal_c={} terminal_rust={} sha_c={} sha_rust={} {}",
            fx.id,
            fx.codec,
            c_frames,
            fx.pcm_frames,
            c_terminal,
            r.terminal,
            c_sha,
            r.sha_hex,
            if fx_pass { "PASS" } else { "FAIL" }
        );
        per_fixture.insert(
            fx.id.clone(),
            json!({
                "pass": fx_pass,
                "expected": {"pcm_sha256": fx.pcm_sha256, "pcm_frames": fx.pcm_frames},
                "c": {"frames": c_frames, "terminal": c_terminal, "pcm_sha256": c_sha},
                "rust": {"frames": r.frames, "terminal": r.terminal, "pcm_sha256": r.sha_hex},
            }),
        );
    }
    (pass, json!({"pass": pass, "fixtures": per_fixture}))
}

fn run_surface(fixtures: &[Fixture]) -> (bool, Value) {
    let mut pass = true;
    let mut per_fixture = serde_json::Map::new();
    for fx in fixtures {
        let path = fixture_path(fx);
        let cstr = path_cstring(&path);
        let mut sel_c: i32 = 0;
        let mut seek_c: i32 = 0;
        let mut actual_c: i64 = 0;
        let mut frames_c: u64 = 0;
        let rc = unsafe {
            caller_surface(
                cstr.as_ptr(),
                &mut sel_c,
                &mut seek_c,
                &mut actual_c,
                &mut frames_c,
            )
        };
        if rc != 0 {
            panic!("c surface failed on {}", fx.id);
        }
        let r = rust_caller::surface(&path)
            .unwrap_or_else(|e| panic!("rust surface failed on {}: {}", fx.id, e));
        let equal = sel_c == r.select_status
            && seek_c == r.seek_status
            && actual_c == r.seek_actual_us
            && frames_c == r.first_frames;
        pass = pass && equal;
        println!(
            "surface {} select={}/{} seek={}/{} actual_us={}/{} first_frames={}/{} {}",
            fx.id,
            sel_c,
            r.select_status,
            seek_c,
            r.seek_status,
            actual_c,
            r.seek_actual_us,
            frames_c,
            r.first_frames,
            if equal { "PASS" } else { "FAIL" }
        );
        per_fixture.insert(
            fx.id.clone(),
            json!({
                "pass": equal,
                "c": {"select_status": sel_c, "seek_status": seek_c, "seek_actual_us": actual_c, "first_frames": frames_c},
                "rust": {"select_status": r.select_status, "seek_status": r.seek_status, "seek_actual_us": r.seek_actual_us, "first_frames": r.first_frames},
            }),
        );
    }
    (pass, json!({"pass": pass, "fixtures": per_fixture}))
}

fn c_steady(cstr: &CString, block: u64) -> (f64, u64, i32) {
    let mut wall: i64 = 0;
    let mut frames: u64 = 0;
    let mut terminal: i32 = 0;
    let rc = unsafe { caller_steady(cstr.as_ptr(), block, &mut wall, &mut frames, &mut terminal) };
    if rc != 0 {
        panic!("c steady decode failed");
    }
    (wall as f64, frames, terminal)
}

fn gate_sample(fx: &Fixture, side: &str, frames: u64, terminal: i32) -> Result<(), String> {
    if frames != fx.pcm_frames || terminal != SONG_EOF as i32 {
        return Err(format!(
            "performance sample rejected on {} for {}: frames={} (expected {}), terminal={} (expected {})",
            side, fx.id, frames, fx.pcm_frames, terminal, SONG_EOF
        ));
    }
    Ok(())
}

fn run_steady(fixtures: &[Fixture], block: u64, warmup: i32, iters: i32) -> (Value, Vec<String>) {
    let mut labels: Vec<String> = Vec::new();
    let mut per_fixture = serde_json::Map::new();
    for fx in fixtures {
        let path = fixture_path(fx);
        let cstr = path_cstring(&path);
        let mut c_wall: Vec<f64> = Vec::new();
        let mut r_wall: Vec<f64> = Vec::new();
        let mut i = -warmup;
        while i < iters {
            let c_first = i.rem_euclid(2) == 0;
            let (c, r) = if c_first {
                let c = c_steady(&cstr, block);
                gate_sample(fx, "c", c.1, c.2).unwrap_or_else(|e| panic!("{}", e));
                let r = rust_caller::steady(&path, block)
                    .unwrap_or_else(|e| panic!("rust steady failed: {}", e));
                gate_sample(fx, "rust", r.frames, r.terminal).unwrap_or_else(|e| panic!("{}", e));
                (c, r)
            } else {
                let r = rust_caller::steady(&path, block)
                    .unwrap_or_else(|e| panic!("rust steady failed: {}", e));
                gate_sample(fx, "rust", r.frames, r.terminal).unwrap_or_else(|e| panic!("{}", e));
                let c = c_steady(&cstr, block);
                gate_sample(fx, "c", c.1, c.2).unwrap_or_else(|e| panic!("{}", e));
                (c, r)
            };
            if i >= 0 {
                c_wall.push(c.0);
                r_wall.push(r.wall_us);
            }
            i += 1;
        }
        let stats = compare::steady_stats(&c_wall, &r_wall);
        println!(
            "steady {} block={} c_median_us={:.3} rust_median_us={:.3} delta={:+.2}% noise_band={:.2}% {}",
            fx.id,
            block,
            stats.med_c,
            stats.med_rust,
            stats.delta_pct,
            stats.noise_band_pct,
            stats.label
        );
        labels.push(stats.label.clone());
        per_fixture.insert(
            fx.id.clone(),
            json!({
                "block_frames": block,
                "warmup": warmup,
                "iterations": iters,
                "c_wall_us_all": c_wall,
                "rust_wall_us_all": r_wall,
                "c_median_us": stats.med_c,
                "rust_median_us": stats.med_rust,
                "delta_pct": stats.delta_pct,
                "noise_band_pct": stats.noise_band_pct,
                "verdict": stats.label,
            }),
        );
    }
    (
        json!({"block_frames": block, "warmup": warmup, "iterations": iters, "fixtures": per_fixture}),
        labels,
    )
}

fn c_latency(cstr: &CString, block: u64, buf: &mut [f64]) -> (i32, usize) {
    let mut count: i32 = 0;
    let mut terminal: i32 = 0;
    let rc = unsafe {
        caller_latency(
            cstr.as_ptr(),
            block,
            buf.as_mut_ptr(),
            buf.len() as i32,
            &mut count,
            &mut terminal,
        )
    };
    if rc != 0 {
        panic!("c latency pass failed");
    }
    (terminal, count as usize)
}

fn run_latency(fixtures: &[Fixture], block: u64, warmup: i32, passes: i32) -> Value {
    let mut per_fixture = serde_json::Map::new();
    for fx in fixtures {
        let path = fixture_path(fx);
        let cstr = path_cstring(&path);
        let mut c_calls: Vec<f64> = Vec::new();
        let mut r_calls: Vec<f64> = Vec::new();
        let mut c_buf: Vec<f64> = vec![0.0; 1 << 20];
        let mut i = -warmup;
        while i < passes {
            let c_first = i.rem_euclid(2) == 0;
            if c_first {
                let (term, n) = c_latency(&cstr, block, &mut c_buf);
                if term != SONG_EOF as i32 {
                    panic!("c latency terminal {} on {}", term, fx.id);
                }
                if i >= 0 {
                    c_calls.extend_from_slice(&c_buf[..n]);
                }
                let (term, calls) =
                    rust_caller::latency(&path, block).unwrap_or_else(|e| panic!("{}", e));
                if term != SONG_EOF as i32 {
                    panic!("rust latency terminal {} on {}", term, fx.id);
                }
                if i >= 0 {
                    r_calls.extend(calls);
                }
            } else {
                let (term, calls) =
                    rust_caller::latency(&path, block).unwrap_or_else(|e| panic!("{}", e));
                if term != SONG_EOF as i32 {
                    panic!("rust latency terminal {} on {}", term, fx.id);
                }
                if i >= 0 {
                    r_calls.extend(calls);
                }
                let (term, n) = c_latency(&cstr, block, &mut c_buf);
                if term != SONG_EOF as i32 {
                    panic!("c latency terminal {} on {}", term, fx.id);
                }
                if i >= 0 {
                    c_calls.extend_from_slice(&c_buf[..n]);
                }
            }
            i += 1;
        }
        let cs = compare::latency_stats(&c_calls);
        let rs = compare::latency_stats(&r_calls);
        println!(
            "latency {} block={} calls={}/{} c_p99_us={:.3} rust_p99_us={:.3} c_max_us={:.3} rust_max_us={:.3}",
            fx.id, block, cs.count, rs.count, cs.p99, rs.p99, cs.max, rs.max
        );
        per_fixture.insert(
            fx.id.clone(),
            json!({
                "block_frames": block,
                "warmup": warmup,
                "passes": passes,
                "c": {"p50_us": cs.p50, "p90_us": cs.p90, "p99_us": cs.p99, "max_us": cs.max, "count": cs.count},
                "rust": {"p50_us": rs.p50, "p90_us": rs.p90, "p99_us": rs.p99, "max_us": rs.max, "count": rs.count},
            }),
        );
    }
    json!({"block_frames": block, "warmup": warmup, "passes": passes, "fixtures": per_fixture})
}

fn c_ttfp(cstr: &CString, first_block: u64) -> (f64, f64, f64) {
    let mut open_us: i64 = 0;
    let mut probe_us: i64 = 0;
    let mut first_us: i64 = 0;
    let rc = unsafe {
        caller_ttfp(
            cstr.as_ptr(),
            first_block,
            &mut open_us,
            &mut probe_us,
            &mut first_us,
        )
    };
    if rc != 0 {
        panic!("c ttfp failed");
    }
    (open_us as f64, probe_us as f64, first_us as f64)
}

fn run_ttfp(fixtures: &[Fixture], iters: i32) -> Value {
    let mut per_fixture = serde_json::Map::new();
    for fx in fixtures {
        let path = fixture_path(fx);
        let cstr = path_cstring(&path);
        let mut c_open: Vec<f64> = Vec::new();
        let mut c_probe: Vec<f64> = Vec::new();
        let mut c_first: Vec<f64> = Vec::new();
        let mut r_open: Vec<f64> = Vec::new();
        let mut r_probe: Vec<f64> = Vec::new();
        let mut r_first: Vec<f64> = Vec::new();
        for i in 0..iters {
            let c_first_order = i.rem_euclid(2) == 0;
            let (c, r) = if c_first_order {
                let c = c_ttfp(&cstr, 1024);
                let r = rust_caller::ttfp(&path, 1024).unwrap_or_else(|e| panic!("{}", e));
                (c, r)
            } else {
                let r = rust_caller::ttfp(&path, 1024).unwrap_or_else(|e| panic!("{}", e));
                let c = c_ttfp(&cstr, 1024);
                (c, r)
            };
            c_open.push(c.0);
            c_probe.push(c.1);
            c_first.push(c.2);
            r_open.push(r.open_us);
            r_probe.push(r.probe_us);
            r_first.push(r.first_us);
        }
        let co = compare::median(&c_open);
        let ro = compare::median(&r_open);
        let cp = compare::median(&c_probe);
        let rp = compare::median(&r_probe);
        let cf = compare::median(&c_first);
        let rf = compare::median(&r_first);
        let ttfp_c = co + cp + cf;
        let ttfp_r = ro + rp + rf;
        println!(
            "ttfp {} c_us={:.3} rust_us={:.3} delta={:+.2}% (open {:+.2}%, probe {:+.2}%, first {:+.2}%)",
            fx.id,
            ttfp_c,
            ttfp_r,
            (ttfp_r - ttfp_c) / ttfp_c * 100.0,
            (ro - co) / co * 100.0,
            (rp - cp) / cp * 100.0,
            (rf - cf) / cf * 100.0,
        );
        per_fixture.insert(
            fx.id.clone(),
            json!({
                "iterations": iters,
                "first_block_frames": 1024,
                "c": {"open_us": co, "probe_us": cp, "first_us": cf, "ttfp_us": ttfp_c},
                "rust": {"open_us": ro, "probe_us": rp, "first_us": rf, "ttfp_us": ttfp_r},
                "ttfp_delta_pct": (ttfp_r - ttfp_c) / ttfp_c * 100.0,
            }),
        );
    }
    json!({"iterations": iters, "fixtures": per_fixture})
}

fn c_floor(iterations: u64) -> f64 {
    let mut wall_ns: i64 = 0;
    let rc = unsafe { caller_floor(iterations as i64, &mut wall_ns) };
    if rc != 0 {
        panic!("c floor failed");
    }
    wall_ns as f64 / iterations as f64
}

fn run_floor(iterations: u64, repeats: usize) -> Value {
    let mut c_ns: Vec<f64> = Vec::new();
    let mut r_ns: Vec<f64> = Vec::new();
    for i in 0..repeats {
        if i.rem_euclid(2) == 0 {
            c_ns.push(c_floor(iterations));
            r_ns.push(rust_caller::floor(iterations));
        } else {
            r_ns.push(rust_caller::floor(iterations));
            c_ns.push(c_floor(iterations));
        }
    }
    let c = compare::median(&c_ns);
    let r = compare::median(&r_ns);
    println!(
        "floor iterations={} c_ns_per_call={:.3} rust_ns_per_call={:.3}",
        iterations, c, r
    );
    json!({
        "iterations": iterations,
        "repeats": repeats,
        "c_ns_per_call": c,
        "rust_ns_per_call": r,
    })
}

fn standalone_c_check(fx: &Fixture) -> Value {
    let path = fixture_path(fx);
    let mut walls: Vec<f64> = Vec::new();
    for _ in 0..5 {
        let out = Command::new(env!("QN_CALLER_BIN"))
            .arg("steady")
            .arg(&path)
            .arg("1024")
            .output()
            .expect("standalone caller failed to start");
        let txt = String::from_utf8_lossy(&out.stdout).trim().to_string();
        let v: Value = serde_json::from_str(&txt).unwrap();
        walls.push(v["wall_us"].as_f64().unwrap());
    }
    json!({"standalone_c_median_us": compare::median(&walls), "samples_us": walls})
}

fn parse_num(args: &[String], max: usize) -> (Vec<u64>, Vec<String>) {
    let mut nums = Vec::new();
    let mut rest = Vec::new();
    for (i, a) in args.iter().enumerate() {
        if i < max {
            if let Ok(n) = a.parse::<u64>() {
                nums.push(n);
                continue;
            }
        }
        rest.push(a.clone());
    }
    (nums, rest)
}

fn usage() {
    eprintln!(
        "usage: driver layout\n\
         \x20      driver surface [fixture...]\n\
         \x20      driver correct [fixture...]\n\
         \x20      driver steady [block=1024] [warmup=3] [iterations=20] [fixture...]\n\
         \x20      driver latency [block=1024] [warmup=2] [passes=3] [fixture...]\n\
         \x20      driver ttfp [iterations=20] [fixture...]\n\
         \x20      driver floor [iterations=2000000] [repeats=3]\n\
         \x20      driver all"
    );
}

fn measured_archive_sha() -> String {
    let path = repo_root().join("native/build/artifacts/libsongcore.a");
    Command::new("sha256sum")
        .arg(&path)
        .output()
        .ok()
        .and_then(|o| {
            String::from_utf8(o.stdout)
                .ok()
                .and_then(|s| s.split_whitespace().next().map(|w| w.to_string()))
        })
        .unwrap_or_default()
}

fn run_all() -> i32 {
    let results_dir = repo_root().join("experiments/songcore-call-comparison/results");
    std::fs::create_dir_all(&results_dir).unwrap();
    let fixtures = load_fixtures();

    let (layout_pass, layout_v) = run_layout(Some(&results_dir));
    println!("layout {}", if layout_pass { "PASS" } else { "FAIL" });
    let (surface_pass, surface_v) = run_surface(&fixtures);
    let (correct_pass, correct_v) = run_correct(&fixtures);

    let (steady_v, steady_labels) = run_steady(&fixtures, 1024, 3, 20);
    let (sweep256_v, sweep256_labels) = run_steady(&fixtures, 256, 2, 12);
    let (sweep4096_v, sweep4096_labels) = run_steady(&fixtures, 4096, 2, 12);
    let mut all_steady_labels = steady_labels.clone();
    all_steady_labels.extend(sweep256_labels);
    all_steady_labels.extend(sweep4096_labels);
    let latency_v = run_latency(&fixtures, 1024, 2, 3);
    let ttfp_v = run_ttfp(&fixtures, 20);
    let floor_v = run_floor(2_000_000, 3);
    let standalone_v = standalone_c_check(&fixtures[0]);

    let any_investigate = all_steady_labels.iter().any(|l| l == "INVESTIGATE");
    let any_measurable = all_steady_labels.iter().any(|l| l.starts_with("MEASURABLE"));
    let overall = if any_investigate {
        "INVESTIGATE".to_string()
    } else if any_measurable {
        let max_delta = steady_v["fixtures"]
            .as_object()
            .unwrap()
            .values()
            .map(|f| f["delta_pct"].as_f64().unwrap().abs())
            .fold(0.0_f64, f64::max);
        format!("MEASURABLE_FFI_TAX = {:.2}%", max_delta)
    } else {
        "NO_MEASURABLE_FFI_TAX".to_string()
    };

    let track_cost_ms = steady_v["fixtures"]
        .as_object()
        .unwrap()
        .values()
        .map(|f| f["delta_pct"].as_f64().unwrap().abs())
        .fold(0.0_f64, f64::max)
        / 100.0
        * 155.0;

    let measured_sha = measured_archive_sha();
    let results = json!({
        "schema": "songcore-call-comparison/1",
        "generated_utc": capture("date", &["-u", "+%Y-%m-%dT%H:%M:%SZ"]),
        "artifacts": {
            "static_libsongcore_sha256": QN_SONGCORE_ARTIFACT_SHA256,
            "songcore_header_sha256": QN_SONGCORE_HEADER_SHA256,
            "link_mode": "static, C caller object and Rust sys crate linked into one binary against one archive",
            "driver_measured_static_sha256": measured_sha,
            "driver_artifact_matches_build_time_sha": measured_sha == QN_SONGCORE_ARTIFACT_SHA256,
        },
        "host": host_info(),
        "protocol": {
            "primary_block_frames": 1024,
            "warmup": 3,
            "iterations": 20,
            "ordering": "balanced interleaved, per iteration both callers run with alternating order",
            "timed_window": "song_read_pcm loop only, fresh open+probe per iteration",
            "steady_gates": "per-iteration frames == reference pcm_frames and terminal == SONG_EOF, else sample rejected",
        },
        "layout_gate": layout_v,
        "surface_gate": surface_v,
        "correctness_gate": correct_v,
        "steady": steady_v,
        "block_sweep": {"256": sweep256_v, "4096": sweep4096_v},
        "read_latency": latency_v,
        "ttfp": ttfp_v,
        "call_floor": floor_v,
        "standalone_c_cross_check": standalone_v,
        "overall_verdict": overall,
        "equivalent_224s_track_cost_ms_at_max_delta": track_cost_ms,
        "track_cost_basis": "SongCore 224 s real-MP3 whole-file decode ~= 155 ms (docs/performance/decode-cost-model.md)",
    });

    let out_path = results_dir.join("c-vs-rust-ffi-cost.json");
    std::fs::write(&out_path, serde_json::to_string_pretty(&results).unwrap()).unwrap();
    println!("results written to {}", out_path.display());
    println!("overall verdict: {}", overall);

    let gates_ok = layout_pass && surface_pass && correct_pass;
    if !gates_ok {
        return 1;
    }
    if all_steady_labels.iter().any(|l| l == "INVESTIGATE") {
        return 2;
    }
    0
}

fn main() {
    let args: Vec<String> = env::args().skip(1).collect();
    let mode = args.first().map(|s| s.as_str()).unwrap_or("");
    let rest: Vec<String> = args.iter().skip(1).cloned().collect();
    match mode {
        "layout" => {
            let (pass, _) = run_layout(None);
            println!("layout {}", if pass { "PASS" } else { "FAIL" });
            std::process::exit(if pass { 0 } else { 1 });
        }
        "surface" => {
            let (pass, _) = run_surface(&selected_fixtures(&rest));
            std::process::exit(if pass { 0 } else { 1 });
        }
        "correct" => {
            let (pass, _) = run_correct(&selected_fixtures(&rest));
            std::process::exit(if pass { 0 } else { 1 });
        }
        "steady" => {
            let (nums, fixtures) = parse_num(&rest, 3);
            let block = nums.first().copied().unwrap_or(1024);
            let warmup = nums.get(1).copied().unwrap_or(3) as i32;
            let iters = nums.get(2).copied().unwrap_or(20) as i32;
            run_steady(&selected_fixtures(&fixtures), block, warmup, iters);
        }
        "latency" => {
            let (nums, fixtures) = parse_num(&rest, 3);
            let block = nums.first().copied().unwrap_or(1024);
            let warmup = nums.get(1).copied().unwrap_or(2) as i32;
            let passes = nums.get(2).copied().unwrap_or(3) as i32;
            let v = run_latency(&selected_fixtures(&fixtures), block, warmup, passes);
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        "ttfp" => {
            let (nums, fixtures) = parse_num(&rest, 1);
            let iters = nums.first().copied().unwrap_or(20) as i32;
            let v = run_ttfp(&selected_fixtures(&fixtures), iters);
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        "floor" => {
            let (nums, _) = parse_num(&rest, 2);
            let iterations = nums.first().copied().unwrap_or(2_000_000);
            let repeats = nums.get(1).copied().unwrap_or(3) as usize;
            let v = run_floor(iterations, repeats);
            println!("{}", serde_json::to_string_pretty(&v).unwrap());
        }
        "all" => {
            std::process::exit(run_all());
        }
        _ => {
            usage();
            std::process::exit(2);
        }
    }
}
