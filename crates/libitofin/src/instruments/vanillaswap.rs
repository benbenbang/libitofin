//! Plain-vanilla fixed-vs-Ibor interest-rate swap.
//!
//! Port of `ql/instruments/vanillaswap.{hpp,cpp}`: `class VanillaSwap : public
//! FixedVsFloatingSwap` (`vanillaswap.hpp:65`), the most-used rates instrument.
//! It builds a fixed leg and a floating [`IborLeg`] and hands them to the base;
//! `setupFloatingArguments` is its one real override.
//!
//! The port composes rather than inherits: [`VanillaSwap`] holds a
//! [`FixedVsFloatingSwap`] and delegates the [`Instrument`] face to it. Its
//! embedded base is reached through [`fixed_vs_floating`](VanillaSwap::fixed_vs_floating)
//! for the fair-rate, leg-NPV/BPS and other base accessors; the type adds no
//! members of its own, exactly as C++ `VanillaSwap` adds no data over the base.
//!
//! Deviations, all by existing design decisions or the inheritance-to-composition
//! shift:
//! - Staging inversion: C++ constructs the base first, then assigns
//!   `legs_[1] = IborLeg(...)` from the base's resolved `floatingNominals()`,
//!   `floatingDayCount()`, `paymentConvention()` and `spread()`
//!   (`vanillaswap.cpp:44-51`). The port's [`FixedVsFloatingSwap::new`] instead
//!   *receives* the floating leg, so [`VanillaSwap::new`] builds the [`IborLeg`]
//!   first (resolving the payment convention the same way the base does, an
//!   unset convention falling back to the floating schedule's) and passes it
//!   down. Same final state, construction order inverted.
//! - `setupFloatingArguments` is not a Rust method override. The base takes a
//!   [`FloatingArgumentsFn`] closure and calls it inside its own
//!   `setupArguments`; [`VanillaSwap::new`] supplies that closure. It captures
//!   the concrete `IborCoupon`s the leg was built from (the same `Rc`-shared
//!   coupons the base holds) because `fixingDate` and the coupon spread are not
//!   on the erased [`Coupon`] face. Because the override lives in the base, every
//!   [`Instrument`] method delegates to the base wholesale with no dispatch
//!   re-routing.
//! - The spread filled per floating coupon is the swap's own [`spread`] rather
//!   than a per-coupon read: the leg is built with a single `withSpreads(spread)`,
//!   so every coupon carries that one spread, matching C++ `coupon->spread()`.
//! - `coupon->amount()` propagates its error with `?` instead of C++'s
//!   catch-and-`Null<Real>`: the port has no `Null<Real>` sentinel (D4/D10), and
//!   the generic-`Swap`-engine oracle path (#322) never reaches this closure, so
//!   the choice is inert there.
//! - `useIndexedCoupons` (`vanillaswap.hpp:76`) is not accepted. The per-coupon
//!   par/indexed mode is read from [`Settings`] at forecast time (#315); a
//!   per-swap override is deferred rather than accepted and silently ignored. The
//!   #322 oracle passes no `useIndexedCoupons`, so it takes the Settings default
//!   either way.

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{Coupon, IborCoupon, IborLeg};
use crate::errors::QlResult;
use crate::indexes::IborIndex;
use crate::instrument::{Instrument, InstrumentBase};
use crate::instruments::fixedvsfloatingswap::{
    FixedVsFloatingSwap, FixedVsFloatingSwapArguments, FloatingArgumentsFn,
};
use crate::instruments::swap::SwapType;
use crate::pricingengine::{Arguments, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::time::schedule::Schedule;
use crate::types::{Rate, Real, Spread};

/// Plain-vanilla swap: a fixed leg versus an Ibor leg.
///
/// Composes a [`FixedVsFloatingSwap`]; build with [`new`](Self::new), reach the
/// base's accessors through [`fixed_vs_floating`](Self::fixed_vs_floating) /
/// [`fixed_vs_floating_mut`](Self::fixed_vs_floating_mut), and price it through
/// its [`Instrument`] face.
pub struct VanillaSwap {
    base: FixedVsFloatingSwap,
}

impl VanillaSwap {
    /// Builds a vanilla swap over a single `nominal` (the C++ ctor,
    /// `vanillaswap.cpp:29`).
    ///
    /// Both legs carry the one `nominal`. The fixed leg is built by the base
    /// from `fixed_schedule` / `fixed_rate` / `fixed_day_count`; the floating
    /// [`IborLeg`] is built here from `float_schedule` / `ibor_index` /
    /// `floating_day_count` / `spread`, with the payment convention resolved
    /// against the floating schedule when `payment_convention` is `None`.
    ///
    /// # Errors
    ///
    /// Propagates the floating-leg build (an empty schedule, say) and the base
    /// construction.
    #[allow(clippy::too_many_arguments)]
    pub fn new(
        swap_type: SwapType,
        nominal: Real,
        fixed_schedule: Schedule,
        fixed_rate: Rate,
        fixed_day_count: DayCounter,
        float_schedule: Schedule,
        ibor_index: Shared<IborIndex>,
        spread: Spread,
        floating_day_count: DayCounter,
        payment_convention: Option<BusinessDayConvention>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<VanillaSwap> {
        let resolved_convention =
            payment_convention.unwrap_or_else(|| float_schedule.business_day_convention());

        let coupons = IborLeg::new(float_schedule.clone(), Shared::clone(&ibor_index))
            .with_notionals(vec![nominal])
            .with_payment_day_counter(floating_day_count.clone())
            .with_payment_adjustment(resolved_convention)
            .with_spreads(vec![spread])
            .coupons()?;
        let floating_leg: Leg = coupons
            .iter()
            .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
            .collect();

        let floating_arguments: FloatingArgumentsFn =
            Box::new(move |swap, args| fill_floating_arguments(&coupons, swap, args));

        let base = FixedVsFloatingSwap::new(
            swap_type,
            vec![nominal],
            fixed_schedule,
            fixed_rate,
            Some(fixed_day_count),
            vec![nominal],
            float_schedule,
            ibor_index,
            spread,
            floating_day_count,
            payment_convention,
            0,
            None,
            floating_leg,
            floating_arguments,
            settings,
        )?;

        Ok(VanillaSwap { base })
    }

    /// The embedded fixed-vs-floating base (its fair-rate, leg and nominal
    /// accessors).
    pub fn fixed_vs_floating(&self) -> &FixedVsFloatingSwap {
        &self.base
    }

    /// The embedded base, mutably (the `&mut self` accessors: `fairRate`,
    /// `fixedLegNPV` and the like, which price on demand).
    pub fn fixed_vs_floating_mut(&mut self) -> &mut FixedVsFloatingSwap {
        &mut self.base
    }
}

/// Fills the floating-leg argument vectors from the swap's `IborCoupon`s (the
/// C++ `VanillaSwap::setupFloatingArguments`, `vanillaswap.cpp:54`).
fn fill_floating_arguments(
    coupons: &[Shared<IborCoupon>],
    swap: &FixedVsFloatingSwap,
    args: &mut FixedVsFloatingSwapArguments,
) -> QlResult<()> {
    let n = coupons.len();
    args.floating_reset_dates = Vec::with_capacity(n);
    args.floating_pay_dates = Vec::with_capacity(n);
    args.floating_nominals = Vec::with_capacity(n);
    args.floating_fixing_dates = Vec::with_capacity(n);
    args.floating_accrual_times = Vec::with_capacity(n);
    args.floating_spreads = Vec::with_capacity(n);
    args.floating_coupons = Vec::with_capacity(n);

    for coupon in coupons {
        args.floating_reset_dates.push(coupon.accrual_start_date());
        args.floating_pay_dates
            .push(coupon.coupon_base().payment_date());
        args.floating_nominals.push(coupon.nominal());
        args.floating_fixing_dates.push(coupon.fixing_date());
        args.floating_accrual_times.push(coupon.accrual_period());
        args.floating_spreads.push(swap.spread());
        args.floating_coupons.push(Coupon::amount(coupon.as_ref())?);
    }
    Ok(())
}

impl Instrument for VanillaSwap {
    fn base(&self) -> &InstrumentBase {
        self.base.base()
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        self.base.base_mut()
    }

    fn is_expired(&self) -> QlResult<bool> {
        self.base.is_expired()
    }

    fn setup_expired(&mut self) {
        self.base.setup_expired();
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        self.base.setup_arguments(arguments)
    }

    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        self.base.fetch_results(results)
    }
}
