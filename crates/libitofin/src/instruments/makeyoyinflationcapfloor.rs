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

#[cfg(test)]
mod tests {
    //! QuantLib covers neither this factory nor `YoYInflationCapFloor::atmRate`
    //! (no test-suite case constructs either), so the oracle is self-authored:
    //! a factory-built cap has to match the cap the same leg builds by hand, and
    //! an at-the-money strike has to be the rate that reprices the leg it ends
    //! up written on.
    //!
    //! The curve is deliberately steep - year-on-year swaps quoted 2 % to 4 %
    //! across 1y to 5y - so that the whole leg's at-the-money rate and the last
    //! coupon's are far apart. On a flat curve the two coincide and the
    //! trim-before-fill order (`cpp:69-83`) would be unpinnable.

    use super::*;
    use crate::cashflows::{Coupon, YoYInflationCoupon};
    use crate::event::Event;
    use crate::handle::RelinkableHandle;
    use crate::indexes::index::Index;
    use crate::indexes::inflation::UkRpi;
    use crate::indexes::inflationindex::InflationIndex;
    use crate::instruments::inflationcapfloor::YoYInflationCapFloor;
    use crate::interestrate::Compounding;
    use crate::math::interpolations::linear::Linear;
    use crate::quotes::SimpleQuote;
    use crate::shared::shared;
    use crate::termstructures::inflation::inflationhelpers::{
        YearOnYearInflationSwapHelper, YoYInflationHelper,
    };
    use crate::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure;
    use crate::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve;
    use crate::termstructures::yields::{FlatForward, Pillar};
    use crate::time::calendars::unitedkingdom::{self, UnitedKingdom};
    use crate::time::date::Month;
    use crate::time::daycounters::actualactual::{
        ActualActual, Convention as ActualActualConvention,
    };

    const NOTIONAL: Real = 1_000_000.0;
    const LENGTH: Size = 5;

    fn uk() -> Calendar {
        UnitedKingdom::new(unitedkingdom::Market::Settlement)
    }

    fn day_counter() -> DayCounter {
        Thirty360::with_convention(Convention::BondBasis)
    }

    fn observation_lag() -> Period {
        Period::new(2, TimeUnit::Months)
    }

    struct Fixture {
        settings: Shared<Settings<Date>>,
        index: Shared<YoYInflationIndex>,
        nominal: Handle<dyn YieldTermStructure>,
        evaluation_date: Date,
        _curve: Shared<PiecewiseYoYInflationCurve<Linear>>,
        _handle: RelinkableHandle<dyn YoYInflationTermStructure>,
    }

