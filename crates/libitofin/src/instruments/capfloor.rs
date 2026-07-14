//! Cap, floor and collar instruments.
//!
//! Port of `ql/instruments/capfloor.{hpp,cpp}`. A [`CapFloor`] is an
//! [`Instrument`] over a floating leg plus per-coupon cap and/or floor strike
//! vectors and a [`CapFloorType`]; [`CapFloor::cap`], [`CapFloor::floor`] and
//! [`CapFloor::collar`] are the thin constructors the C++ `Cap`/`Floor`/`Collar`
//! subclasses provide. [`setup_arguments`](Instrument::setup_arguments) fills the
//! [`CapFloorArguments`] a `CapFloor::engine` reads.
//!
//! ## Leg shape
//!
//! C++ holds a generic `Leg` of `FloatingRateCoupon`s and, in `setupArguments`,
//! `dynamic_pointer_cast`s each flow back to a `FloatingRateCoupon`. The port
//! cannot downcast an erased [`Leg`](crate::cashflow::Leg), and the par/indexed
//! forecast that drives the cached price lives only on the concrete
//! [`IborCoupon`] (its mode-aware [`rate`](crate::cashflows::coupon::Coupon::rate)),
//! not on the base `FloatingRateCoupon`. So [`CapFloor`] holds concrete
//! `Vec<Shared<IborCoupon>>`, the same choice [`IborLeg::coupons`] makes for the
//! same reason. The forward the engine prices from is the coupon's adjusted
//! fixing `(rate - spread) / gearing`, read off the mode-aware rate.
//!
//! ## Divergences from QuantLib
//!
//! - The generic `Leg`/`FloatingRateCoupon` surface becomes the concrete
//!   `IborCoupon`, as above; the fixture only ever builds ibor legs.
//! - `MakeCapFloor` is not ported (the fixture constructs `Cap`/`Floor`
//!   directly); nor are `optionlet`, `lastFloatingRateCoupon`, `impliedVolatility`
//!   or `deepUpdate`, none of which the ported tests reach.
//! - The `CapFloor::arguments` bundle keeps only the fields the Black engine
//!   reads: the C++ `startDates`, `spreads` and `indexes` are filled but unread,
//!   so they are omitted here.
//! - The D5 `Settings` handle replaces `Settings::instance()` for the evaluation
//!   date the expiry check and the forward guard read.

use std::any::Any;

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{CashFlows, Coupon, IborCoupon};
use crate::errors::QlResult;
use crate::event::Event;
use crate::instrument::{Instrument, InstrumentBase};
use crate::patterns::observable::AsObservable;
use crate::pricingengine::Arguments;
use crate::settings::Settings;
use crate::shared::Shared;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::types::{Rate, Real, Time};
use crate::{fail, require};

/// Whether the instrument caps, floors or collars its floating leg
/// (`CapFloor::Type`).
#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum CapFloorType {
    /// A cap: long a call on each coupon's rate at the cap strike.
    Cap,
    /// A floor: long a put on each coupon's rate at the floor strike.
    Floor,
    /// A collar: long the cap, short the floor.
    Collar,
}

/// Argument bundle a `CapFloor::engine` prices (`CapFloor::arguments`).
///
/// Per optionlet: the payment date (`end_dates`), fixing date, accrual time,
/// nominal, gearing, the coupon's adjusted forward (`None` for a past-fixing
/// coupon, the C++ `Null<Rate>`), and the de-spread cap and floor strikes
/// (`None` where the type has none).
#[derive(Default)]
pub struct CapFloorArguments {
    /// The instrument type, set by `setup_arguments`.
    pub cap_floor_type: Option<CapFloorType>,
    /// Each coupon's fixing date.
    pub fixing_dates: Vec<Date>,
    /// Each coupon's payment date (the C++ `endDates`).
    pub end_dates: Vec<Date>,
    /// Each coupon's accrual period as a year fraction.
    pub accrual_times: Vec<Time>,
    /// Each coupon's de-spread cap strike, `None` for a pure floor.
    pub cap_rates: Vec<Option<Rate>>,
    /// Each coupon's de-spread floor strike, `None` for a pure cap.
    pub floor_rates: Vec<Option<Rate>>,
    /// Each coupon's adjusted forward, `None` when its fixing has passed.
    pub forwards: Vec<Option<Rate>>,
    /// Each coupon's gearing.
    pub gearings: Vec<Real>,
    /// Each coupon's nominal.
    pub nominals: Vec<Real>,
}

