//! R0 bootstrap witness — not the Composition Kernel's home.
//!
//! R0 contains no composition/lifecycle kernel concepts. Composition
//! currently lives in `qianqian-runtime` as ordinary construction.
//! The generic Composition Kernel must not depend on product
//! semantics, so its final home is an implementation-issue decision;
//! this module is not its mandated location, and lifecycle/profile
//! concepts landing here is not a compatibility expectation.
