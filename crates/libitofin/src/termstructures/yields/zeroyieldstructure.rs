//! Zero-yield based term structure.
//!
//! Port of `ql/termstructures/yield/zeroyieldstructure.hpp`: the
//! [`ZeroYieldStructure`] adapter lets a curve implement only
//! [`zero_yield_impl`](ZeroYieldStructure::zero_yield_impl); discounts (and
//! through them forwards) are calculated from the zero yields.
//!
//! Zero rates are assumed to be annual continuous compounding.
//!
//! ## Divergences from QuantLib
//!
//! C++ derives the abstract class from `YieldTermStructure` and overrides
//! `discountImpl`; a Rust blanket impl of [`YieldTermStructure`] for every
//! `ZeroYieldStructure` would conflict (E0119) with curves implementing
//! [`YieldTermStructure`] directly, so the derivation is the provided
//! [`discount_from_zero_yield`](ZeroYieldStructure::discount_from_zero_yield)
//! and each concrete curve wires it in:
//!
//! ```ignore
//! impl YieldTermStructure for MyCurve {
//!     fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
//!         self.discount_from_zero_yield(t)
//!     }
//! }
//! ```

use crate::errors::QlResult;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::types::{DiscountFactor, Rate, Time};

/// Zero-yield term structure: implement
/// [`zero_yield_impl`](Self::zero_yield_impl) and wire
/// [`discount_from_zero_yield`](Self::discount_from_zero_yield) into
/// [`discount_impl`](YieldTermStructure::discount_impl).
pub trait ZeroYieldStructure: YieldTermStructure {
    /// Zero-yield calculation, called after range checking; it must assume
    /// extrapolation is required.
    fn zero_yield_impl(&self, t: Time) -> QlResult<Rate>;

    /// The discount factor calculated from the zero yield (C++'s
    /// `discountImpl`); at `t = 0` it returns 1.0 without calling
    /// [`zero_yield_impl`](Self::zero_yield_impl), guarding implementations
    /// that cannot evaluate there.
    fn discount_from_zero_yield(&self, t: Time) -> QlResult<DiscountFactor> {
        if t == 0.0 {
            return Ok(1.0);
        }
        let r = self.zero_yield_impl(t)?;
        Ok((-r * t).exp())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::fail;
    use crate::interestrate::Compounding;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::termstructures::{TermStructure, TermStructureBase};
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::frequency::Frequency;
    use crate::types::Real;

    struct LinearZeroCurve {
        base: TermStructureBase,
        a: Real,
        b: Real,
    }

    impl LinearZeroCurve {
        fn new(a: Real, b: Real) -> LinearZeroCurve {
            LinearZeroCurve {
                base: TermStructureBase::with_reference_date(
                    Date::new(15, Month::June, 2026),
                    None,
                    Some(Actual360::new()),
                ),
                a,
                b,
            }
        }
    }

    impl AsObservable for LinearZeroCurve {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for LinearZeroCurve {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl ZeroYieldStructure for LinearZeroCurve {
        fn zero_yield_impl(&self, t: Time) -> QlResult<Rate> {
            if t == 0.0 {
                fail!("zero yield not defined at t = 0");
            }
            Ok(self.a + self.b * t)
        }
    }

    impl YieldTermStructure for LinearZeroCurve {
        fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
            self.discount_from_zero_yield(t)
        }
    }

    #[test]
    fn discount_is_derived_from_the_zero_yield() {
        let curve = LinearZeroCurve::new(0.03, 0.01);
        for t in [0.25_f64, 1.0, 2.5] {
            let expected = (-(0.03 + 0.01 * t) * t).exp();
            assert!((curve.discount(t, false).unwrap() - expected).abs() < 1.0e-15);
        }
    }

    #[test]
    fn discount_at_zero_guards_the_zero_yield_call() {
        let curve = LinearZeroCurve::new(0.03, 0.01);
        assert!(curve.zero_yield_impl(0.0).is_err());
        assert_eq!(curve.discount(0.0, false).unwrap(), 1.0);
    }

    #[test]
    fn zero_yield_errors_propagate_through_discount() {
        struct FailingCurve {
            inner: LinearZeroCurve,
        }
        impl AsObservable for FailingCurve {
            fn observable(&self) -> &Observable {
                self.inner.observable()
            }
        }
        impl TermStructure for FailingCurve {
            fn base(&self) -> &TermStructureBase {
                self.inner.base()
            }
            fn max_date(&self) -> Date {
                Date::max_date()
            }
        }
        impl ZeroYieldStructure for FailingCurve {
            fn zero_yield_impl(&self, _t: Time) -> QlResult<Rate> {
                fail!("no zero yield available");
            }
        }
        impl YieldTermStructure for FailingCurve {
            fn discount_impl(&self, t: Time) -> QlResult<DiscountFactor> {
                self.discount_from_zero_yield(t)
            }
        }

        let curve = FailingCurve {
            inner: LinearZeroCurve::new(0.03, 0.01),
        };
        let err = curve.discount(1.0, false).unwrap_err();
        assert!(err.message().contains("no zero yield available"));
        assert_eq!(curve.discount(0.0, false).unwrap(), 1.0);
    }

    #[test]
    fn zero_rate_recovers_the_curve_and_forwards_differentiate_it() {
        let curve = LinearZeroCurve::new(0.03, 0.01);
        let zero = curve
            .zero_rate(2.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((zero.rate() - 0.05).abs() < 1.0e-14);

        // The instantaneous forward is d(z(t) t)/dt = a + 2 b t; the DT-wide
        // finite difference in forward_rate resolves it to O(DT).
        let forward = curve
            .forward_rate(2.0, 2.0, Compounding::Continuous, Frequency::Annual, false)
            .unwrap();
        assert!((forward.rate() - 0.07).abs() < 1.0e-6);
    }
}
