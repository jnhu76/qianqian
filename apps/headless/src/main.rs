//! Headless bootstrap of Architecture v2.
//!
//! Correctness authority must not depend on a UI framework, so the
//! product must always be able to start without one.

fn main() {
    let _runtime = qianqian_runtime::AppRuntime::new();

    println!("Qianqian Architecture v2");
    println!("headless runtime initialized");
}
