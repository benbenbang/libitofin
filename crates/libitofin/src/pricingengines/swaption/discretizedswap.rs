//! Swap priced on a lattice.
//!
//! Port of `ql/pricingengines/swap/discretizedswap.{hpp,cpp}`:
//! [`DiscretizedSwap`] is a [`DiscretizedAsset`] built from a
//! [`FixedVsFloatingSwapArguments`] bundle. It rolls the swap value backward
//! through a [`Lattice`], repricing each floating coupon with an embedded
//! [`DiscretizedDiscountBond`] and applying each fixed coupon at its reset node.
//! The Rust placement under `pricingengines/swaption/` (the C++ source lives in
//! `swap/`) follows the consuming `DiscretizedSwaption` (#465).
//!
//! # Reset-time routing (the `CouponAdjustment` split)
//! Each coupon is tagged [`CouponAdjustment::Pre`] or [`CouponAdjustment::Post`].
//! A normal coupon is applied in the pre-adjustment pass at its reset node; a
//! coupon whose reset time is already in the past (relative to the reference
//! date) is instead handled in the post pass, at its pay node, as an
//! already-fixed scalar amount. The past-in-time test is
//! [`is_reset_time_in_past`] (`discretizedswap.cpp:28`).
//!
//! Divergences from QuantLib, all deliberate:
//! - Every driver returns [`QlResult`] (D4/D10) rather than `void`.
//! - C++ copies the whole `VanillaSwap::arguments`; here only the fields the
//!   swap reads are extracted from `&args` at construction (the argument bundle
//!   is not `Clone`, and copying its legs would be wasteful).
//! - C++ reads `Settings::instance().includeTodaysCashFlows()` inside the ctor;
//!   per D5 the [`Settings`] handle is passed in explicitly and the flag read
//!   once at construction.
//! - The C++ `Null<Real>` sentinels become `Option`: a non-constant nominal
//!   (`arguments.nominal == Null`) is `None`, and an unavailable already-fixed
//!   floating coupon is a missing vector entry - both surface as `Err`.

use crate::discretizedasset::{
    CouponAdjustment, DiscretizedAsset, DiscretizedAssetBase, DiscretizedDiscountBond,
};
use crate::errors::QlResult;
use crate::instruments::{FixedVsFloatingSwapArguments, SwapType};
use crate::math::array::Array;
use crate::settings::Settings;
use crate::time::date::Date;
use crate::time::daycounter::DayCounter;
use crate::types::{Real, Size, Spread, Time};
use crate::{fail, require};

/// `isResetTimeInPast` (`discretizedswap.cpp:28`): a reset already happened when
/// its time is negative and either the coupon still pays in the future, or it
/// pays today and today's cash flows are included.
#[allow(clippy::float_cmp)]
fn is_reset_time_in_past(
    reset_time: Time,
    pay_time: Time,
    include_todays_cash_flows: bool,
) -> bool {
    reset_time < 0.0 && (pay_time > 0.0 || (include_todays_cash_flows && pay_time == 0.0))
}

/// A swap discretized on a [`Lattice`](crate::methods::lattices::lattice::Lattice)
/// (`discretizedswap.hpp:34`).
///
/// Built from a [`FixedVsFloatingSwapArguments`] with a reference date and day
/// counter; the reset/pay dates become year-fraction times on the lattice grid.
pub struct DiscretizedSwap {
    base: DiscretizedAssetBase,
    swap_type: SwapType,
    nominal: Option<Real>,
    fixed_coupons: Vec<Real>,
    floating_accrual_times: Vec<Time>,
    floating_spreads: Vec<Spread>,
    floating_coupons: Vec<Real>,
    fixed_reset_times: Vec<Time>,
    fixed_pay_times: Vec<Time>,
    fixed_coupon_adjustments: Vec<CouponAdjustment>,
    fixed_reset_is_in_past: Vec<bool>,
    floating_reset_times: Vec<Time>,
    floating_pay_times: Vec<Time>,
    floating_coupon_adjustments: Vec<CouponAdjustment>,
    floating_reset_is_in_past: Vec<bool>,
}

