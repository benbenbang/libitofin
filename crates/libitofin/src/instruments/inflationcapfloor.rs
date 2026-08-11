//! Year-on-year inflation cap, floor and collar instruments.
//!
//! Port of `ql/instruments/inflationcapfloor.{hpp,cpp}`. A
//! [`YoYInflationCapFloor`] is an [`Instrument`] over a year-on-year inflation
//! leg plus per-coupon cap and/or floor strike vectors and a
//! [`CapFloorType`](super::CapFloorType); [`cap`](YoYInflationCapFloor::cap),
//! [`floor`](YoYInflationCapFloor::floor) and
//! [`collar`](YoYInflationCapFloor::collar) are the thin constructors the C++
//! `YoYInflationCap`/`Floor`/`Collar` subclasses provide.
//!
//! Unlike a nominal cap/floor this one keeps its *first* optionlet: a nominal
//! coupon sets in advance, so its front optionlet is already determined, while
//! a year-on-year coupon effectively sets in arrears (bar the observation lag),
//! so the front optionlet is live and cap - floor == swap holds without a
//! bespoke instrument definition (`hpp:38-45`).
//!
//! The leg is *plain*. The strikes live here, on the instrument, and reach the
//! engine through [`YoYInflationCapFloorArguments`]; they are deliberately not
//! pushed down into
//! [`CappedFlooredYoYInflationCoupon`](crate::cashflows::CappedFlooredYoYInflationCoupon),
//! which would cap the coupon a second time on top of the engine's optionlet.
//!
//! ## Divergences from QuantLib
//!
//! - C++ holds an erased `Leg` and `dynamic_pointer_cast`s each flow back to a
//!   `YoYInflationCoupon` in `setupArguments` (`.cpp:152-155`); the port cannot
//!   downcast an erased [`Leg`](crate::cashflow::Leg) and so holds the concrete
//!   `Vec<Shared<YoYInflationCoupon>>` that
//!   [`YoYInflationLeg::coupons`](crate::cashflows::YoYInflationLeg::coupons)
//!   hands out.
//! - `CapFloor::Type` is reused as [`CapFloorType`](super::CapFloorType) rather
//!   than respelled; the three cases are the same three.
//! - The D5 `Settings` handle replaces `Settings::instance()` for the
//!   evaluation date the expiry check reads.
//!
//! ## Deferred (visible)
//!
//! `MakeYoYInflationCapFloor`, `atmRate` and `impliedVolatility` (which C++
//! leaves as a `QL_FAIL("not implemented yet")`, `hpp:161-170`) are tracked on
//! `#854`, along with the stripped/interpolated volatility hierarchy the last
//! would need and `lastYoYInflationCoupon` (`.cpp:109-115`), which only the
//! deferred factory reads.

use std::any::Any;

use super::CapFloorType;
use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{CashFlows, Coupon, YoYInflationCoupon};
use crate::errors::QlResult;
use crate::event::Event;
use crate::indexes::inflationindex::YoYInflationIndex;
use crate::instrument::{Instrument, InstrumentBase};
use crate::patterns::observable::AsObservable;
use crate::pricingengine::Arguments;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::time::date::Date;
use crate::time::period::Period;
use crate::types::{Rate, Real, Time};
use crate::{fail, require};

/// Argument bundle a year-on-year cap/floor engine prices
/// (`YoYInflationCapFloor::arguments`, `hpp:132-150`): per optionlet the dates,
/// accrual time, nominal, gearing, spread and the de-geared cap and floor
/// strikes (`None` where the type has none, the C++ `Null<Rate>`).
#[derive(Default)]
pub struct YoYInflationCapFloorArguments {
    /// The instrument type, set by `setup_arguments`.
    pub cap_floor_type: Option<CapFloorType>,
    /// The index the engine reads its forwards off.
    pub index: Option<Shared<YoYInflationIndex>>,
    /// The lag the coupons observe inflation with.
    pub observation_lag: Option<Period>,
    /// Each coupon's accrual start date.
    pub start_dates: Vec<Date>,
    /// Each coupon's fixing date.
    pub fixing_dates: Vec<Date>,
    /// Each coupon's payment date.
    pub pay_dates: Vec<Date>,
    /// Each coupon's accrual period as a year fraction.
    pub accrual_times: Vec<Time>,
    /// Each coupon's de-geared cap strike, `None` for a pure floor.
    pub cap_rates: Vec<Option<Rate>>,
    /// Each coupon's de-geared floor strike, `None` for a pure cap.
    pub floor_rates: Vec<Option<Rate>>,
    /// Each coupon's gearing.
    pub gearings: Vec<Real>,
    /// Each coupon's spread.
    pub spreads: Vec<Real>,
    /// Each coupon's nominal.
    pub nominals: Vec<Real>,
}

