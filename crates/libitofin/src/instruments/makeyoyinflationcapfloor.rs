//! Standard market year-on-year inflation cap/floor builder
//! (`MakeYoYInflationCapFloor`).
//!
//! Port of `ql/instruments/makeyoyinflationcapfloor.{hpp,cpp}`: the comfortable
//! way to instantiate a standard market [`YoYInflationCapFloor`]. It derives an
//! annual year-on-year leg from a length in years, a [`YoYInflationIndex`] and a
//! forward start, trims that leg to the optionlets asked for, and strikes it
//! either at an explicit rate or at the money off a nominal curve
//! (`makeyoyinflationcapfloor.cpp:46-89`).
//!
//! ## Trimming happens before the at-the-money fill
//!
//! [`build`](MakeYoYInflationCapFloor::build) drops coupons *before* it resolves
//! an at-the-money strike (`cpp:69-83`):
//! [`with_first_caplet_excluded`](MakeYoYInflationCapFloor::with_first_caplet_excluded)
//! drops the front optionlet, [`as_optionlet`](MakeYoYInflationCapFloor::as_optionlet)
//! keeps only the last, and the strike is then the rate that reprices whatever
//! survives. Filling the strike first would strike a one-coupon optionlet at the
//! whole leg's rate, which is a different instrument on any curve with slope.
//!
//! ## Divergences from QuantLib
//!
//! - The explicit-strike/at-the-money conflict is a *build*-time error rather
//!   than a setter error. C++ guards it at both setters (`cpp:133` "ATM strike
//!   already given", `cpp:141` "explicit strike already given"), which a
//!   consumed-self fluent setter returning `Self` cannot do. [`build`] refuses
//!   the both-given case instead, and also the neither-given case, which C++
//!   reaches as a dereference of its empty nominal-curve handle. The two agree
//!   on which builders are legal; they differ only in when the refusal lands.
//! - `withFirstCapletExcluded` is *declared* upstream (`hpp:49`) and defined in
//!   no translation unit, so calling it in C++ fails to link, even though
//!   `build` reads the flag it would have set (`cpp:69-70`). The port implements
//!   a working setter.
//!
//! [`build`]: MakeYoYInflationCapFloor::build

use crate::cashflow::{CashFlow, Leg};
use crate::cashflows::{CashFlows, YoYInflationLeg};
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::inflationindex::{CpiInterpolationType, YoYInflationIndex};
use crate::instrument::Instrument;
use crate::instruments::capfloor::CapFloorType;
use crate::instruments::inflationcapfloor::YoYInflationCapFloor;
use crate::pricingengine::PricingEngine;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::calendar::Calendar;
use crate::time::date::Date;
use crate::time::dategenerationrule::DateGeneration;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::thirty360::{Convention, Thirty360};
use crate::time::frequency::Frequency;
use crate::time::period::Period;
use crate::time::schedule::MakeSchedule;
use crate::time::timeunit::TimeUnit;
use crate::types::{Integer, Natural, Rate, Real, Size};

/// Builder for a standard market year-on-year inflation cap or floor.
pub struct MakeYoYInflationCapFloor {
    cap_floor_type: CapFloorType,
    index: Shared<YoYInflationIndex>,
    length: Size,
    calendar: Calendar,
    observation_lag: Period,
    interpolation: CpiInterpolationType,
    strike: Option<Rate>,
    nominal: Real,
    roll: BusinessDayConvention,
    day_counter: DayCounter,
    fixing_days: Natural,
    first_caplet_excluded: bool,
    as_optionlet: bool,
    effective_date: Option<Date>,
    forward_start: Period,
    nominal_term_structure: Handle<dyn YieldTermStructure>,
    engine: Option<SharedMut<dyn PricingEngine>>,
    settings: Shared<Settings<Date>>,
}

