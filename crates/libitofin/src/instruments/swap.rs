//! Base swap instrument.
//!
//! Port of `ql/instruments/swap.{hpp,cpp}`: the abstract [`Swap`] an
//! interest-rate swap derives from. It is an [`Instrument`] holding N [`Leg`]s
//! with a per-leg payer multiplier, and it prices through a
//! [`Swap::engine`](SwapEngine) that returns each leg's NPV and BPS plus the
//! discount factors framing the valuation.
//!
//! `Swap` has no standalone oracle: it is abstract, and the numbers are pinned
//! by the derived `VanillaSwap` + `DiscountingSwapEngine`. This module ports the
//! interface header-faithfully.
//!
//! Deviations, all by existing design decisions or the inheritance-to-composition
//! shift:
//! - The `Swap::arguments`, `Swap::results` and `Swap::engine` inner classes
//!   become the free [`SwapArguments`], [`SwapResults`] and [`SwapEngine`].
//! - The `Null<Real>`/`Null<DiscountFactor>` "result not available" sentinels
//!   become [`Option`] (D4/D10): a leg result the engine did not provide is
//!   `None`, and its accessor returns `Err("result not available")`.
//! - The constructor's `registerWith(Settings::instance().evaluationDate())` has
//!   no singleton to reach (D5), so the owner wires the swap to its [`Settings`]
//!   evaluation date, as `Bond` does.
//! - The protected `Swap(Size numberOfLegs)` staged-build constructor
//!   (`swap.hpp:126`) and the protected mutable `legs_`/`payer_` members are not
//!   ported: they exist so a derived `FixedVsFloatingSwap` can `Swap(2)` and then
//!   fill `legs_[0]`/`payer_` while leaving `legs_[1]` for a further-derived
//!   `VanillaSwap`, a staged build across two inheritance levels that Rust
//!   composition does not mirror. A derived swap builds its legs and constructs
//!   the base whole through [`Swap::new`].

use std::any::Any;
use std::fmt;

use crate::cashflow::Leg;
use crate::cashflows::CashFlows;
use crate::errors::QlResult;
use crate::instrument::{Instrument, InstrumentBase, InstrumentResults};
use crate::pricingengine::{Arguments, GenericEngine, Results};
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::types::{DiscountFactor, Real};
use crate::utilities::null::Null;
use crate::{fail, require};

/// Whether a two-leg swap is seen from the receiver or the payer of the leg it
/// is named for (the C++ `Swap::Type`, `swap.hpp:50`).
///
/// The values are the sign a derived swap carries for that leg; the base
/// [`Swap`] stores the resolved per-leg multiplier in `payer`.
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum SwapType {
    /// The leg is received (`-1`).
    Receiver = -1,
    /// The leg is paid (`+1`).
    Payer = 1,
}

impl fmt::Display for SwapType {
    fn fmt(&self, out: &mut fmt::Formatter<'_>) -> fmt::Result {
        match self {
            SwapType::Payer => out.write_str("Payer"),
            SwapType::Receiver => out.write_str("Receiver"),
        }
    }
}

/// Arguments passed to a swap pricing engine (the C++ `Swap::arguments`).
#[derive(Default)]
pub struct SwapArguments {
    /// The swap's legs.
    pub legs: Vec<Leg>,
    /// The per-leg multiplier: `-1` for a paid leg, `+1` for a received one.
    pub payer: Vec<Real>,
}

impl Arguments for SwapArguments {
    fn validate(&self) -> QlResult<()> {
        require!(
            self.legs.len() == self.payer.len(),
            "number of legs and multipliers differ"
        );
        Ok(())
    }
}

/// Results returned by a swap pricing engine (the C++ `Swap::results`).
///
/// An empty leg vector means the engine did not provide that result; a filled
/// one must have exactly one entry per leg. [`npv_date_discount`] is `None` when
/// unprovided (the C++ `Null<DiscountFactor>` sentinel).
///
/// [`npv_date_discount`]: SwapResults::npv_date_discount
#[derive(Default)]
pub struct SwapResults {
    /// The instrument-level results (NPV and the rest).
    pub instrument: InstrumentResults,
    /// Each leg's NPV.
    pub leg_npv: Vec<Real>,
    /// Each leg's BPS.
    pub leg_bps: Vec<Real>,
    /// The discount factor at each leg's start date.
    pub start_discounts: Vec<DiscountFactor>,
    /// The discount factor at each leg's end date.
    pub end_discounts: Vec<DiscountFactor>,
    /// The discount factor at the NPV date.
    pub npv_date_discount: Option<DiscountFactor>,
}

