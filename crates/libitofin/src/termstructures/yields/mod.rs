//! Yield term structures.
//!
//! Port of `ql/termstructures/yield/` (named `yields` because `yield` is a
//! Rust keyword); concrete curves implementing
//! [`YieldTermStructure`](super::yieldtermstructure::YieldTermStructure).

mod discountcurve;
mod flatforward;
mod forwardcurve;
mod forwardspreadedtermstructure;
mod forwardstructure;
mod impliedtermstructure;
mod zerocurve;
mod zerospreadedtermstructure;
mod zeroyieldstructure;

pub use discountcurve::{DiscountCurve, InterpolatedDiscountCurve};
pub use flatforward::FlatForward;
pub use forwardcurve::{ForwardCurve, InterpolatedForwardCurve};
pub use forwardspreadedtermstructure::ForwardSpreadedTermStructure;
pub use forwardstructure::ForwardRateStructure;
pub use impliedtermstructure::ImpliedTermStructure;
pub use zerocurve::{InterpolatedZeroCurve, ZeroCurve};
pub use zerospreadedtermstructure::ZeroSpreadedTermStructure;
pub use zeroyieldstructure::ZeroYieldStructure;

use crate::handle::Handle;
use crate::termstructures::TermStructureBase;
use crate::termstructures::yieldtermstructure::YieldTermStructure;

/// Re-syncs a spreaded/implied curve's extrapolation flag to its underlying
/// curve's, shared by the adapters' `update()` observers.
pub(super) fn sync_extrapolation(
    base: &TermStructureBase,
    original: &Handle<dyn YieldTermStructure>,
) {
    if let Ok(original) = original.current_link() {
        if original.allows_extrapolation() {
            base.enable_extrapolation();
        } else {
            base.disable_extrapolation();
        }
    }
}
