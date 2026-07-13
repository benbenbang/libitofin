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
            self.leg_npv = results.leg_npv.iter().map(|&v| Some(v)).collect();
        }

        if results.leg_bps.is_empty() {
            self.leg_bps.iter_mut().for_each(|v| *v = None);
        } else {
            require!(
                results.leg_bps.len() == self.leg_bps.len(),
                "wrong number of leg BPS returned"
            );
            self.leg_bps = results.leg_bps.iter().map(|&v| Some(v)).collect();
        }

        if results.start_discounts.is_empty() {
            self.start_discounts.iter_mut().for_each(|v| *v = None);
        } else {
            require!(
                results.start_discounts.len() == self.start_discounts.len(),
                "wrong number of leg start discounts returned"
            );
            self.start_discounts = results.start_discounts.iter().map(|&v| Some(v)).collect();
        }

        if results.end_discounts.is_empty() {
            self.end_discounts.iter_mut().for_each(|v| *v = None);
        } else {
            require!(
                results.end_discounts.len() == self.end_discounts.len(),
                "wrong number of leg end discounts returned"
            );
            self.end_discounts = results.end_discounts.iter().map(|&v| Some(v)).collect();
        }

        self.npv_date_discount = results.npv_date_discount;
        Ok(())
    }
}
