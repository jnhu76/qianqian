//! Completed-attempt result carried from Session activation to fresh assembly.
//! Semantics belong to PBK-002 D14.6; this is neither a Fact nor a read model.

use std::cell::RefCell;
use std::rc::Rc;

/// D14.6 `Activated` for the required Session in a fresh Decode/Output/Session
/// composition. Establishment remains true even if D11 settles immediately.
#[derive(Debug, Clone, PartialEq, Eq)]
pub enum EstablishmentResult {
    Established,
    NotEstablished { diagnostic: Option<String> },
}

/// One fresh attempt's return slot. Only the Session activation closure writes
/// it. A provider failure or unresolved dependency leaves it NotEstablished,
/// because K0 cannot invoke that closure until both required providers bind.
///
/// Public because an assembly crate cannot reach the private activation path.
/// Keeping the result on the observation handle would invite a second read-side
/// definition. This slot adds no lifecycle, retry, terminal or kernel semantics.
/// Attach the paired spec once to a fresh root, then consume after synchronous
/// `revise_desired` returns. It does not certify arbitrary additional Plugins.
pub struct EstablishmentAttempt(pub(crate) Rc<RefCell<EstablishmentResult>>);

impl EstablishmentAttempt {
    pub(crate) fn new() -> Self {
        Self(Rc::new(RefCell::new(EstablishmentResult::NotEstablished {
            diagnostic: None,
        })))
    }

    /// Consume the authority-owned result of the completed fresh attempt.
    /// No snapshot, source evidence, diagnostic predicate or terminal read.
    pub fn finish(self) -> EstablishmentResult {
        self.0.borrow().clone()
    }
}
