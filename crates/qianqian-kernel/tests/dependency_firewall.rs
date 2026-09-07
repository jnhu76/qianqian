//! Dependency firewall (implementation ADR D1): the generic kernel must not
//! depend on any product crate. Enforced at compile time via the manifest.

use qianqian_kernel::{ComponentSpec, DesiredEntry, Kernel, Revision};

/// The kernel's own Cargo.toml, embedded at compile time.
const MANIFEST: &str = include_str!("../Cargo.toml");

#[test]
fn kernel_crate_has_no_product_dependencies() {
    for forbidden in [
        "qianqian-core",
        "qianqian-runtime",
        "qianqian-headless",
        "[dependencies.qianqian",
    ] {
        assert!(
            !MANIFEST.contains(forbidden),
            "kernel manifest must not reference '{forbidden}': the generic kernel \
             depends on no product crate (AGENTS.md constitution; ADR D1)"
        );
    }
}

/// Stage-2 smoke proof: a generic kernel that knows nothing about any domain
/// can host components end to end.
#[test]
fn minimal_kernel_hosts_an_anonymous_component() {
    let mut kernel = Kernel::new();
    kernel
        .register_component(ComponentSpec::new("anonymous"))
        .expect("component registered");
    kernel
        .set_desired(vec![DesiredEntry::enabled(
            "a",
            "anonymous",
            Revision::new(1),
        )])
        .expect("legal desired composition");
    kernel.settle();

    let snap = kernel.snapshot();
    assert!(snap.quiet);
    assert_eq!(
        snap.fibers.get("a").map(|f| f.state),
        Some(qianqian_kernel::FiberState::Active)
    );
}
