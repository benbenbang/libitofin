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
//! `impliedVolatility` is not ported. C++ leaves it a
//! `QL_FAIL("not implemented yet")` (`hpp:161-170`), so there is no oracle for
//! it, and the solver it would need rests on a stripped/interpolated volatility
//! hierarchy that is itself unported; the omission follows
//! [`CapFloor`](super::CapFloor), `Swaption` and `OneAssetOption`, which all
//! defer theirs the same way rather than carry an always-failing method.
//! `lastYoYInflationCoupon` (`.cpp:109-115`) is tracked on `#856`.
//!
//! The builder these were deferred alongside is now
//! [`MakeYoYInflationCapFloor`](super::MakeYoYInflationCapFloor).

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
use crate::termstructures::yieldtermstructure::YieldTermStructure;
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

    /// The at-the-money rate: the strike at which the leg reprices on
    /// `discount_curve` (`atmRate`, `.cpp:214-217`).
    ///
    /// # Errors
    ///
    /// As [`CashFlows::atm_rate`] does, and when the curve has no reference
    /// date.
    pub fn atm_rate(&self, discount_curve: &dyn YieldTermStructure) -> QlResult<Rate> {
        let reference = discount_curve.reference_date()?;
        CashFlows::atm_rate(
            &self.cash_flows(),
            discount_curve,
            &self.settings,
            Some(false),
            Some(reference),
            None,
            None,
        )
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

#[cfg(test)]
mod tests {
    //! The prices are pinned by the instrument oracle
    //! (`test-suite/inflationcapfloor.cpp`, ported alongside the engine); what
    //! is here is the structure the oracle's flat-gearing fixture cannot see -
    //! the strike padding, the refusals, and the de-gearing.

    use super::*;
    use crate::cashflows::YoYInflationLeg;
    use crate::handle::Handle;
    use crate::indexes::inflation::UkRpi;
    use crate::indexes::inflationindex::CpiInterpolationType;
    use crate::shared::shared;
    use crate::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::unitedkingdom::{self, UnitedKingdom};
    use crate::time::date::Month;
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;
    use crate::time::timeunit::TimeUnit;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    /// A three-coupon annual leg over an unlinked UK RPI year-on-year index.
    /// Nothing here prices, so the empty curve handle is never read.
    fn leg(
        settings: &Shared<Settings<Date>>,
        gearing: Real,
        spread: Real,
    ) -> Vec<Shared<YoYInflationCoupon>> {
        let rpi = shared(UkRpi::new(Shared::clone(settings)));
        let index = shared(
            YoYInflationIndex::from_underlying(rpi)
                .with_term_structure(Handle::<dyn YoYInflationTermStructure>::empty()),
        );
        let calendar = UnitedKingdom::new(unitedkingdom::Market::Settlement);
        let schedule = MakeSchedule::new()
            .from(Date::new(13, Month::August, 2007))
            .to(Date::new(13, Month::August, 2010))
            .with_frequency(Frequency::Annual)
            .with_calendar(calendar.clone())
            .with_convention(BusinessDayConvention::Unadjusted)
            .build();
        YoYInflationLeg::new(
            schedule,
            calendar,
            index,
            Period::new(2, TimeUnit::Months),
            CpiInterpolationType::Flat,
        )
        .with_notional(1_000_000.0)
        .with_payment_day_counter(Thirty360::with_convention(Convention::BondBasis))
        .with_gearing(gearing)
        .with_spread(spread)
        .coupons()
        .expect("a well-formed leg")
    }

    #[test]
    fn a_cap_pads_the_strike_to_the_leg_length() {
        let settings = settings_on(Date::new(13, Month::August, 2007));
        let coupons = leg(&settings, 1.0, 0.0);
        let n = coupons.len();
        let cap = YoYInflationCapFloor::cap(coupons, vec![0.03], settings).unwrap();

        assert_eq!(cap.cap_floor_type(), CapFloorType::Cap);
        assert_eq!(cap.cap_rates(), vec![0.03; n].as_slice());
        assert!(cap.floor_rates().is_empty());
    }

    #[test]
    fn a_collar_cannot_be_built_from_one_strike_vector() {
        let settings = settings_on(Date::new(13, Month::August, 2007));
        let coupons = leg(&settings, 1.0, 0.0);
        let err =
            YoYInflationCapFloor::with_strikes(CapFloorType::Collar, coupons, vec![0.03], settings)
                .err()
                .expect("a collar needs both vectors");
        assert_eq!(
            err.message(),
            "only Cap/Floor types allowed in this constructor"
        );
    }

    #[test]
    fn a_cap_needs_at_least_one_rate() {
        let settings = settings_on(Date::new(13, Month::August, 2007));
        let coupons = leg(&settings, 1.0, 0.0);
        let err = YoYInflationCapFloor::cap(coupons, Vec::new(), settings)
            .err()
            .expect("a cap needs a strike");
        assert_eq!(err.message(), "no cap rates given");
    }

    /// `setupArguments` pulls the strike back onto the bare index forward the
    /// engine prices on, `(rate - spread) / gearing` (`.cpp:170`, `:175`), and
    /// leaves the unused side `None` (`.cpp:172`, `:177`).
    #[test]
    fn setup_arguments_de_gears_the_strike() {
        let settings = settings_on(Date::new(13, Month::August, 2007));
        let coupons = leg(&settings, 2.0, 0.01);
        let cap = YoYInflationCapFloor::cap(coupons, vec![0.05], settings).unwrap();

        let mut args = YoYInflationCapFloorArguments::default();
        cap.setup_arguments(&mut args).unwrap();
        args.validate().expect("every vector spans the leg");

        assert!(args.cap_rates.iter().all(|rate| *rate == Some(0.02)));
        assert!(args.floor_rates.iter().all(Option::is_none));
        assert!(args.gearings.iter().all(|gearing| *gearing == 2.0));
    }

    /// `optionlet(n)` keeps the parent's type and carries that coupon's strike.
    #[test]
    fn an_optionlet_carries_one_coupon_and_its_own_strike() {
        let settings = settings_on(Date::new(13, Month::August, 2007));
        let coupons = leg(&settings, 1.0, 0.0);
        let collar =
            YoYInflationCapFloor::collar(coupons, vec![0.06], vec![0.02], settings).unwrap();

        let optionlet = collar.optionlet(1).unwrap();
        assert_eq!(optionlet.cap_floor_type(), CapFloorType::Collar);
        assert_eq!(optionlet.yoy_leg().len(), 1);
        assert_eq!(optionlet.cap_rates(), [0.06].as_slice());
        assert_eq!(optionlet.floor_rates(), [0.02].as_slice());

        let err = collar.optionlet(3).err().expect("only three coupons");
        assert!(err.message().contains("does not exist"), "err was: {err}");
    }

    /// `validate` rejects a per-coupon vector that has desynced from the leg.
    #[test]
    fn validate_rejects_a_desynced_nominal_count() {
        let settings = settings_on(Date::new(13, Month::August, 2007));
        let coupons = leg(&settings, 1.0, 0.0);
        let cap = YoYInflationCapFloor::cap(coupons, vec![0.03], settings).unwrap();

        let mut args = YoYInflationCapFloorArguments::default();
        cap.setup_arguments(&mut args).unwrap();
        args.nominals.pop();
        assert_eq!(
            args.validate().expect_err("a short vector").message(),
            "nominal count mismatch"
        );
    }
}