impl DiscretizedSwap {
    /// `DiscretizedSwap(args, referenceDate, dayCounter)` (`discretizedswap.cpp:36`):
    /// every coupon starts tagged [`CouponAdjustment::Pre`], then delegates to
    /// [`with_adjustments`](DiscretizedSwap::with_adjustments).
    ///
    /// # Errors
    /// Propagates [`with_adjustments`](DiscretizedSwap::with_adjustments).
    pub fn new(
        args: &FixedVsFloatingSwapArguments,
        reference_date: Date,
        day_counter: &DayCounter,
        settings: &Settings<Date>,
    ) -> QlResult<Self> {
        let fixed = vec![CouponAdjustment::Pre; args.fixed_pay_dates.len()];
        let floating = vec![CouponAdjustment::Pre; args.floating_pay_dates.len()];
        Self::with_adjustments(args, reference_date, day_counter, fixed, floating, settings)
    }

    /// `DiscretizedSwap(args, referenceDate, dayCounter, fixedAdjustments,
    /// floatingAdjustments)` (`discretizedswap.cpp:46`): resolve each leg's
    /// reset/pay times, and flip any coupon whose reset is in the past to
    /// [`CouponAdjustment::Post`].
    ///
    /// # Errors
    /// Fails if either adjustment vector's length differs from its leg's pay-date
    /// count, or if the arguments carry no swap type.
    #[allow(clippy::needless_range_loop)]
    pub fn with_adjustments(
        args: &FixedVsFloatingSwapArguments,
        reference_date: Date,
        day_counter: &DayCounter,
        mut fixed_coupon_adjustments: Vec<CouponAdjustment>,
        mut floating_coupon_adjustments: Vec<CouponAdjustment>,
        settings: &Settings<Date>,
    ) -> QlResult<Self> {
        require!(
            fixed_coupon_adjustments.len() == args.fixed_pay_dates.len(),
            "The fixed coupon adjustments must have the same size as the number of fixed coupons."
        );
        require!(
            floating_coupon_adjustments.len() == args.floating_pay_dates.len(),
            "The floating coupon adjustments must have the same size as the number of floating \
             coupons."
        );

        let Some(swap_type) = args.swap_type else {
            fail!("the swap type is not set on the arguments");
        };

        let include_todays = settings.include_todays_cash_flows() == Some(true);

        let n_fixed = args.fixed_reset_dates.len();
        let mut fixed_reset_times = Vec::with_capacity(n_fixed);
        let mut fixed_pay_times = Vec::with_capacity(n_fixed);
        let mut fixed_reset_is_in_past = Vec::with_capacity(n_fixed);
        for i in 0..n_fixed {
            let reset = day_counter.year_fraction(reference_date, args.fixed_reset_dates[i]);
            let pay = day_counter.year_fraction(reference_date, args.fixed_pay_dates[i]);
            let in_past = is_reset_time_in_past(reset, pay, include_todays);
            fixed_reset_times.push(reset);
            fixed_pay_times.push(pay);
            fixed_reset_is_in_past.push(in_past);
            if in_past {
                fixed_coupon_adjustments[i] = CouponAdjustment::Post;
            }
        }

        let n_float = args.floating_reset_dates.len();
        let mut floating_reset_times = Vec::with_capacity(n_float);
        let mut floating_pay_times = Vec::with_capacity(n_float);
        let mut floating_reset_is_in_past = Vec::with_capacity(n_float);
        for i in 0..n_float {
            let reset = day_counter.year_fraction(reference_date, args.floating_reset_dates[i]);
            let pay = day_counter.year_fraction(reference_date, args.floating_pay_dates[i]);
            let in_past = is_reset_time_in_past(reset, pay, include_todays);
            floating_reset_times.push(reset);
            floating_pay_times.push(pay);
            floating_reset_is_in_past.push(in_past);
            if in_past {
                floating_coupon_adjustments[i] = CouponAdjustment::Post;
            }
        }

        Ok(DiscretizedSwap {
            base: DiscretizedAssetBase::default(),
            swap_type,
            nominal: args.nominal,
            fixed_coupons: args.fixed_coupons.clone(),
            floating_accrual_times: args.floating_accrual_times.clone(),
            floating_spreads: args.floating_spreads.clone(),
            floating_coupons: args.floating_coupons.clone(),
            fixed_reset_times,
            fixed_pay_times,
            fixed_coupon_adjustments,
            fixed_reset_is_in_past,
            floating_reset_times,
            floating_pay_times,
            floating_coupon_adjustments,
            floating_reset_is_in_past,
        })
    }