impl MakeYoYInflationCapFloor {
    /// Starts a builder for a `cap_floor_type` cap/floor of `length` years on
    /// `index`, observed `observation_lag` back under `interpolation` and paying
    /// on `calendar` (`makeyoyinflationcapfloor.cpp:29-38`).
    ///
    /// The defaults are the C++ member initialisers (`hpp:72-80`): a 1,000,000
    /// nominal, a `ModifiedFollowing` payment roll, a 30/360 bond-basis day
    /// counter, no fixing days, every optionlet kept, and no strike - so a
    /// builder left as it comes needs either
    /// [`with_strike`](Self::with_strike) or
    /// [`with_atm_strike`](Self::with_atm_strike) before it will build.
    /// `settings` carries the evaluation date (D5).
    pub fn new(
        cap_floor_type: CapFloorType,
        index: Shared<YoYInflationIndex>,
        length: Size,
        calendar: Calendar,
        observation_lag: Period,
        interpolation: CpiInterpolationType,
        settings: Shared<Settings<Date>>,
    ) -> MakeYoYInflationCapFloor {
        MakeYoYInflationCapFloor {
            cap_floor_type,
            index,
            length,
            calendar,
            observation_lag,
            interpolation,
            strike: None,
            nominal: 1_000_000.0,
            roll: BusinessDayConvention::ModifiedFollowing,
            day_counter: Thirty360::with_convention(Convention::BondBasis),
            fixing_days: 0,
            first_caplet_excluded: false,
            as_optionlet: false,
            effective_date: None,
            forward_start: Period::new(0, TimeUnit::Days),
            nominal_term_structure: Handle::<dyn YieldTermStructure>::empty(),
            engine: None,
            settings,
        }
    }

    /// Sets the nominal every coupon carries (`withNominal`, `cpp:91-94`).
    pub fn with_nominal(mut self, nominal: Real) -> MakeYoYInflationCapFloor {
        self.nominal = nominal;
        self
    }

    /// Sets the start date outright, bypassing the spot-plus-forward-start
    /// derivation (`withEffectiveDate`, `cpp:96-100`).
    pub fn with_effective_date(mut self, effective_date: Date) -> MakeYoYInflationCapFloor {
        self.effective_date = Some(effective_date);
        self
    }

    /// Sets the day counter the coupons accrue with
    /// (`withPaymentDayCounter`, `cpp:108-112`).
    pub fn with_payment_day_counter(mut self, day_counter: DayCounter) -> MakeYoYInflationCapFloor {
        self.day_counter = day_counter;
        self
    }

    /// Sets the convention the payment dates are adjusted with
    /// (`withPaymentAdjustment`, `cpp:102-106`).
    pub fn with_payment_adjustment(
        mut self,
        convention: BusinessDayConvention,
    ) -> MakeYoYInflationCapFloor {
        self.roll = convention;
        self
    }

    /// Sets how many business days after the evaluation date the leg starts
    /// (`withFixingDays`, `cpp:114-118`). Only the start date reads this; the
    /// coupons keep their own fixing-days default.
    pub fn with_fixing_days(mut self, fixing_days: Natural) -> MakeYoYInflationCapFloor {
        self.fixing_days = fixing_days;
        self
    }

    /// Sets the engine installed on the built cap/floor
    /// (`withPricingEngine`, `cpp:124-128`).
    pub fn with_pricing_engine(
        mut self,
        engine: SharedMut<dyn PricingEngine>,
    ) -> MakeYoYInflationCapFloor {
        self.engine = Some(engine);
        self
    }

    /// Keeps only the last optionlet when `as_optionlet` is set
    /// (`asOptionlet`, `cpp:120-123`).
    pub fn as_optionlet(mut self, as_optionlet: bool) -> MakeYoYInflationCapFloor {
        self.as_optionlet = as_optionlet;
        self
    }

    /// Sets how long after spot the leg starts (`withForwardStart`,
    /// `cpp:146-150`).
    pub fn with_forward_start(mut self, forward_start: Period) -> MakeYoYInflationCapFloor {
        self.forward_start = forward_start;
        self
    }

    /// Drops the front optionlet (`cpp:69-70`; see the module docs for the
    /// upstream setter that is declared but never defined).
    pub fn with_first_caplet_excluded(mut self) -> MakeYoYInflationCapFloor {
        self.first_caplet_excluded = true;
        self
    }