impl Arguments for YoYInflationCapFloorArguments {
    /// `arguments::validate` (`.cpp:181-212`): every per-coupon vector must span
    /// the leg. C++ exempts the strike vector the type does not use; here that
    /// vector is filled with `None` rather than left short, so the lengths are
    /// checked unconditionally.
    fn validate(&self) -> QlResult<()> {
        let n = self.start_dates.len();
        require!(self.cap_floor_type.is_some(), "cap/floor type not set");
        require!(self.index.is_some(), "no inflation index given");
        require!(self.pay_dates.len() == n, "pay-date count mismatch");
        require!(self.fixing_dates.len() == n, "fixing-date count mismatch");
        require!(self.accrual_times.len() == n, "accrual-time count mismatch");
        require!(self.cap_rates.len() == n, "cap-rate count mismatch");
        require!(self.floor_rates.len() == n, "floor-rate count mismatch");
        require!(self.gearings.len() == n, "gearing count mismatch");
        require!(self.spreads.len() == n, "spread count mismatch");
        require!(self.nominals.len() == n, "nominal count mismatch");
        Ok(())
    }
}

/// A cap, floor or collar over a year-on-year inflation leg.
pub struct YoYInflationCapFloor {
    base: InstrumentBase,
    cap_floor_type: CapFloorType,
    coupons: Vec<Shared<YoYInflationCoupon>>,
    cap_rates: Vec<Rate>,
    floor_rates: Vec<Rate>,
    index: Shared<YoYInflationIndex>,
    observation_lag: Period,
    settings: Shared<Settings<Date>>,
}