impl Results for SwapResults {
    fn reset(&mut self) {
        self.instrument.reset();
        self.leg_npv.clear();
        self.leg_bps.clear();
        self.start_discounts.clear();
        self.end_discounts.clear();
        self.npv_date_discount = None;
    }

    fn as_instrument_results(&self) -> Option<&InstrumentResults> {
        Some(&self.instrument)
    }
}

/// Engine base for swaps (the C++ `Swap::engine`).
pub type SwapEngine = GenericEngine<SwapArguments, SwapResults>;

/// Base swap instrument.
///
/// Holds N [`Leg`]s and a per-leg multiplier (`-1` paid, `+1` received). A
/// derived swap builds its legs and constructs the base whole through
/// [`Swap::new`]; [`two_leg`](Swap::two_leg) is the receiver/payer convenience
/// for the common two-leg case.
pub struct Swap {
    base: InstrumentBase,
    settings: Shared<Settings<Date>>,
    legs: Vec<Leg>,
    payer: Vec<Real>,
    leg_npv: Vec<Option<Real>>,
    leg_bps: Vec<Option<Real>>,
    start_discounts: Vec<Option<DiscountFactor>>,
    end_discounts: Vec<Option<DiscountFactor>>,
    npv_date_discount: Option<DiscountFactor>,
}