    /// `addFixedCoupon(i)` (`discretizedswap.cpp:184`): reprice the `i`-th fixed
    /// coupon by rolling a unit discount bond from its pay node back to the
    /// current time, scaling by the coupon, and subtracting it on a payer swap
    /// (adding on a receiver).
    fn add_fixed_coupon(&mut self, i: Size) -> QlResult<()> {
        let method = self.require_method()?;
        let time = self.time();
        let pay_time = self.fixed_pay_times[i];
        let mut bond = DiscretizedDiscountBond::new();
        bond.initialize(method, pay_time)?;
        bond.rollback(time)?;

        let fixed_coupon = self.fixed_coupons[i];
        let payer = matches!(self.swap_type, SwapType::Payer);
        let values = self.values_mut();
        for j in 0..values.size() {
            let coupon = fixed_coupon * bond.values()[j];
            if payer {
                values[j] -= coupon;
            } else {
                values[j] += coupon;
            }
        }
        Ok(())
    }

    /// `addFloatingCoupon(i)` (`discretizedswap.cpp:199`): reprice the `i`-th
    /// floating coupon as `nominal * (1 - P) + accruedSpread * P`, where `P` is a
    /// unit discount bond rolled from the coupon's pay node to the current time
    /// and `accruedSpread = nominal * accrualTime * spread`. A payer swap adds it
    /// (a receiver subtracts).
    ///
    /// # Errors
    /// Fails if the nominal is non-constant (`arguments.nominal == Null`).
    fn add_floating_coupon(&mut self, i: Size) -> QlResult<()> {
        let method = self.require_method()?;
        let time = self.time();
        let pay_time = self.floating_pay_times[i];
        let mut bond = DiscretizedDiscountBond::new();
        bond.initialize(method, pay_time)?;
        bond.rollback(time)?;

        let Some(nominal) = self.nominal else {
            fail!("non-constant nominals are not supported yet");
        };
        let accrual = self.floating_accrual_times[i];
        let spread = self.floating_spreads[i];
        let accrued_spread = nominal * accrual * spread;
        let payer = matches!(self.swap_type, SwapType::Payer);
        let values = self.values_mut();
        for j in 0..values.size() {
            let bond_value = bond.values()[j];
            let coupon = nominal * (1.0 - bond_value) + accrued_spread * bond_value;
            if payer {
                values[j] += coupon;
            } else {
                values[j] -= coupon;
            }
        }
        Ok(())
    }
}

impl DiscretizedAsset for DiscretizedSwap {
    fn base(&self) -> &DiscretizedAssetBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut DiscretizedAssetBase {
        &mut self.base
    }

    fn as_asset_mut(&mut self) -> &mut dyn DiscretizedAsset {
        self
    }

    /// `reset(size)` (`discretizedswap.cpp:97`): zero the values, then adjust -
    /// so coupons at the initialization time are applied immediately.
    fn reset(&mut self, size: Size) -> QlResult<()> {
        *self.values_mut() = Array::filled(size, 0.0);
        self.adjust_values()
    }

