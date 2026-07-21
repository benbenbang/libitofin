//! Analytic cap/floor engine for the Hull-White model.
//!
//! Port of `ql/pricingengines/capfloor/analyticcapfloorengine.{hpp,cpp}`:
//! [`AnalyticCapFloorEngine`] prices a
//! [`CapFloor`](crate::instruments::CapFloor) under the one-factor
//! [`HullWhite`] model as a portfolio of options on the individual coupon
//! discount bonds. Each caplet is a put on the accrual-period zero-coupon bond,
//! each floorlet a call, priced by the model's `discount_bond_option`
//! (`analyticcapfloorengine.cpp:94-113`). A coupon whose fixing has already set
//! contributes its deterministic intrinsic value instead
//! (`analyticcapfloorengine.cpp:78-93`).
//!
//! ## Model binding (documented deferral)
//!
//! C++'s engine is `GenericModelEngine<AffineModel, ...>`
//! (`analyticcapfloorengine.hpp:36`), generic over any affine model.
//! `discount_bond_option` lives on no trait on main, and [`HullWhite`] (#391) is
//! the only model that provides it, so the port binds concretely to a
//! [`SharedMut<HullWhite>`] - the same choice, for the same reason, that the
//! [`JamshidianSwaptionEngine`](crate::pricingengines::swaption::JamshidianSwaptionEngine)
//! documents. A generic engine over every affine model waits for a
//! `DiscountBondOption` trait, a later ticket.
//!
//! ## Deferred / collapsed
//!
//! - **The non-term-structure-consistent-model fallback**
//!   (`analyticcapfloorengine.cpp:40-48`, the `dynamic_pointer_cast` `else`
//!   branch, and the engine-level `termStructure_`): Hull-White is always
//!   term-structure consistent, so only the `tsmodel` branch is live. The
//!   reference date and day counter are read straight off
//!   `model.term_structure()`. The fallback and its ctor overload are not ported.
//! - **The model-present guard** (`:35`, `QL_REQUIRE(!model_.empty())`): the ctor
//!   takes the model by value, so an absent model is structurally impossible.
//!
//! ## Divergences from QuantLib
//!
//! - **Explicit [`Settings`] (D5).** C++ reads the global `Settings::instance()`
//!   for `includeReferenceDateEvents`/`includeTodaysCashFlows` and the evaluation
//!   date (`:54-61`); the port threads an explicit handle, as every other engine
//!   does under D5.
//! - **The intrinsic-branch discount reads the curve directly.** C++ calls
//!   `model_->discount(paymentTime)` (`:80`); [`HullWhite::discount`] is private,
//!   but its value equals `term_structure()->discount(paymentTime)` by
//!   construction (`hullwhite.cpp:76-78`), so the port reads the curve handle.
//! - **`Option`-typed forwards and strikes are checked, not unwrapped.** A
//!   still-live coupon always has a forward (`setup_arguments` fills it whenever
//!   `end_date >= today`, `capfloor.cpp:245`, and a coupon reaching the pricing
//!   branch has `payment_time >= 0`), and a cap/collar always has a cap rate (a
//!   floor/collar a floor rate); the port `Err`s rather than `unwrap`s if any is
//!   absent, mirroring C++'s implicit invariants without a panic path.

use crate::errors::QlResult;
use crate::fail;
use crate::instrument::InstrumentResults;
use crate::instruments::{CapFloorArguments, CapFloorType};
use crate::models::model::CalibratedModelHolder;
use crate::models::shortrate::hullwhite::HullWhite;
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::time::date::Date;

/// Analytic cap/floor engine under [`HullWhite`]
/// (`analyticcapfloorengine.hpp:35`).
pub struct AnalyticCapFloorEngine {
    base: GenericEngine<CapFloorArguments, InstrumentResults>,
    model: SharedMut<HullWhite>,
    settings: Shared<Settings<Date>>,
}

impl AnalyticCapFloorEngine {
    /// Builds the engine over a Hull-White `model`
    /// (`analyticcapfloorengine.hpp:43`). The engine observes the model's
    /// observable, so a model or curve change invalidates a cap/floor priced by
    /// it. `settings` supplies the evaluation date and the reference-date-event
    /// gates the C++ engine reads from the global singleton.
    pub fn new(
        model: SharedMut<HullWhite>,
        settings: Shared<Settings<Date>>,
    ) -> AnalyticCapFloorEngine {
        let base = GenericEngine::new(CapFloorArguments::default(), InstrumentResults::default());
        base.register_with(model.borrow().calibrated_model().observable());
        AnalyticCapFloorEngine {
            base,
            model,
            settings,
        }
    }
}