impl Swap {
    /// Builds a multi-leg swap from its legs and their payer flags (the C++
    /// multi-leg constructor, `swap.cpp:46`).
    ///
    /// A `true` flag marks a paid leg, stored as the multiplier `-1`; a `false`
    /// flag a received leg, stored as `+1`. The swap registers with the settings
    /// evaluation date and with every flow of every leg.
    ///
    /// # Errors
    ///
    /// The number of payer flags must match the number of legs.
    pub fn new(
        legs: Vec<Leg>,
        payer: Vec<bool>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<Swap> {
        require!(
            payer.len() == legs.len(),
            "size mismatch between payer ({}) and legs ({})",
            payer.len(),
            legs.len()
        );
        let n = legs.len();
        let base = InstrumentBase::new();
        settings.register_eval_date_observer(&base.observer());
        let payer = payer.iter().map(|&p| if p { -1.0 } else { 1.0 }).collect();
        let swap = Swap {
            base,
            settings,
            legs,
            payer,
            leg_npv: vec![Some(0.0); n],
            leg_bps: vec![Some(0.0); n],
            start_discounts: vec![Some(0.0); n],
            end_discounts: vec![Some(0.0); n],
            npv_date_discount: Some(0.0),
        };
        for leg in &swap.legs {
            for flow in leg {
                swap.base.register_with(flow.observable());
            }
        }
        Ok(swap)
    }

    /// Builds a two-leg swap whose first leg is paid and second received (the
    /// C++ two-leg constructor, `swap.cpp:30`).
    pub fn two_leg(first_leg: Leg, second_leg: Leg, settings: Shared<Settings<Date>>) -> Swap {
        Swap::new(vec![first_leg, second_leg], vec![true, false], settings)
            .expect("two legs match two payer flags")
    }

    /// The number of legs.
    pub fn number_of_legs(&self) -> usize {
        self.legs.len()
    }

    /// All the swap's legs.
    pub fn legs(&self) -> &[Leg] {
        &self.legs
    }

    /// The `j`-th leg.
    ///
    /// # Errors
    ///
    /// The index must be within range.
    pub fn leg(&self, j: usize) -> QlResult<&Leg> {
        require!(j < self.legs.len(), "leg #{j} doesn't exist!");
        Ok(&self.legs[j])
    }

    /// Whether the `j`-th leg is paid.
    ///
    /// # Errors
    ///
    /// The index must be within range.
    pub fn payer(&self, j: usize) -> QlResult<bool> {
        require!(j < self.legs.len(), "leg #{j} doesn't exist!");
        Ok(self.payer[j] < 0.0)
    }

    /// The earliest start date over the legs.
    ///
    /// # Errors
    ///
    /// The swap must have at least one leg, each non-empty.
    pub fn start_date(&self) -> QlResult<Date> {
        require!(!self.legs.is_empty(), "no legs given");
        let mut date = CashFlows::start_date(&self.legs[0])?;
        for leg in &self.legs[1..] {
            date = date.min(CashFlows::start_date(leg)?);
        }
        Ok(date)
    }

    /// The latest maturity date over the legs.
    ///
    /// # Errors
    ///
    /// The swap must have at least one leg, each non-empty.
    pub fn maturity_date(&self) -> QlResult<Date> {
        require!(!self.legs.is_empty(), "no legs given");
        let mut date = CashFlows::maturity_date(&self.legs[0])?;
        for leg in &self.legs[1..] {
            date = date.max(CashFlows::maturity_date(leg)?);
        }
        Ok(date)
    }

    /// The `j`-th leg's NPV.
    ///
    /// # Errors
    ///
    /// The index must be within range and the engine must have provided the
    /// value.
    pub fn leg_npv(&mut self, j: usize) -> QlResult<Real> {
        require!(j < self.legs.len(), "leg #{j} doesn't exist!");
        self.calculate()?;
        let Some(value) = self.leg_npv[j] else {
            fail!("result not available");
        };
        Ok(value)
    }

    /// The `j`-th leg's BPS.
    ///
    /// # Errors
    ///
    /// The index must be within range and the engine must have provided the
    /// value.
    pub fn leg_bps(&mut self, j: usize) -> QlResult<Real> {
        require!(j < self.legs.len(), "leg #{j} doesn't exist!");
        self.calculate()?;
        let Some(value) = self.leg_bps[j] else {
            fail!("result not available");
        };
        Ok(value)
    }

    /// The discount factor at the `j`-th leg's start date.
    ///
    /// # Errors
    ///
    /// The index must be within range and the engine must have provided the
    /// value.
    pub fn start_discounts(&mut self, j: usize) -> QlResult<DiscountFactor> {
        require!(j < self.legs.len(), "leg #{j} doesn't exist!");
        self.calculate()?;
        let Some(value) = self.start_discounts[j] else {
            fail!("result not available");
        };
        Ok(value)
    }

    /// The discount factor at the `j`-th leg's end date.
    ///
    /// # Errors
    ///
    /// The index must be within range and the engine must have provided the
    /// value.
    pub fn end_discounts(&mut self, j: usize) -> QlResult<DiscountFactor> {
        require!(j < self.legs.len(), "leg #{j} doesn't exist!");
        self.calculate()?;
        let Some(value) = self.end_discounts[j] else {
            fail!("result not available");
        };
        Ok(value)
    }

    /// The discount factor at the NPV date.
    ///
    /// # Errors
    ///
    /// The engine must have provided the value.
    pub fn npv_date_discount(&mut self) -> QlResult<DiscountFactor> {
        self.calculate()?;
        let Some(value) = self.npv_date_discount else {
            fail!("result not available");
        };
        Ok(value)
    }

    /// Invalidates the cached results and notifies observers (the C++
    /// `Swap::deepUpdate`).
    ///
    /// C++ additionally walks each leg's flows calling `deepUpdate` on them, to
    /// refresh coupons whose pricer caches; the crate's cash-flow surface exposes
    /// no such hook yet, so the leg walk reduces to the `update` step until one
    /// lands.
    pub fn deep_update(&mut self) {
        self.base().observer().borrow_mut().update();
    }
}

impl Instrument for Swap {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }

    fn is_expired(&self) -> QlResult<bool> {
        for leg in &self.legs {
            if !CashFlows::is_expired(leg, &self.settings, None, None)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(arguments) = (arguments as &mut dyn Any).downcast_mut::<SwapArguments>() else {
            fail!("wrong argument type");
        };
        arguments.legs = self.legs.clone();
        arguments.payer = self.payer.clone();
        Ok(())
    }

    fn setup_expired(&mut self) {
        let expired = InstrumentResults {
            value: Some(0.0),
            error_estimate: Some(0.0),
            ..InstrumentResults::default()
        };
        self.base_mut().store_results(&expired);
        self.leg_npv.iter_mut().for_each(|v| *v = Some(0.0));
        self.leg_bps.iter_mut().for_each(|v| *v = Some(0.0));
        self.start_discounts.iter_mut().for_each(|v| *v = Some(0.0));
        self.end_discounts.iter_mut().for_each(|v| *v = Some(0.0));
        self.npv_date_discount = Some(0.0);
    }

    fn fetch_results(&mut self, results: &dyn Results) -> QlResult<()> {
        let Some(results) = (results as &dyn Any).downcast_ref::<SwapResults>() else {
            fail!("wrong result type");
        };
        self.base_mut().store_results(&results.instrument);

        if results.leg_npv.is_empty() {
            self.leg_npv.iter_mut().for_each(|v| *v = None);
        } else {
            require!(
                results.leg_npv.len() == self.leg_npv.len(),
                "wrong number of leg NPV returned"
            );
            self.leg_npv = results
                .leg_npv
                .iter()
                .map(|&v| (!v.is_null()).then_some(v))
                .collect();
        }

        if results.leg_bps.is_empty() {
            self.leg_bps.iter_mut().for_each(|v| *v = None);
        } else {
            require!(
                results.leg_bps.len() == self.leg_bps.len(),
                "wrong number of leg BPS returned"
            );
            self.leg_bps = results
                .leg_bps
                .iter()
                .map(|&v| (!v.is_null()).then_some(v))
                .collect();
        }

        if results.start_discounts.is_empty() {
            self.start_discounts.iter_mut().for_each(|v| *v = None);
        } else {
            require!(
                results.start_discounts.len() == self.start_discounts.len(),
                "wrong number of leg start discounts returned"
            );
            self.start_discounts = results
                .start_discounts
                .iter()
                .map(|&v| (!v.is_null()).then_some(v))
                .collect();
        }

        if results.end_discounts.is_empty() {
            self.end_discounts.iter_mut().for_each(|v| *v = None);
        } else {
            require!(
                results.end_discounts.len() == self.end_discounts.len(),
                "wrong number of leg end discounts returned"
            );
            self.end_discounts = results
                .end_discounts
                .iter()
                .map(|&v| (!v.is_null()).then_some(v))
                .collect();
        }

        self.npv_date_discount = results.npv_date_discount.filter(|v| !v.is_null());
        Ok(())
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflow::CashFlow;
    use crate::cashflows::SimpleCashFlow;
    use crate::patterns::observable::{AsObservable, Observable};
    use crate::pricingengine::PricingEngine;
    use crate::shared::{SharedMut, shared, shared_mut};
    use crate::time::date::Month;

    fn today() -> Date {
        Date::new(7, Month::July, 2026)
    }

    fn settings_today() -> Shared<Settings<Date>> {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        settings
    }

    /// A one-flow leg paying `amount` on `date`.
    fn leg(amount: Real, date: Date) -> Leg {
        vec![shared(SimpleCashFlow::new(amount, date).unwrap()) as Shared<dyn CashFlow>]
    }

    /// A receiver/payer swap: pays 100 in a year, receives 100 in two.
    fn two_leg_swap() -> Swap {
        Swap::two_leg(
            leg(100.0, Date::new(7, Month::July, 2027)),
            leg(100.0, Date::new(7, Month::July, 2028)),
            settings_today(),
        )
    }

    struct StubEngine {
        base: SwapEngine,
        npv: Real,
        leg_npv: Vec<Real>,
        leg_bps: Vec<Real>,
        start_discounts: Vec<DiscountFactor>,
        end_discounts: Vec<DiscountFactor>,
        npv_date_discount: Option<DiscountFactor>,
    }

    impl AsObservable for StubEngine {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl PricingEngine for StubEngine {
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
            let results = self.base.results_mut();
            results.instrument.value = Some(self.npv);
            results.leg_npv = self.leg_npv.clone();
            results.leg_bps = self.leg_bps.clone();
            results.start_discounts = self.start_discounts.clone();
            results.end_discounts = self.end_discounts.clone();
            results.npv_date_discount = self.npv_date_discount;
            Ok(())
        }
    }

    fn engine(
        leg_npv: Vec<Real>,
        leg_bps: Vec<Real>,
        start_discounts: Vec<DiscountFactor>,
        end_discounts: Vec<DiscountFactor>,
        npv_date_discount: Option<DiscountFactor>,
    ) -> SharedMut<StubEngine> {
        shared_mut(StubEngine {
            base: SwapEngine::new(SwapArguments::default(), SwapResults::default()),
            npv: 2.0,
            leg_npv,
            leg_bps,
            start_discounts,
            end_discounts,
            npv_date_discount,
        })
    }

    /// The first leg is paid (multiplier -1) and the second received (+1), the
    /// sign convention `Swap::Swap(firstLeg, secondLeg)` stores.
    #[test]
    fn the_two_leg_ctor_pays_the_first_leg_and_receives_the_second() {
        let swap = two_leg_swap();

        assert_eq!(swap.number_of_legs(), 2);
        assert!(swap.payer(0).unwrap(), "the first leg is paid");
        assert!(!swap.payer(1).unwrap(), "the second leg is received");
    }

    /// A `true` payer flag becomes the multiplier -1 and a `false` one +1.
    #[test]
    fn the_multi_leg_ctor_maps_payer_flags_to_multipliers() {
        let swap = Swap::new(
            vec![
                leg(1.0, today() + 100),
                leg(1.0, today() + 200),
                leg(1.0, today() + 300),
            ],
            vec![false, true, false],
            settings_today(),
        )
        .unwrap();

        assert_eq!(swap.number_of_legs(), 3);
        assert!(!swap.payer(0).unwrap());
        assert!(swap.payer(1).unwrap());
        assert!(!swap.payer(2).unwrap());
    }

    #[test]
    fn a_payer_legs_size_mismatch_is_rejected() {
        let error = Swap::new(
            vec![leg(1.0, today() + 100), leg(1.0, today() + 200)],
            vec![true],
            settings_today(),
        )
        .map(|_| ())
        .unwrap_err();
        assert_eq!(
            error.message(),
            "size mismatch between payer (1) and legs (2)"
        );
    }

    #[test]
    fn out_of_range_leg_indices_are_rejected() {
        let mut swap = two_leg_swap();

        assert_eq!(
            swap.leg(2).map(|_| ()).unwrap_err().message(),
            "leg #2 doesn't exist!"
        );
        assert_eq!(
            swap.payer(2).unwrap_err().message(),
            "leg #2 doesn't exist!"
        );
        assert_eq!(
            swap.leg_npv(2).unwrap_err().message(),
            "leg #2 doesn't exist!"
        );
        assert_eq!(
            swap.start_discounts(2).unwrap_err().message(),
            "leg #2 doesn't exist!"
        );
    }

    /// `startDate` is the earliest and `maturityDate` the latest date over the
    /// legs (plain payments report their payment date).
    #[test]
    fn start_and_maturity_span_the_legs() {
        let swap = two_leg_swap();

        assert_eq!(swap.start_date().unwrap(), Date::new(7, Month::July, 2027));
        assert_eq!(
            swap.maturity_date().unwrap(),
            Date::new(7, Month::July, 2028)
        );
    }

    /// A swap is expired once every flow of every leg has occurred, and live
    /// while any flow is still to pay.
    #[test]
    fn is_expired_tracks_the_legs_flows() {
        let future = two_leg_swap();
        assert!(
            !future.is_expired().unwrap(),
            "both flows are in the future"
        );

        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(8, Month::July, 2028));
        let past = Swap::two_leg(
            leg(100.0, Date::new(7, Month::July, 2027)),
            leg(100.0, Date::new(7, Month::July, 2028)),
            settings,
        );
        assert!(past.is_expired().unwrap(), "both flows have paid");
    }

    /// With an engine that fills every leg result, the accessors return the
    /// provided values and `payer` still reflects the sign convention.
    #[test]
    fn the_accessors_read_the_engine_leg_results() {
        let mut swap = two_leg_swap();
        swap.base_mut().set_pricing_engine(engine(
            vec![-98.0, 99.0],
            vec![-1.0, 1.0],
            vec![1.0, 1.0],
            vec![0.95, 0.90],
            Some(0.99),
        ));

        assert_eq!(swap.npv().unwrap(), 2.0);
        assert_eq!(swap.leg_npv(0).unwrap(), -98.0);
        assert_eq!(swap.leg_npv(1).unwrap(), 99.0);
        assert_eq!(swap.leg_bps(1).unwrap(), 1.0);
        assert_eq!(swap.start_discounts(0).unwrap(), 1.0);
        assert_eq!(swap.end_discounts(1).unwrap(), 0.90);
        assert_eq!(swap.npv_date_discount().unwrap(), 0.99);
    }

    /// A sentinel value INSIDE a non-empty vector is also "result not
    /// available": each C++ accessor checks its element against `Null<Real>`
    /// (`swap.hpp:94-110`), and the engine writes that sentinel for a leg
    /// whose start date precedes the curve reference date (a seasoned leg,
    /// `discountingswapengine.cpp:90-105`).
    #[test]
    fn a_null_sentinel_inside_leg_results_is_not_available() {
        let mut swap = two_leg_swap();
        swap.base_mut().set_pricing_engine(engine(
            vec![Real::null(), 99.0],
            vec![-1.0, 1.0],
            vec![DiscountFactor::null(), 1.0],
            vec![0.95, DiscountFactor::null()],
            Some(DiscountFactor::null()),
        ));

        let err = swap.leg_npv(0).unwrap_err();
        assert!(err.message().contains("result not available"));
        assert_eq!(swap.leg_npv(1).unwrap(), 99.0);

        let err = swap.start_discounts(0).unwrap_err();
        assert!(err.message().contains("result not available"));
        assert_eq!(swap.start_discounts(1).unwrap(), 1.0);

        assert_eq!(swap.end_discounts(0).unwrap(), 0.95);
        assert!(swap.end_discounts(1).is_err());

        assert!(swap.npv_date_discount().is_err());
        assert_eq!(swap.leg_bps(0).unwrap(), -1.0);
    }

    /// An engine that leaves the leg results empty leaves each accessor with
    /// nothing to return: the C++ `Null<Real>` "result not available".
    #[test]
    fn unprovided_leg_results_are_not_available() {
        let mut swap = two_leg_swap();
        swap.base_mut()
            .set_pricing_engine(engine(vec![], vec![], vec![], vec![], None));

        assert_eq!(swap.npv().unwrap(), 2.0);
        assert_eq!(
            swap.leg_npv(0).unwrap_err().message(),
            "result not available"
        );
        assert_eq!(
            swap.leg_bps(0).unwrap_err().message(),
            "result not available"
        );
        assert_eq!(
            swap.npv_date_discount().unwrap_err().message(),
            "result not available"
        );
    }

    /// A leg-count mismatch between the engine's results and the swap's legs is
    /// an error (the C++ `wrong number of leg NPV returned`).
    #[test]
    fn a_wrong_leg_result_count_is_rejected() {
        let mut swap = two_leg_swap();
        swap.base_mut().set_pricing_engine(engine(
            vec![1.0],
            vec![1.0, 1.0],
            vec![1.0, 1.0],
            vec![1.0, 1.0],
            Some(1.0),
        ));

        assert_eq!(
            swap.leg_npv(0).unwrap_err().message(),
            "wrong number of leg NPV returned"
        );
    }

    /// An expired swap short-circuits to a zero value and zero leg results
    /// without consulting an engine (`Swap::setupExpired`).
    #[test]
    fn an_expired_swap_reports_zero() {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(Date::new(8, Month::July, 2028));
        let mut swap = Swap::two_leg(
            leg(100.0, Date::new(7, Month::July, 2027)),
            leg(100.0, Date::new(7, Month::July, 2028)),
            settings,
        );

        assert_eq!(swap.npv().unwrap(), 0.0);
        assert_eq!(swap.leg_npv(0).unwrap(), 0.0);
        assert_eq!(swap.leg_bps(1).unwrap(), 0.0);
        assert_eq!(swap.npv_date_discount().unwrap(), 0.0);
    }

    #[test]
    fn the_arguments_reject_a_legs_payer_mismatch() {
        let mut arguments = SwapArguments {
            legs: vec![leg(1.0, today() + 100)],
            payer: vec![-1.0, 1.0],
        };
        assert_eq!(
            arguments.validate().unwrap_err().message(),
            "number of legs and multipliers differ"
        );
        arguments.payer = vec![-1.0];
        assert!(arguments.validate().is_ok());
    }

    #[test]
    fn swap_type_displays_payer_and_receiver() {
        assert_eq!(SwapType::Payer.to_string(), "Payer");
        assert_eq!(SwapType::Receiver.to_string(), "Receiver");
    }
}