    /// Strikes the cap/floor at `strike` (`withStrike`, `cpp:130-135`).
    ///
    /// Conflicts with [`with_atm_strike`](Self::with_atm_strike); the two
    /// together are refused by [`build`](Self::build), not here.
    pub fn with_strike(mut self, strike: Rate) -> MakeYoYInflationCapFloor {
        self.strike = Some(strike);
        self
    }

    /// Strikes the cap/floor at the money on `nominal_term_structure`
    /// (`withAtmStrike`, `cpp:137-144`).
    ///
    /// The rate is the one that reprices the *trimmed* leg; see the module docs.
    /// Conflicts with [`with_strike`](Self::with_strike); the two together are
    /// refused by [`build`](Self::build), not here.
    pub fn with_atm_strike(
        mut self,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> MakeYoYInflationCapFloor {
        self.nominal_term_structure = nominal_term_structure;
        self
    }

    /// Builds the cap/floor (C++ `operator shared_ptr<YoYInflationCapFloor>()`,
    /// `cpp:46-89`).
    ///
    /// Derives the start date from the evaluation date, the fixing days and the
    /// forward start unless [`with_effective_date`](Self::with_effective_date)
    /// set it; runs an annual unadjusted schedule out to `length` years; builds
    /// the year-on-year leg; trims it; resolves the strike on what is left; and
    /// installs the pricing engine when one was set.
    ///
    /// # Errors
    ///
    /// When the start date has to be derived and no evaluation date is set;
    /// when both or neither of an explicit strike and an at-the-money curve were
    /// given; and as the leg construction, the at-the-money rate and the
    /// [`YoYInflationCapFloor`] construction do.
    pub fn build(self) -> QlResult<YoYInflationCapFloor> {
        let start_date = match self.effective_date {
            Some(effective_date) => effective_date,
            None => {
                let reference_date = match self.settings.evaluation_date() {
                    Some(today) => today,
                    None => crate::fail!(
                        "no evaluation date set: MakeYoYInflationCapFloor needs a reference date to derive the start date"
                    ),
                };
                let spot_date = self.calendar.advance(
                    reference_date,
                    self.fixing_days as Integer,
                    TimeUnit::Days,
                    BusinessDayConvention::Following,
                    false,
                );
                spot_date + self.forward_start
            }
        };

        let end_date = self.calendar.advance(
            start_date,
            self.length as Integer,
            TimeUnit::Years,
            BusinessDayConvention::Unadjusted,
            false,
        );
        let schedule = MakeSchedule::new()
            .from(start_date)
            .to(end_date)
            .with_frequency(Frequency::Annual)
            .with_calendar(self.calendar.clone())
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .with_rule(DateGeneration::Forward)
            .build();

        let mut coupons = YoYInflationLeg::new(
            schedule,
            self.calendar,
            self.index,
            self.observation_lag,
            self.interpolation,
        )
        .with_payment_adjustment(self.roll)
        .with_payment_day_counter(self.day_counter)
        .with_notional(self.nominal)
        .coupons()?;

        if self.first_caplet_excluded && !coupons.is_empty() {
            coupons.remove(0);
        }
        if self.as_optionlet && coupons.len() > 1 {
            coupons.drain(..coupons.len() - 1);
        }

        let strikes = match (self.strike, self.nominal_term_structure.is_empty()) {
            (Some(_), false) => {
                crate::fail!("explicit strike and ATM curve both given")
            }
            (Some(strike), true) => vec![strike],
            (None, false) => {
                let curve = self.nominal_term_structure.current_link()?;
                let reference = curve.reference_date()?;
                let leg: Leg = coupons
                    .iter()
                    .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
                    .collect();
                vec![CashFlows::atm_rate(
                    &leg,
                    curve.as_ref(),
                    &self.settings,
                    Some(false),
                    Some(reference),
                    None,
                    None,
                )?]
            }
            (None, true) => crate::fail!("no strike and no ATM curve given"),
        };

        let mut cap_floor = YoYInflationCapFloor::with_strikes(
            self.cap_floor_type,
            coupons,
            strikes,
            self.settings,
        )?;
        if let Some(engine) = self.engine {
            cap_floor.base_mut().set_pricing_engine(engine);
        }
        Ok(cap_floor)
    }
}