impl AsObservable for AnalyticCapFloorEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for AnalyticCapFloorEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        let model = self.model.borrow();
        let (reference_date, day_counter) = {
            let curve = model.term_structure().current_link()?;
            (curve.reference_date()?, curve.require_day_counter()?)
        };

        // includeRefDatePayments, overridden by includeTodaysCashFlows only when
        // the reference date is the evaluation date (`:54-61`).
        let mut include_ref_date_payments = self.settings.include_reference_date_events();
        if Some(reference_date) == self.settings.evaluation_date()
            && let Some(include_todays) = self.settings.include_todays_cash_flows()
        {
            include_ref_date_payments = include_todays;
        }

        let arguments = self.base.arguments();
        let Some(cap_floor_type) = arguments.cap_floor_type else {
            fail!("cap/floor type not set");
        };
        let has_cap = matches!(cap_floor_type, CapFloorType::Cap | CapFloorType::Collar);
        let has_floor = matches!(cap_floor_type, CapFloorType::Floor | CapFloorType::Collar);
        let floor_mult = if cap_floor_type == CapFloorType::Floor {
            1.0
        } else {
            -1.0
        };

        let mut value = 0.0;
        for i in 0..arguments.end_dates.len() {
            let fixing_time = day_counter.year_fraction(reference_date, arguments.fixing_dates[i]);
            let payment_time = day_counter.year_fraction(reference_date, arguments.end_dates[i]);
            let not_expired = if include_ref_date_payments {
                payment_time >= 0.0
            } else {
                payment_time > 0.0
            };
            if !not_expired {
                continue;
            }

            let tenor = arguments.accrual_times[i];
            let nominal = arguments.nominals[i];
            let gearing = arguments.gearings[i];

            if fixing_time <= 0.0 {
                let Some(fixing) = arguments.forwards[i] else {
                    fail!("a still-live cap/floor coupon has no forward set");
                };
                let discount = model
                    .term_structure()
                    .current_link()?
                    .discount(payment_time, false)?;
                if has_cap {
                    let Some(strike) = arguments.cap_rates[i] else {
                        fail!("cap rate not set for a cap/collar");
                    };
                    value += discount * nominal * tenor * gearing * (fixing - strike).max(0.0);
                }
                if has_floor {
                    let Some(strike) = arguments.floor_rates[i] else {
                        fail!("floor rate not set for a floor/collar");
                    };
                    value += discount
                        * nominal
                        * tenor
                        * floor_mult
                        * gearing
                        * (strike - fixing).max(0.0);
                }
            } else {
                let maturity = day_counter.year_fraction(reference_date, arguments.start_dates[i]);
                if has_cap {
                    let Some(cap_rate) = arguments.cap_rates[i] else {
                        fail!("cap rate not set for a cap/collar");
                    };
                    let temp = 1.0 + cap_rate * tenor;
                    value += nominal
                        * gearing
                        * temp
                        * model.discount_bond_option(
                            OptionType::Put,
                            1.0 / temp,
                            maturity,
                            payment_time,
                        )?;
                }
                if has_floor {
                    let Some(floor_rate) = arguments.floor_rates[i] else {
                        fail!("floor rate not set for a floor/collar");
                    };
                    let temp = 1.0 + floor_rate * tenor;
                    value += nominal
                        * gearing
                        * temp
                        * floor_mult
                        * model.discount_bond_option(
                            OptionType::Call,
                            1.0 / temp,
                            maturity,
                            payment_time,
                        )?;
                }
            }
        }
        drop(model);

        self.base.results_mut().value = Some(value);
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::{Coupon, IborCoupon, IborLeg};
    use crate::event::Event;
    use crate::handle::Handle;
    use crate::indexes::IborIndex;
    use crate::indexes::ibor::Euribor;
    use crate::indexes::index::Index;
    use crate::indexes::interestrateindex::InterestRateIndex;
    use crate::instrument::Instrument;
    use crate::instruments::{CapFloor, SwapType, VanillaSwap};
    use crate::interestrate::Compounding;
    use crate::pricingengines::DiscountingSwapEngine;
    use crate::shared::{shared, shared_mut};
    use crate::termstructures::yields::FlatForward;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::schedule::{MakeSchedule, Schedule};
    use crate::types::{Rate, Real, Volatility};

    const A: Real = 0.05;
    const SIGMA: Volatility = 0.01;
    const NOMINAL: Real = 100.0;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    /// A flat 3% continuously-compounded `Actual365Fixed` curve anchored at
    /// `reference`.
    fn flat_curve(reference: Date) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            0.03,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn hw_model(curve: Handle<dyn YieldTermStructure>, sigma: Volatility) -> SharedMut<HullWhite> {
        HullWhite::new(curve, A, sigma).unwrap()
    }

    fn schedule(from: Date, to: Date) -> Schedule {
        MakeSchedule::new()
            .from(from)
            .to(to)
            .with_frequency(Frequency::Semiannual)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_termination_date_convention(BusinessDayConvention::ModifiedFollowing)
            .forwards()
            .end_of_month(false)
            .build()
    }

    /// A leg over a Euribor 6M forecasting off `curve`, on a nominal of 100.
    fn leg(
        curve: &Handle<dyn YieldTermStructure>,
        settings: &Shared<Settings<Date>>,
        from: Date,
        to: Date,
    ) -> (Shared<IborIndex>, Vec<Shared<IborCoupon>>) {
        let index: Shared<IborIndex> =
            shared(Euribor::six_months(curve.clone(), Shared::clone(settings)));
        let coupons = IborLeg::new(schedule(from, to), Shared::clone(&index))
            .with_notional(NOMINAL)
            .coupons()
            .unwrap();
        (index, coupons)
    }

    fn priced_cap(
        coupons: Vec<Shared<IborCoupon>>,
        is_cap: bool,
        strike: Rate,
        model: SharedMut<HullWhite>,
        settings: &Shared<Settings<Date>>,
    ) -> CapFloor {
        let mut cf = if is_cap {
            CapFloor::cap(coupons, vec![strike], Shared::clone(settings))
        } else {
            CapFloor::floor(coupons, vec![strike], Shared::clone(settings))
        }
        .unwrap();
        let engine = shared_mut(AnalyticCapFloorEngine::new(model, Shared::clone(settings)))
            as SharedMut<dyn PricingEngine>;
        cf.base_mut().set_pricing_engine(engine);
        cf
    }

    /// The primary-branch put-call parity oracle. For a cap and floor at the same
    /// strike `K` with every fixing in the future, `cap - floor` collapses (via
    /// `black_formula`'s exact internal put-call identity `Put - Call = k - f`) to
    /// the curve-native sum `sum_i N * g * (P(accrualStart) - (1 + K*tau)
    /// P(accrualEnd))`. This pins the `1/temp` strike, the maturity =
    /// `start_dates[i]` wiring, the Put-for-cap / Call-for-floor assignment and
    /// the `temp` scaling all at once; a slip in any of them breaks it. Machine
    /// exact (~1e-12): the two option values cancel coupon by coupon before the
    /// curve arithmetic.
    #[test]
    fn primary_parity_matches_the_curve_native_sum() {
        let today = Date::new(15, Month::January, 2026);
        let settings = settings_on(today);
        let curve = flat_curve(today);
        let k = 0.03;
        let (_index, coupons) = leg(
            &curve,
            &settings,
            Date::new(15, Month::January, 2027),
            Date::new(15, Month::January, 2029),
        );

        let mut cap = priced_cap(
            coupons.clone(),
            true,
            k,
            hw_model(curve.clone(), SIGMA),
            &settings,
        );
        let mut floor = priced_cap(
            coupons.clone(),
            false,
            k,
            hw_model(curve.clone(), SIGMA),
            &settings,
        );
        let cap_npv = cap.npv().unwrap();
        let floor_npv = floor.npv().unwrap();

        let mut args = CapFloorArguments::default();
        cap.setup_arguments(&mut args).unwrap();
        let curve_link = curve.current_link().unwrap();
        let dc = curve_link.require_day_counter().unwrap();
        let mut reference = 0.0;
        for i in 0..args.end_dates.len() {
            let temp = 1.0 + k * args.accrual_times[i];
            let p_start = curve_link
                .discount(dc.year_fraction(today, args.start_dates[i]), false)
                .unwrap();
            let p_end = curve_link
                .discount(dc.year_fraction(today, args.end_dates[i]), false)
                .unwrap();
            reference += args.nominals[i] * args.gearings[i] * (p_start - temp * p_end);
        }

        assert!(
            (cap_npv - floor_npv - reference).abs() < 1.0e-11,
            "primary parity: cap-floor {} vs curve-native {reference} (error {})",
            cap_npv - floor_npv,
            (cap_npv - floor_npv - reference).abs()
        );
    }

    /// Market-side swap parity (`testParity`): under the model engine
    /// `cap - floor` equals the `DiscountingSwapEngine` NPV of the matching payer
    /// swap struck at `K`, and the sign lands on the payer (not receiver) side.
    /// In the default par-coupon mode the coupon forecasts the par rate
    /// `(P(accrualStart)/P(accrualEnd) - 1)/tau` over its own accrual dates, which
    /// is exactly the forward the engine's bond options imply, so the two agree to
    /// ~2e-13 (measured) despite the index's fixing-days value-date shift; only in
    /// indexed-coupon mode would a small value-date residual appear. The
    /// float and fixed legs must share the coupon accrual day counter for the
    /// accrual factors to line up. The exact, model-internal oracle is
    /// `primary_parity_matches_the_curve_native_sum`.
    #[test]
    fn swap_parity_matches_the_payer_swap() {
        let today = Date::new(15, Month::January, 2026);
        let settings = settings_on(today);
        let curve = flat_curve(today);
        let k = 0.03;
        let from = Date::new(15, Month::January, 2027);
        let to = Date::new(15, Month::January, 2029);
        let (index, coupons) = leg(&curve, &settings, from, to);
        let index_dc = index.day_counter().clone();

        let mut cap = priced_cap(
            coupons.clone(),
            true,
            k,
            hw_model(curve.clone(), SIGMA),
            &settings,
        );
        let mut floor = priced_cap(coupons, false, k, hw_model(curve.clone(), SIGMA), &settings);
        let cap_minus_floor = cap.npv().unwrap() - floor.npv().unwrap();

        let sched = schedule(from, to);
        let mut swap = VanillaSwap::new(
            SwapType::Payer,
            NOMINAL,
            sched.clone(),
            k,
            index_dc.clone(),
            sched,
            index,
            0.0,
            index_dc,
            None,
            Shared::clone(&settings),
        )
        .unwrap();
        let engine = shared_mut(DiscountingSwapEngine::new(
            curve.clone(),
            None,
            None,
            None,
            Shared::clone(&settings),
        )) as SharedMut<dyn PricingEngine>;
        swap.base_mut().set_pricing_engine(engine);
        let swap_npv = swap.npv().unwrap();

        assert!(
            cap_minus_floor.signum() == swap_npv.signum(),
            "sign mismatch: cap-floor {cap_minus_floor} vs payer swap {swap_npv}"
        );
        assert!(
            (cap_minus_floor - swap_npv).abs() < 1.0e-10,
            "swap-parity residual too large: cap-floor {cap_minus_floor} vs swap {swap_npv}"
        );
    }

    /// Zero-vol limit: as `sigma -> 0` the primary bond-option branch converges to
    /// the deterministic intrinsic `sum_i N * g * P(accrualEnd) * tau *
    /// max(0, F - K)`, with the curve-native forward `F = (P(start)/P(end) - 1)/tau`
    /// (`black_formula` at `v = 0` returns the option intrinsic). At a live
    /// `sigma = 0.2` the cap is materially richer, proving the branch is genuinely
    /// optional-valued.
    #[test]
    fn zero_vol_converges_to_the_curve_intrinsic() {
        let today = Date::new(15, Month::January, 2026);
        let settings = settings_on(today);
        let curve = flat_curve(today);
        let k = 0.02;
        let (_index, coupons) = leg(
            &curve,
            &settings,
            Date::new(15, Month::January, 2027),
            Date::new(15, Month::January, 2029),
        );

        let mut cap_zero = priced_cap(
            coupons.clone(),
            true,
            k,
            hw_model(curve.clone(), 1.0e-10),
            &settings,
        );
        let npv_zero = cap_zero.npv().unwrap();

        let mut args = CapFloorArguments::default();
        cap_zero.setup_arguments(&mut args).unwrap();
        let curve_link = curve.current_link().unwrap();
        let dc = curve_link.require_day_counter().unwrap();
        let mut intrinsic = 0.0;
        for i in 0..args.end_dates.len() {
            let tau = args.accrual_times[i];
            let p_start = curve_link
                .discount(dc.year_fraction(today, args.start_dates[i]), false)
                .unwrap();
            let p_end = curve_link
                .discount(dc.year_fraction(today, args.end_dates[i]), false)
                .unwrap();
            let forward = (p_start / p_end - 1.0) / tau;
            intrinsic += args.nominals[i] * args.gearings[i] * p_end * tau * (forward - k).max(0.0);
        }
        assert!(
            (npv_zero - intrinsic).abs() < 1.0e-8,
            "zero-vol cap {npv_zero} vs intrinsic {intrinsic} (error {})",
            (npv_zero - intrinsic).abs()
        );

        let mut cap_live = priced_cap(coupons, true, k, hw_model(curve.clone(), 0.2), &settings);
        let npv_live = cap_live.npv().unwrap();
        assert!(
            npv_live - npv_zero > 1.0e-3,
            "a live-vol cap should be materially richer: live {npv_live} vs zero {npv_zero}"
        );
    }

    /// Intrinsic branch: a single-coupon cap whose fixing has already set
    /// (`fixing_time <= 0`, `end_date` still in the future). With the curve
    /// reference date equal to the evaluation date, the past fixing gives
    /// `fixing_time < 0` while `payment_time > 0` keeps the forward `Some`. The
    /// coupon must contribute exactly `P(paymentTime) * N * tau * max(0, F - K)`
    /// off the seeded fixing (`analyticcapfloorengine.cpp:78-85`).
    #[test]
    fn intrinsic_branch_prices_a_past_fixing_by_hand() {
        let reference = Date::new(20, Month::January, 2026);
        let settings = settings_on(reference);
        let curve = flat_curve(reference);
        let k = 0.03;
        let fixing_value: Rate = 0.05;
        let (index, coupons) = leg(
            &curve,
            &settings,
            Date::new(15, Month::January, 2026),
            Date::new(15, Month::July, 2026),
        );
        assert_eq!(coupons.len(), 1);
        let fixing_date = coupons[0].fixing_date();
        assert!(fixing_date < reference, "fixing must be in the past");
        index.add_fixing(fixing_date, fixing_value).unwrap();

        let mut cap = priced_cap(
            coupons.clone(),
            true,
            k,
            hw_model(curve.clone(), SIGMA),
            &settings,
        );
        let npv = cap.npv().unwrap();

        let coupon = &coupons[0];
        let tau = coupon.accrual_period();
        let curve_link = curve.current_link().unwrap();
        let dc = curve_link.require_day_counter().unwrap();
        let payment_time = dc.year_fraction(reference, coupon.date());
        let discount = curve_link.discount(payment_time, false).unwrap();
        let expected = discount * NOMINAL * tau * (fixing_value - k).max(0.0);

        assert!(
            (npv - expected).abs() < 1.0e-12,
            "intrinsic branch: npv {npv} vs hand {expected} (error {})",
            (npv - expected).abs()
        );
    }

    /// The collar branch (`has_cap && has_floor`, `floor_mult = -1`): a collar
    /// equals the cap minus the floor at the same strikes, exercising the
    /// otherwise-untested combined path (`testConsistency`).
    #[test]
    fn collar_equals_the_cap_minus_the_floor() {
        let today = Date::new(15, Month::January, 2026);
        let settings = settings_on(today);
        let curve = flat_curve(today);
        let (cap_rate, floor_rate) = (0.05, 0.02);
        let (_index, coupons) = leg(
            &curve,
            &settings,
            Date::new(15, Month::January, 2027),
            Date::new(15, Month::January, 2029),
        );

        let cap_npv = priced_cap(
            coupons.clone(),
            true,
            cap_rate,
            hw_model(curve.clone(), SIGMA),
            &settings,
        )
        .npv()
        .unwrap();
        let floor_npv = priced_cap(
            coupons.clone(),
            false,
            floor_rate,
            hw_model(curve.clone(), SIGMA),
            &settings,
        )
        .npv()
        .unwrap();

        let mut collar = CapFloor::collar(
            coupons,
            vec![cap_rate],
            vec![floor_rate],
            Shared::clone(&settings),
        )
        .unwrap();
        let engine = shared_mut(AnalyticCapFloorEngine::new(
            hw_model(curve, SIGMA),
            Shared::clone(&settings),
        )) as SharedMut<dyn PricingEngine>;
        collar.base_mut().set_pricing_engine(engine);

        assert!(
            (collar.npv().unwrap() - (cap_npv - floor_npv)).abs() < 1.0e-12,
            "collar {} vs cap-floor {}",
            collar.npv().unwrap(),
            cap_npv - floor_npv
        );
    }
}