impl Arguments for CapFloorArguments {
    fn validate(&self) -> QlResult<()> {
        let n = self.end_dates.len();
        require!(self.cap_floor_type.is_some(), "cap/floor type not set");
        require!(self.fixing_dates.len() == n, "fixing-date count mismatch");
        require!(self.accrual_times.len() == n, "accrual-time count mismatch");
        require!(self.cap_rates.len() == n, "cap-rate count mismatch");
        require!(self.floor_rates.len() == n, "floor-rate count mismatch");
        require!(self.forwards.len() == n, "forward count mismatch");
        require!(self.gearings.len() == n, "gearing count mismatch");
        require!(self.nominals.len() == n, "nominal count mismatch");
        Ok(())
    }
}

/// A cap, floor or collar over a floating (ibor) leg.
pub struct CapFloor {
    base: InstrumentBase,
    cap_floor_type: CapFloorType,
    coupons: Vec<Shared<IborCoupon>>,
    cap_rates: Vec<Rate>,
    floor_rates: Vec<Rate>,
    settings: Shared<Settings<Date>>,
}

impl CapFloor {
    /// Builds a cap/floor/collar over `coupons`, padding the strike vectors to
    /// the leg length by repeating the last strike (the C++ constructor's
    /// `while (rates.size() < leg.size()) push_back(rates.back())`).
    ///
    /// A `Cap` or `Collar` requires at least one cap rate, a `Floor` or `Collar`
    /// at least one floor rate.
    pub fn new(
        cap_floor_type: CapFloorType,
        coupons: Vec<Shared<IborCoupon>>,
        mut cap_rates: Vec<Rate>,
        mut floor_rates: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<CapFloor> {
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

        let base = InstrumentBase::new();
        for coupon in &coupons {
            base.register_with(coupon.observable());
        }
        settings.register_eval_date_observer(&base.observer());

        Ok(CapFloor {
            base,
            cap_floor_type,
            coupons,
            cap_rates,
            floor_rates,
            settings,
        })
    }

    /// A cap over `coupons` struck at `strikes` (the C++ `Cap`).
    pub fn cap(
        coupons: Vec<Shared<IborCoupon>>,
        strikes: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<CapFloor> {
        CapFloor::new(CapFloorType::Cap, coupons, strikes, Vec::new(), settings)
    }

    /// A floor over `coupons` struck at `strikes` (the C++ `Floor`).
    pub fn floor(
        coupons: Vec<Shared<IborCoupon>>,
        strikes: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<CapFloor> {
        CapFloor::new(CapFloorType::Floor, coupons, Vec::new(), strikes, settings)
    }

    /// A collar over `coupons`, long the cap at `cap_rates` and short the floor
    /// at `floor_rates` (the C++ `Collar`).
    pub fn collar(
        coupons: Vec<Shared<IborCoupon>>,
        cap_rates: Vec<Rate>,
        floor_rates: Vec<Rate>,
        settings: Shared<Settings<Date>>,
    ) -> QlResult<CapFloor> {
        CapFloor::new(
            CapFloorType::Collar,
            coupons,
            cap_rates,
            floor_rates,
            settings,
        )
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

    /// The floating coupons.
    pub fn coupons(&self) -> &[Shared<IborCoupon>] {
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

    /// The at-the-money rate: the fixed rate that reprices the floating leg on
    /// `discount_curve` (`atmRate`).
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

    /// The concrete coupons erased to a [`Leg`] for the [`CashFlows`] analytics.
    fn cash_flows(&self) -> Leg {
        self.coupons
            .iter()
            .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
            .collect()
    }
}

impl Instrument for CapFloor {
    fn base(&self) -> &InstrumentBase {
        &self.base
    }

    fn base_mut(&mut self) -> &mut InstrumentBase {
        &mut self.base
    }

    fn is_expired(&self) -> QlResult<bool> {
        for coupon in self.coupons.iter().rev() {
            if !coupon.has_occurred(&self.settings, None, None)? {
                return Ok(false);
            }
        }
        Ok(true)
    }

    fn setup_arguments(&self, arguments: &mut dyn Arguments) -> QlResult<()> {
        let Some(args) = (arguments as &mut dyn Any).downcast_mut::<CapFloorArguments>() else {
            fail!("wrong argument type");
        };
        let today = match self.settings.evaluation_date() {
            Some(today) => today,
            None => fail!("no evaluation date set: a cap/floor needs a reference date"),
        };

        let n = self.coupons.len();
        args.cap_floor_type = Some(self.cap_floor_type);
        args.fixing_dates = Vec::with_capacity(n);
        args.end_dates = Vec::with_capacity(n);
        args.accrual_times = Vec::with_capacity(n);
        args.cap_rates = Vec::with_capacity(n);
        args.floor_rates = Vec::with_capacity(n);
        args.forwards = Vec::with_capacity(n);
        args.gearings = Vec::with_capacity(n);
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
            let end_date = coupon.date();

            args.fixing_dates.push(coupon.fixing_date());
            args.end_dates.push(end_date);
            args.accrual_times.push(coupon.accrual_period());
            args.nominals.push(coupon.nominal());
            args.gearings.push(gearing);

            // Passed explicitly for precision, but only if the coupon can still
            // pay (`capfloor.cpp:245`): a past-fixing coupon has no forward.
            let forward = if end_date >= today {
                Some((coupon.rate()? - spread) / gearing)
            } else {
                None
            };
            args.forwards.push(forward);

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
    use super::*;
    use crate::cashflows::IborLeg;
    use crate::handle::Handle;
    use crate::indexes::ibor::Euribor;
    use crate::shared::shared;
    use crate::termstructures::yieldtermstructure::YieldTermStructure;
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::{Date, Month};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;

    fn settings_on(today: Date) -> Shared<Settings<Date>> {
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        settings
    }

    /// A three-coupon 18-month leg over an unlinked Euribor 6M.
    fn leg(settings: Shared<Settings<Date>>) -> Vec<Shared<IborCoupon>> {
        let index = shared(Euribor::six_months(
            Handle::<dyn YieldTermStructure>::empty(),
            settings,
        ));
        let schedule = MakeSchedule::new()
            .from(Date::new(15, Month::January, 2026))
            .to(Date::new(15, Month::July, 2027))
            .with_frequency(Frequency::Semiannual)
            .with_calendar(Target::new())
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .build();
        IborLeg::new(schedule, index)
            .with_notional(100.0)
            .coupons()
            .unwrap()
    }

    #[test]
    fn a_cap_pads_the_strike_to_the_leg_length() {
        let settings = settings_on(Date::new(2, Month::January, 2026));
        let coupons = leg(settings.clone());
        let n = coupons.len();
        let cap = CapFloor::cap(coupons, vec![0.03], settings).unwrap();

        assert_eq!(cap.cap_floor_type(), CapFloorType::Cap);
        assert_eq!(cap.cap_rates(), vec![0.03; n].as_slice());
        assert!(cap.floor_rates().is_empty());
    }

    #[test]
    fn a_collar_keeps_both_padded_strike_vectors() {
        let settings = settings_on(Date::new(2, Month::January, 2026));
        let coupons = leg(settings.clone());
        let n = coupons.len();
        let collar = CapFloor::collar(coupons, vec![0.06], vec![0.02], settings).unwrap();

        assert_eq!(collar.cap_floor_type(), CapFloorType::Collar);
        assert_eq!(collar.cap_rates(), vec![0.06; n].as_slice());
        assert_eq!(collar.floor_rates(), vec![0.02; n].as_slice());
    }

    #[test]
    fn a_cap_needs_at_least_one_rate() {
        let settings = settings_on(Date::new(2, Month::January, 2026));
        let coupons = leg(settings.clone());
        let err = CapFloor::cap(coupons, Vec::new(), settings).err().unwrap();
        assert_eq!(err.message(), "no cap rates given");
    }

    #[test]
    fn a_floor_needs_at_least_one_rate() {
        let settings = settings_on(Date::new(2, Month::January, 2026));
        let coupons = leg(settings.clone());
        let err = CapFloor::floor(coupons, Vec::new(), settings)
            .err()
            .unwrap();
        assert_eq!(err.message(), "no floor rates given");
    }

    /// `CapFloor::isExpired`: expired once every coupon has paid.
    #[test]
    fn a_cap_is_expired_only_once_all_coupons_have_paid() {
        let settings = settings_on(Date::new(2, Month::January, 2026));
        let coupons = leg(settings.clone());
        let cap = CapFloor::cap(coupons, vec![0.03], settings.clone()).unwrap();
        assert!(!cap.is_expired().unwrap());

        settings.set_evaluation_date(Date::new(15, Month::August, 2027));
        assert!(cap.is_expired().unwrap());
    }
}