    /// `mandatoryTimes()` (`discretizedswap.cpp:102`): every non-negative reset
    /// and pay time across both legs.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn mandatory_times(&self) -> Vec<Time> {
        let mut times = Vec::new();
        for &t in &self.fixed_reset_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        for &t in &self.fixed_pay_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        for &t in &self.floating_reset_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        for &t in &self.floating_pay_times {
            if t >= 0.0 {
                times.push(t);
            }
        }
        times
    }

    /// `preAdjustValuesImpl()` (`discretizedswap.cpp:123`): apply each
    /// pre-tagged floating coupon, then each pre-tagged fixed coupon, at any node
    /// on its reset time.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn pre_adjust_values_impl(&mut self) -> QlResult<()> {
        for i in 0..self.floating_reset_times.len() {
            let t = self.floating_reset_times[i];
            if self.floating_coupon_adjustments[i] == CouponAdjustment::Pre
                && t >= 0.0
                && self.is_on_time(t)
            {
                self.add_floating_coupon(i)?;
            }
        }
        for i in 0..self.fixed_reset_times.len() {
            let t = self.fixed_reset_times[i];
            if self.fixed_coupon_adjustments[i] == CouponAdjustment::Pre
                && t >= 0.0
                && self.is_on_time(t)
            {
                self.add_fixed_coupon(i)?;
            }
        }
        Ok(())
    }

    /// `postAdjustValuesImpl()` (`discretizedswap.cpp:140`): apply each
    /// post-tagged coupon at its reset node, then the already-fixed past-reset
    /// coupons as scalar amounts at their pay nodes.
    #[allow(clippy::neg_cmp_op_on_partial_ord)]
    fn post_adjust_values_impl(&mut self) -> QlResult<()> {
        for i in 0..self.floating_reset_times.len() {
            let t = self.floating_reset_times[i];
            if self.floating_coupon_adjustments[i] == CouponAdjustment::Post
                && t >= 0.0
                && self.is_on_time(t)
            {
                self.add_floating_coupon(i)?;
            }
        }
        for i in 0..self.fixed_reset_times.len() {
            let t = self.fixed_reset_times[i];
            if self.fixed_coupon_adjustments[i] == CouponAdjustment::Post
                && t >= 0.0
                && self.is_on_time(t)
            {
                self.add_fixed_coupon(i)?;
            }
        }

        let payer = matches!(self.swap_type, SwapType::Payer);
        for i in 0..self.fixed_pay_times.len() {
            if self.fixed_reset_is_in_past[i] && self.is_on_time(self.fixed_pay_times[i]) {
                let fixed_coupon = self.fixed_coupons[i];
                for v in self.values_mut().iter_mut() {
                    if payer {
                        *v -= fixed_coupon;
                    } else {
                        *v += fixed_coupon;
                    }
                }
            }
        }

        for i in 0..self.floating_pay_times.len() {
            if self.floating_reset_is_in_past[i] && self.is_on_time(self.floating_pay_times[i]) {
                let Some(&current) = self.floating_coupons.get(i) else {
                    fail!("current floating coupon not given");
                };
                for v in self.values_mut().iter_mut() {
                    if payer {
                        *v += current;
                    } else {
                        *v -= current;
                    }
                }
            }
        }
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::handle::Handle;
    use crate::indexes::ibor::Euribor;
    use crate::instrument::Instrument;
    use crate::instruments::VanillaSwap;
    use crate::interestrate::Compounding;
    use crate::math::timegrid::TimeGrid;
    use crate::methods::lattices::lattice::Lattice;
    use crate::models::shortrate::HullWhite;
    use crate::pricingengine::PricingEngine;
    use crate::pricingengines::DiscountingSwapEngine;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;
    use crate::time::timeunit::TimeUnit;
    use crate::types::{Integer, Rate, Real};

    // ------------------------------------------------------------------
    // Unit pins (no tree): reset routing and mandatory-time filtering.
    // ------------------------------------------------------------------

    /// A one-state test lattice: a single node per slice whose backward step
    /// leaves the value untouched (discount 1). Enough to drive `initialize` /
    /// `is_on_time` for the reset-routing pins; the numeric swap-vs-analytic
    /// anchor runs on the real Hull-White tree in the oracle test.
    struct OneNodeLattice {
        grid: TimeGrid,
    }

    impl Lattice for OneNodeLattice {
        fn time_grid(&self) -> &TimeGrid {
            &self.grid
        }
        fn initialize(&self, asset: &mut dyn DiscretizedAsset, time: Time) -> QlResult<()> {
            asset.set_time(time);
            asset.reset(1)
        }
        fn partial_rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()> {
            asset.set_time(to);
            Ok(())
        }
        fn rollback(&self, asset: &mut dyn DiscretizedAsset, to: Time) -> QlResult<()> {
            self.partial_rollback(asset, to)?;
            asset.adjust_values()
        }
        fn present_value(&self, asset: &mut dyn DiscretizedAsset) -> QlResult<Real> {
            Ok(asset.values()[0])
        }
        fn grid(&self, _time: Time) -> QlResult<Array> {
            Ok(Array::filled(1, 0.0))
        }
    }

    /// A fixed-only argument bundle: one fixed coupon resetting `reset` and
    /// paying `pay`, on a nominal of 100 with amount `coupon`. Floating legs are
    /// empty.
    fn fixed_only_args(
        swap_type: SwapType,
        reset: Date,
        pay: Date,
        coupon: Real,
    ) -> FixedVsFloatingSwapArguments {
        FixedVsFloatingSwapArguments {
            swap_type: Some(swap_type),
            nominal: Some(100.0),
            fixed_reset_dates: vec![reset],
            fixed_pay_dates: vec![pay],
            fixed_coupons: vec![coupon],
            fixed_nominals: vec![100.0],
            ..FixedVsFloatingSwapArguments::default()
        }
    }

    /// `mandatoryTimes` keeps only non-negative reset/pay times: a coupon whose
    /// reset predates the reference date contributes its pay time but not its
    /// (negative) reset time.
    #[test]
    fn mandatory_times_excludes_past_resets() {
        let reference = Date::new(15, Month::January, 2026);
        let dc = Actual365Fixed::new();
        let settings = Settings::<Date>::new();
        let args = fixed_only_args(
            SwapType::Payer,
            Date::new(15, Month::January, 2025),
            Date::new(15, Month::January, 2027),
            5.0,
        );
        let swap = DiscretizedSwap::new(&args, reference, &dc, &settings).unwrap();

        let times = swap.mandatory_times();
        assert_eq!(times.len(), 1, "only the future pay time survives");
        assert!((times[0] - 1.0).abs() < 1e-12, "pay time is +1y");
    }

    /// A past-reset fixed coupon flips to the post pass and is applied as a raw
    /// scalar at its pay node during `reset` (`reset` calls `adjust_values`, so
    /// the coupon lands immediately on initialization at that node). A payer
    /// subtracts it, a receiver adds it - the sign pins the past-reset branch.
    #[test]
    fn reset_applies_the_past_reset_coupon_at_its_pay_node() {
        let reference = Date::new(15, Month::January, 2026);
        let dc = Actual365Fixed::new();
        let settings = Settings::<Date>::new();
        let reset = Date::new(15, Month::January, 2025);
        let pay = Date::new(15, Month::January, 2027);

        let grid = TimeGrid::with_mandatory_times(&[1.0], 2).unwrap();
        let pay_time = grid.back().unwrap();

        for (swap_type, expected) in [(SwapType::Payer, -5.0), (SwapType::Receiver, 5.0)] {
            let args = fixed_only_args(swap_type, reset, pay, 5.0);
            let mut swap = DiscretizedSwap::new(&args, reference, &dc, &settings).unwrap();
            let lattice: Shared<dyn Lattice> = shared(OneNodeLattice { grid: grid.clone() });
            swap.initialize(Shared::clone(&lattice), pay_time).unwrap();
            assert!(
                (swap.values()[0] - expected).abs() < 1e-12,
                "{swap_type}: past-reset coupon gives {} (want {expected})",
                swap.values()[0]
            );
        }
    }

    // ------------------------------------------------------------------
    // Oracle: swap-on-tree NPV == analytic NPV (the batch's absolute anchor).
    // ------------------------------------------------------------------

    /// The `swap.cpp` `CommonVars` fixture (`:87-104`): a TARGET calendar, a
    /// two-day settlement, and a flat 5% Actual365Fixed curve fixed at
    /// settlement that a Euribor 6M index also forecasts off. `usingAtParCoupons`
    /// is off so the analytic floating leg is the true forward (indexed) leg the
    /// tree's discount-bond reprice converges to.
    struct Vars {
        settings: Shared<Settings<Date>>,
        calendar: Calendar,
        settlement: Date,
        curve: Handle<dyn YieldTermStructure>,
        index: Shared<crate::indexes::IborIndex>,
    }

    impl Vars {
        fn new() -> Vars {
            let settings = shared(Settings::new());
            settings.set_evaluation_date(Date::new(17, Month::June, 2002));
            settings.set_using_at_par_coupons(false);
            let calendar = Target::new();
            let settlement = calendar.advance(
                Date::new(17, Month::June, 2002),
                2,
                TimeUnit::Days,
                BusinessDayConvention::Following,
                false,
            );
            let curve: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
                settlement,
                0.05,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            ))
                as Shared<dyn YieldTermStructure>);
            let index = shared(Euribor::six_months(curve.clone(), Shared::clone(&settings)));
            Vars {
                settings,
                calendar,
                settlement,
                curve,
                index,
            }
        }

        /// A `length`-year swap at an off-market fixed rate and a nonzero spread,
        /// priced through the discounting engine (its NPV is the oracle).
        fn make_swap(&self, swap_type: SwapType, length: Integer, rate: Rate) -> VanillaSwap {
            let maturity = self.calendar.advance(
                self.settlement,
                length,
                TimeUnit::Years,
                BusinessDayConvention::ModifiedFollowing,
                false,
            );
            let fixed_schedule = MakeSchedule::new()
                .from(self.settlement)
                .to(maturity)
                .with_frequency(Frequency::Annual)
                .with_calendar(self.calendar.clone())
                .with_convention(BusinessDayConvention::Unadjusted)
                .with_termination_date_convention(BusinessDayConvention::Unadjusted)
                .forwards()
                .end_of_month(false)
                .build();
            let float_schedule = MakeSchedule::new()
                .from(self.settlement)
                .to(maturity)
                .with_frequency(Frequency::Semiannual)
                .with_calendar(self.calendar.clone())
                .with_convention(BusinessDayConvention::ModifiedFollowing)
                .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
                .forwards()
                .end_of_month(false)
                .build();
            let mut swap = VanillaSwap::new(
                swap_type,
                100.0,
                fixed_schedule,
                rate,
                Thirty360::with_convention(Convention::BondBasis),
                float_schedule,
                Shared::clone(&self.index),
                0.002,
                Actual360::new(),
                None,
                Shared::clone(&self.settings),
            )
            .unwrap();
            let engine = shared_mut(DiscountingSwapEngine::new(
                self.curve.clone(),
                None,
                None,
                None,
                Shared::clone(&self.settings),
            ));
            swap.base_mut()
                .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
            swap
        }
    }

    /// Builds a `DiscretizedSwap` from a priced swap's arguments and a fitted
    /// Hull-White tree over the swap's mandatory times, rolls it back to 0, and
    /// returns its present value. `model_curve` is the curve the tree is fit to
    /// (the same 5% curve for the anchor; a different one for the stub).
    fn tree_npv(
        vars: &Vars,
        swap: &VanillaSwap,
        model_curve: Handle<dyn YieldTermStructure>,
    ) -> Real {
        let mut args = FixedVsFloatingSwapArguments::default();
        swap.setup_arguments(&mut args).unwrap();

        let link = vars.curve.current_link().unwrap();
        let reference_date = link.reference_date().unwrap();
        let day_counter = link.day_counter().unwrap();
        let mut dswap =
            DiscretizedSwap::new(&args, reference_date, &day_counter, &vars.settings).unwrap();

        let grid = TimeGrid::with_mandatory_times(&dswap.mandatory_times(), 40).unwrap();
        let model = HullWhite::new(model_curve, 0.1, 0.01).unwrap();
        let lattice: Shared<dyn Lattice> = shared(model.borrow().tree(grid.clone()).unwrap());

        dswap
            .initialize(Shared::clone(&lattice), grid.back().unwrap())
            .unwrap();
        dswap.rollback(0.0).unwrap();
        dswap.present_value().unwrap()
    }

    /// ABSOLUTE ANCHOR (advisor): a 3Y off-market (6% fixed, 0.002 spread) swap
    /// priced on the Hull-White tree reproduces the on-main `DiscountingSwapEngine`
    /// NPV. Both the payer and receiver arms are pinned: the tree value matches
    /// the analytic value, and the receiver negates the payer. The nonzero spread
    /// exercises the `accrued_spread = nominal*T*spread` term in every floating
    /// coupon. The analytic payer NPV is materially nonzero (~-1.8 on a nominal of
    /// 100), so the relative comparison is not vacuous; the tree reproduces it to a
    /// relative ~8e-5 (pure discretization), well inside the 1e-3 bound.
    #[test]
    fn swap_on_tree_matches_the_analytic_npv() {
        let vars = Vars::new();

        let mut payer = vars.make_swap(SwapType::Payer, 3, 0.06);
        let mut receiver = vars.make_swap(SwapType::Receiver, 3, 0.06);
        let analytic_payer = payer.npv().unwrap();
        let analytic_receiver = receiver.npv().unwrap();
        assert!(
            analytic_payer.abs() > 1.0,
            "the anchor swap must be materially off-market, got {analytic_payer}"
        );

        let tree_payer = tree_npv(&vars, &payer, vars.curve.clone());
        let tree_receiver = tree_npv(&vars, &receiver, vars.curve.clone());

        let rel_payer = (tree_payer - analytic_payer).abs() / analytic_payer.abs();
        let rel_receiver = (tree_receiver - analytic_receiver).abs() / analytic_receiver.abs();
        assert!(
            rel_payer < 1e-3,
            "payer: tree {tree_payer} vs analytic {analytic_payer} (rel {rel_payer})"
        );
        assert!(
            rel_receiver < 1e-3,
            "receiver: tree {tree_receiver} vs analytic {analytic_receiver} (rel {rel_receiver})"
        );
        assert!(
            (tree_payer + tree_receiver).abs() < 1e-9,
            "payer and receiver tree NPVs must negate: {tree_payer} vs {tree_receiver}"
        );
    }

    /// CONFIRM-BY-STUB: a tree fit to the WRONG curve (flat 2% instead of the
    /// swap's 5%) discounts the legs differently, so the swap-on-tree NPV diverges
    /// from the analytic NPV by far more than the discretization tolerance. This
    /// proves the anchor rests on the tree fit + discount-bond leg reprice, not a
    /// coincidental match.
    #[test]
    fn a_tree_fit_to_the_wrong_curve_breaks_the_anchor() {
        let vars = Vars::new();
        let mut payer = vars.make_swap(SwapType::Payer, 3, 0.06);
        let analytic = payer.npv().unwrap();

        let wrong_curve: Handle<dyn YieldTermStructure> =
            Handle::new(shared(FlatForward::with_rate(
                vars.settlement,
                0.02,
                Actual365Fixed::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>);
        let mispriced = tree_npv(&vars, &payer, wrong_curve);

        assert!(
            (mispriced - analytic).abs() / analytic.abs() > 1e-2,
            "a mis-fit tree must diverge: {mispriced} vs analytic {analytic}"
        );
    }
}