    /// A UK RPI index with monthly history, a 5 % nominal curve, and a
    /// year-on-year curve bootstrapped off five steeply rising swap quotes.
    ///
    /// The evaluation date is a UK business day, so a default build's spot date
    /// is the evaluation date itself and the hand-built comparison leg can start
    /// there without re-deriving it.
    fn a_sloped_market() -> Fixture {
        let settings = shared(Settings::<Date>::new());
        let evaluation_date = Date::new(13, Month::August, 2007);
        settings.set_evaluation_date(evaluation_date);

        let rpi_schedule = MakeSchedule::new()
            .from(Date::new(1, Month::January, 2005))
            .to(Date::new(1, Month::August, 2007))
            .with_tenor(Period::new(1, TimeUnit::Months))
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .with_rule(DateGeneration::Forward)
            .build();
        let rpi = shared(UkRpi::new(Shared::clone(&settings)));
        for (n, date) in rpi_schedule.dates().iter().enumerate() {
            rpi.add_fixing(*date, 190.0 + n as Real)
                .expect("a published figure");
        }

        let handle = RelinkableHandle::<dyn YoYInflationTermStructure>::empty();
        let index = shared(
            YoYInflationIndex::from_underlying(Shared::clone(&rpi))
                .with_term_structure(handle.handle()),
        );
        let nominal = Handle::new(shared(FlatForward::with_rate(
            evaluation_date,
            0.05,
            ActualActual::with_convention(ActualActualConvention::ISDA),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);

        let helpers: Vec<Shared<dyn YoYInflationHelper>> = (1..=5)
            .map(|year| {
                let rate = 0.02 + 0.005 * (year - 1) as Real;
                YearOnYearInflationSwapHelper::new(
                    Handle::new(shared(SimpleQuote::new(Some(rate)))),
                    observation_lag(),
                    uk().advance(
                        evaluation_date,
                        year,
                        TimeUnit::Years,
                        BusinessDayConvention::Unadjusted,
                        false,
                    ),
                    uk(),
                    BusinessDayConvention::ModifiedFollowing,
                    day_counter(),
                    &index,
                    CpiInterpolationType::Flat,
                    nominal.clone(),
                    Pillar::LastRelevantDate,
                    Shared::clone(&settings),
                )
                .expect("a well-formed helper") as Shared<dyn YoYInflationHelper>
            })
            .collect();

        let curve = PiecewiseYoYInflationCurve::<Linear>::new(
            evaluation_date,
            rpi.last_fixing_date().expect("RPI has history"),
            0.02,
            index.frequency(),
            day_counter(),
            helpers,
            None,
        )
        .expect("five helpers");
        handle.link_to(Shared::clone(&curve) as Shared<dyn YoYInflationTermStructure>);

        Fixture {
            settings,
            index,
            nominal,
            evaluation_date,
            _curve: curve,
            _handle: handle,
        }
    }

    /// A builder with the factory defaults, over `LENGTH` years.
    fn a_builder(fixture: &Fixture) -> MakeYoYInflationCapFloor {
        MakeYoYInflationCapFloor::new(
            CapFloorType::Cap,
            Shared::clone(&fixture.index),
            LENGTH,
            uk(),
            observation_lag(),
            CpiInterpolationType::Flat,
            Shared::clone(&fixture.settings),
        )
    }

    /// The leg `build` should produce for the default settings, assembled by
    /// hand off the same schedule.
    fn a_hand_built_leg(fixture: &Fixture) -> Vec<Shared<YoYInflationCoupon>> {
        let start = fixture.evaluation_date;
        let end = uk().advance(
            start,
            LENGTH as Integer,
            TimeUnit::Years,
            BusinessDayConvention::Unadjusted,
            false,
        );
        let schedule = MakeSchedule::new()
            .from(start)
            .to(end)
            .with_frequency(Frequency::Annual)
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::Unadjusted)
            .with_termination_date_convention(BusinessDayConvention::Unadjusted)
            .with_rule(DateGeneration::Forward)
            .build();
        YoYInflationLeg::new(
            schedule,
            uk(),
            Shared::clone(&fixture.index),
            observation_lag(),
            CpiInterpolationType::Flat,
        )
        .with_payment_adjustment(BusinessDayConvention::ModifiedFollowing)
        .with_payment_day_counter(day_counter())
        .with_notional(NOTIONAL)
        .coupons()
        .expect("a well-formed leg")
    }

    fn pay_dates(cap_floor: &YoYInflationCapFloor) -> Vec<Date> {
        cap_floor.yoy_leg().iter().map(|c| c.date()).collect()
    }

    /// The factory's leg is the leg the same schedule builds by hand, coupon for
    /// coupon (`cpp:58-67`). Structure alone pins this: no engine, no
    /// volatility, no NPV.
    #[test]
    fn the_factory_reproduces_a_hand_built_cap() {
        let fixture = a_sloped_market();
        let built = a_builder(&fixture)
            .with_nominal(NOTIONAL)
            .with_strike(0.03)
            .build()
            .expect("an explicit strike is enough to build");
        let by_hand = YoYInflationCapFloor::cap(
            a_hand_built_leg(&fixture),
            vec![0.03],
            Shared::clone(&fixture.settings),
        )
        .expect("a well-formed cap");

        assert_eq!(built.cap_rates(), by_hand.cap_rates());
        assert_eq!(built.yoy_leg().len(), by_hand.yoy_leg().len());
        for (from_factory, by_hand) in built.yoy_leg().iter().zip(by_hand.yoy_leg()) {
            assert_eq!(from_factory.nominal(), by_hand.nominal());
            assert_eq!(from_factory.accrual_period(), by_hand.accrual_period());
            assert_eq!(from_factory.date(), by_hand.date());
            assert_eq!(from_factory.fixing_date(), by_hand.fixing_date());
        }
    }

    /// `atmRate` (`inflationcapfloor.cpp:214-217`) is [`CashFlows::atm_rate`]
    /// over the instrument's own leg, off the curve's reference date.
    #[test]
    fn atm_rate_is_the_leg_repricing_rate() {
        let fixture = a_sloped_market();
        let cap = a_builder(&fixture)
            .with_strike(0.03)
            .build()
            .expect("a well-formed cap");
        let curve = fixture.nominal.current_link().expect("a linked curve");

        let leg: Leg = cap
            .yoy_leg()
            .iter()
            .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
            .collect();
        let by_hand = CashFlows::atm_rate(
            &leg,
            curve.as_ref(),
            &fixture.settings,
            Some(false),
            Some(curve.reference_date().expect("a reference date")),
            None,
            None,
        )
        .expect("a leg with basis-point sensitivity");

        assert_eq!(cap.atm_rate(curve.as_ref()).expect("an atm rate"), by_hand);
    }

    /// Every trimming flag reaches the leg (`cpp:69-76`).
    #[test]
    fn the_trimming_flags_reach_the_leg() {
        let fixture = a_sloped_market();
        let whole = a_builder(&fixture).with_strike(0.03).build().unwrap();
        let trimmed = a_builder(&fixture)
            .with_first_caplet_excluded()
            .with_strike(0.03)
            .build()
            .unwrap();

        assert_eq!(trimmed.yoy_leg().len(), whole.yoy_leg().len() - 1);
        assert_eq!(pay_dates(&trimmed), pay_dates(&whole)[1..]);
    }
}