impl YoYInflationCapFloor {
    /// Builds a cap/floor/collar over `coupons`, padding the strike vectors to
    /// the leg length by repeating the last strike (the C++
    /// `while (rates.size() < leg.size()) push_back(rates.back())`, `.cpp:50-61`).
    ///
    /// # Errors
    ///
    /// On an empty leg, or a strike vector the type needs and did not get: a
    /// `Cap` or `Collar` needs a cap rate, a `Floor` or `Collar` a floor rate.
    pub fn new(
        cap_floor_type: CapFloorType,
        coupons: Vec<Shared<YoYInflationCoupon>>,
        mut cap_rates: Vec<Rate>,
        mut floor_rates: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<YoYInflationCapFloor> {
        require!(!coupons.is_empty(), "no coupons given");
        let n = coupons.len();
        if matches!(cap_floor_type, CapFloorType::Cap | CapFloorType::Collar) {
            require!(!cap_rates.is_empty(), "no cap rates given");
            while cap_rates.len() < n {
                cap_rates.push(*cap_rates.last().expect("non-empty"));
            }
        }
        if matches!(cap_floor_type, CapFloorType::Floor | CapFloorType::Collar) {
            require!(!floor_rates.is_empty(), "no floor rates given");
            while floor_rates.len() < n {
                floor_rates.push(*floor_rates.last().expect("non-empty"));
            }
        }

        let front = &coupons[0];
        let index = Shared::clone(front.yoy_index());
        let observation_lag = front.observation_lag();

        let base = InstrumentBase::new();
        for coupon in &coupons {
            base.register_with(coupon.observable());
        }
        settings.register_eval_date_observer(&base.observer());

        Ok(YoYInflationCapFloor {
            base,
            cap_floor_type,
            coupons,
            cap_rates,
            floor_rates,
            index,
            observation_lag,
            settings,
        })
    }

    /// A cap over `coupons` struck at `strikes` (the C++ `YoYInflationCap`).
    pub fn cap(
        coupons: Vec<Shared<YoYInflationCoupon>>,
        strikes: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<YoYInflationCapFloor> {
        YoYInflationCapFloor::new(CapFloorType::Cap, coupons, strikes, Vec::new(), settings)
    }

    /// A floor over `coupons` struck at `strikes` (the C++ `YoYInflationFloor`).
    pub fn floor(
        coupons: Vec<Shared<YoYInflationCoupon>>,
        strikes: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<YoYInflationCapFloor> {
        YoYInflationCapFloor::new(CapFloorType::Floor, coupons, Vec::new(), strikes, settings)
    }

    /// A collar over `coupons`, long the cap at `cap_rates` and short the floor
    /// at `floor_rates` (the C++ `YoYInflationCollar`).
    pub fn collar(
        coupons: Vec<Shared<YoYInflationCoupon>>,
        cap_rates: Vec<Rate>,
        floor_rates: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<YoYInflationCapFloor> {
        YoYInflationCapFloor::new(
            CapFloorType::Collar,
            coupons,
            cap_rates,
            floor_rates,
            settings,
        )
    }

    /// The strikes ctor (`.cpp:69-92`): `strikes` are cap rates for a `Cap` and
    /// floor rates for a `Floor`. A `Collar` needs two vectors and is refused.
    ///
    /// # Errors
    ///
    /// On a `Collar`, and as [`new`](Self::new).
    pub fn with_strikes(
        cap_floor_type: CapFloorType,
        coupons: Vec<Shared<YoYInflationCoupon>>,
        strikes: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<YoYInflationCapFloor> {
        require!(!strikes.is_empty(), "no strikes given");
        match cap_floor_type {
            CapFloorType::Cap => YoYInflationCapFloor::cap(coupons, strikes, settings),
            CapFloorType::Floor => YoYInflationCapFloor::floor(coupons, strikes, settings),
            CapFloorType::Collar => fail!("only Cap/Floor types allowed in this constructor"),
        }
    }

    /// The instrument type.
    pub fn cap_floor_type(&self) -> CapFloorType {
        self.cap_floor_type
    }

    /// The padded cap strikes.
    pub fn cap_rates(&self) -> &[Rate] {
        &self.cap_rates
    }

    /// The padded floor strikes.
    pub fn floor_rates(&self) -> &[Rate] {
        &self.floor_rates
    }

    /// The year-on-year coupons the optionlets are written on (`yoyLeg`).
    pub fn yoy_leg(&self) -> &[Shared<YoYInflationCoupon>] {
        &self.coupons
    }

    /// The leg's earliest accrual start (`startDate`).
    pub fn start_date(&self) -> QlResult<Date> {
        CashFlows::start_date(&self.cash_flows())
    }

    /// The leg's latest accrual end (`maturityDate`).
    pub fn maturity_date(&self) -> QlResult<Date> {
        CashFlows::maturity_date(&self.cash_flows())
    }

    /// The `n`-th optionlet as a cap/floor over that one coupon (`.cpp:117-131`).
    ///
    /// The sub-instrument keeps the parent's type and carries only the strikes
    /// that type uses, so summing the optionlets' NPVs recomposes the parent's.
    ///
    /// # Errors
    ///
    /// When `n` is past the end of the leg.
    pub fn optionlet(&self, n: usize) -> QlResult<YoYInflationCapFloor> {
        require!(
            n < self.coupons.len(),
            "optionlet {n} does not exist, only {}",
            self.coupons.len()
        );
        let mut cap_rates = Vec::new();
        let mut floor_rates = Vec::new();
        if matches!(
            self.cap_floor_type,
            CapFloorType::Cap | CapFloorType::Collar
        ) {
            cap_rates.push(self.cap_rates[n]);
        }
        if matches!(
            self.cap_floor_type,
            CapFloorType::Floor | CapFloorType::Collar
        ) {
            floor_rates.push(self.floor_rates[n]);
        }
        YoYInflationCapFloor::new(
            self.cap_floor_type,
            vec![Shared::clone(&self.coupons[n])],
            cap_rates,
            floor_rates,
            Shared::clone(&self.settings),
        )
    }

    /// The concrete coupons erased to a [`Leg`] for the [`CashFlows`] analytics.
    fn cash_flows(&self) -> Leg {
        self.coupons
            .iter()
            .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
            .collect()
    }
}

impl Instrument for YoYInflationCapFloor {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }

    /// `isExpired` (`.cpp:94-99`): expired once every coupon has paid.
    fn is_expired(&self) -> QlResult<bool> {
        for coupon in self.coupons.iter().rev() {
            if !coupon.has_occurred(&self.settings, None, None)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    /// `setupArguments` (`.cpp:133-179`). The strikes reach the engine
    /// *de-geared*, `(rate - spread) / gearing`: the engine prices an optionlet
    /// on the bare index forward, so a geared coupon's strike has to be pulled
    /// back onto that forward's scale. The strike vector the type does not use
    /// is filled with `None`, C++'s `Null<Rate>`.
    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(args) =
            (arguments as &mut dyn Any).downcast_mut::<YoYInflationCapFloorArguments>()
        else {
            fail!("wrong argument type");
        };

        let n = self.coupons.len();
        args.cap_floor_type = Some(self.cap_floor_type);
        args.index = Some(Shared::clone(&self.index));
        args.observation_lag = Some(self.observation_lag);
        args.start_dates = Vec::with_capacity(n);
        args.fixing_dates = Vec::with_capacity(n);
        args.pay_dates = Vec::with_capacity(n);
        args.accrual_times = Vec::with_capacity(n);
        args.cap_rates = Vec::with_capacity(n);
        args.floor_rates = Vec::with_capacity(n);
        args.gearings = Vec::with_capacity(n);
        args.spreads = Vec::with_capacity(n);
        args.nominals = Vec::with_capacity(n);

        let has_cap = matches!(
            self.cap_floor_type,
            CapFloorType::Cap | CapFloorType::Collar
        );
        let has_floor = matches!(
            self.cap_floor_type,
            CapFloorType::Floor | CapFloorType::Collar
        );

        for (i, coupon) in self.coupons.iter().enumerate() {
            let spread = coupon.spread();
            let gearing = coupon.gearing();

            args.start_dates.push(coupon.accrual_start_date());
            args.fixing_dates.push(coupon.fixing_date());
            args.pay_dates.push(coupon.date());
            args.accrual_times.push(coupon.accrual_period());
            args.nominals.push(coupon.nominal());
            args.gearings.push(gearing);
            args.spreads.push(spread);

            args.cap_rates.push(if has_cap {
                Some((self.cap_rates[i] - spread) / gearing)
            } else {
                None
            });
            args.floor_rates.push(if has_floor {
                Some((self.floor_rates[i] - spread) / gearing)
            } else {
                None
            });
        }
        Ok(())
    }
}
