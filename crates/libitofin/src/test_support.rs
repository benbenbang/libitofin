//! Shared test utilities.
//!
//! Port of the `Flag` utility from QuantLib's `test-suite/utilities.hpp`: an
//! observer recording whether it was raised. Pre-existing per-module copies
//! (`settings`, `handle`, `patterns::lazyobject`) predate this module and can
//! migrate here over time.

use crate::handle::Handle;
use crate::math::interpolations::loglinear::LogLinear;
use crate::patterns::observable::Observer;
use crate::quotes::{Quote, SimpleQuote};
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::bootstraphelper::RateHelper;
use crate::termstructures::bootstraptraits::Discount;
use crate::termstructures::yields::PiecewiseYieldCurve;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::types::Real;

/// Observer recording whether it was notified.
pub(crate) struct Flag {
    up: bool,
}

impl Flag {
    pub(crate) fn new() -> SharedMut<Flag> {
        shared_mut(Flag { up: false })
    }

    pub(crate) fn lower(flag: &SharedMut<Flag>) {
        flag.borrow_mut().up = false;
    }

    pub(crate) fn is_up(flag: &SharedMut<Flag>) -> bool {
        flag.borrow().up
    }
}

impl Observer for Flag {
    fn update(&mut self) {
        self.up = true;
    }
}

/// Upcasts a flag to the `dyn Observer` handle expected by registration.
pub(crate) fn as_observer(flag: &SharedMut<Flag>) -> SharedMut<dyn Observer> {
    flag.clone()
}

/// Re-prices `make`'s helper on a dirty curve, the issue #1344 scenario.
///
/// Bootstraps a single-helper curve on `quote_before`, moves the helper's own
/// quote to `quote_after`, then calls `implied_quote()` directly while the
/// curve is stale. Returns that value together with `implied_quote()` of a
/// fresh helper bootstrapped on `quote_after` from the start.
pub(crate) fn implied_quote_on_dirty_curve(
    reference: Date,
    quote_before: Real,
    quote_after: Real,
    make: impl Fn(Handle<dyn Quote>) -> Shared<dyn RateHelper>,
) -> (Real, Real) {
    let bootstrap = |quote: &Shared<SimpleQuote>| {
        let helper = make(Handle::new(Shared::clone(quote) as Shared<dyn Quote>));
        let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
            reference,
            vec![Shared::clone(&helper)],
            Actual365Fixed::new(),
            LogLinear,
        )
        .unwrap();
        curve.discount(0.5, true).unwrap();
        (helper, curve)
    };

    let quote = shared(SimpleQuote::new(quote_before));
    let (helper, _curve) = bootstrap(&quote);
    quote.set_value(quote_after);
    let dirty = helper.implied_quote().unwrap();

    let (fresh_helper, _fresh_curve) = bootstrap(&shared(SimpleQuote::new(quote_after)));
    (dirty, fresh_helper.implied_quote().unwrap())
}
