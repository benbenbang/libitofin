//! Year-on-year inflation cap/floor engines.
//!
//! Port of `ql/pricingengines/inflation/inflationcapfloorengines.{hpp,cpp}`:
//! [`YoYInflationCapFloorEngine`] prices each optionlet of a
//! [`YoYInflationCapFloor`](crate::instruments::YoYInflationCapFloor) against a
//! [`YoYOptionletVolatilitySurface`], discounting on a nominal
//! [`YieldTermStructure`].
//!
//! The engine prices the optionlets *standalone*: it reads each forward off the
//! index's own year-on-year curve rather than through a coupon pricer. C++ says
//! why (`.cpp:74-80`) - the fixing is natural, so there is no convexity
//! adjustment to make, and a convexity correction would need nominal vols and
//! hence a different engine altogether.
//!
//! That is also what makes cap - floor == swap exact rather than approximate.
//! [`yoy_rate_date`](crate::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure::yoy_rate_date)
//! quantizes the date it is given to the start of its inflation period, and the
//! swap coupon's own forecast quantizes to the same period, so both read the
//! curve at one identical point and the intra-period drift cancels.
//!
//! ## Divergences from QuantLib
//!
//! - C++ spells three engine classes differing only in `optionletImpl`
//!   (`.cpp:142-184`); nothing dispatches on the concrete type, so the port
//!   folds them into one engine carrying a
//!   [`YoYOptionletDistribution`], with a constructor apiece - the same shape
//!   `#838` gave the coupon pricers. The distribution switch itself lives once,
//!   in [`yoy_optionlet_price`], which both this engine and that pricer call.
//! - The past-fixing guard is `fixing_date > base_date` where C++ writes
//!   `sqrt(volatility_->timeFromBase(fixingDate)) > 0.0` (`.cpp:86-89`). The
//!   two coincide, and the port's `dyn YoYOptionletVolatilitySurface` carries no
//!   `timeFromBase` - it serves the stripping hierarchy, not the pricing.

use std::any::Any;

use crate::errors::QlResult;
use crate::handle::Handle;
use crate::indexes::Index;
use crate::indexes::inflationindex::YoYInflationIndex;
use crate::instrument::InstrumentResults;
use crate::instruments::{CapFloorType, YoYInflationCapFloorArguments};
use crate::option::OptionType;
use crate::patterns::observable::{AsObservable, Observable};
use crate::pricingengine::{Arguments, GenericEngine, PricingEngine, Results};
use crate::pricingengines::blackformula::{bachelier_black_formula, black_formula};
use crate::shared::{Shared, shared};
use crate::termstructures::volatility::YoYOptionletVolatilitySurface;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::period::Period;
use crate::time::timeunit::TimeUnit;
use crate::types::{Rate, Real};
use crate::{cashflows::YoYOptionletDistribution, fail};

/// The value of one year-on-year optionlet under `distribution`
/// (the three C++ `optionletImpl` overrides, `.cpp:142-184`).
///
/// `discount` carries whatever the caller wants folded in: the engine passes
/// nominal times gearing times discount factor times accrual time, while the
/// coupon pricer, which wants a *rate* rather than a price, passes `1.0`.
///
/// # Errors
///
/// As the underlying formula: a negative std-dev or discount, or a strike the
/// lognormal cases cannot take.
pub fn yoy_optionlet_price(
    distribution: YoYOptionletDistribution,
    option_type: OptionType,
    strike: Rate,
    forward: Rate,
    std_dev: Real,
    discount: Real,
) -> QlResult<Real> {
    match distribution {
        YoYOptionletDistribution::Black => {
            black_formula(option_type, strike, forward, std_dev, discount, 0.0)
        }
        // C++ writes `blackFormula(type, strike + 1, forward + 1, stdDev)`
        // (`.cpp:166-167`); a displacement of 1.0 adds the same 1 to both
        // (`blackformula.rs:115-116`).
        YoYOptionletDistribution::UnitDisplaced => {
            black_formula(option_type, strike, forward, std_dev, discount, 1.0)
        }
        YoYOptionletDistribution::Bachelier => {
            bachelier_black_formula(option_type, strike, forward, std_dev, discount)
        }
    }
}

/// Engine pricing a year-on-year cap, floor or collar optionlet by optionlet.
pub struct YoYInflationCapFloorEngine {
    base: GenericEngine<YoYInflationCapFloorArguments, InstrumentResults>,
    distribution: YoYOptionletDistribution,
    index: Shared<YoYInflationIndex>,
    volatility: Handle<dyn YoYOptionletVolatilitySurface>,
    nominal_term_structure: Handle<dyn YieldTermStructure>,
}

impl YoYInflationCapFloorEngine {
    /// Builds the engine over `index`, `volatility` and a nominal discount
    /// curve, registering for changes in all three (`.cpp:29-38`).
    fn new(
        distribution: YoYOptionletDistribution,
        index: Shared<YoYInflationIndex>,
        volatility: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationCapFloorEngine {
        let base = GenericEngine::new(
            YoYInflationCapFloorArguments::default(),
            InstrumentResults::default(),
        );
        base.register_with(index.observable());
        volatility.register_observer(&base.observer());
        nominal_term_structure.register_observer(&base.observer());
        YoYInflationCapFloorEngine {
            base,
            distribution,
            index,
            volatility,
            nominal_term_structure,
        }
    }

    /// Optionlets under the lognormal model
    /// (`YoYInflationBlackCapFloorEngine`). See [`new`](Self::new).
    pub fn black(
        index: Shared<YoYInflationIndex>,
        volatility: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationCapFloorEngine {
        Self::new(
            YoYOptionletDistribution::Black,
            index,
            volatility,
            nominal_term_structure,
        )
    }

    /// Optionlets under the unit-displaced lognormal model
    /// (`YoYInflationUnitDisplacedBlackCapFloorEngine`). See [`new`](Self::new).
    pub fn unit_displaced(
        index: Shared<YoYInflationIndex>,
        volatility: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationCapFloorEngine {
        Self::new(
            YoYOptionletDistribution::UnitDisplaced,
            index,
            volatility,
            nominal_term_structure,
        )
    }

    /// Optionlets under the normal model
    /// (`YoYInflationBachelierCapFloorEngine`). See [`new`](Self::new).
    pub fn bachelier(
        index: Shared<YoYInflationIndex>,
        volatility: Handle<dyn YoYOptionletVolatilitySurface>,
        nominal_term_structure: Handle<dyn YieldTermStructure>,
    ) -> YoYInflationCapFloorEngine {
        Self::new(
            YoYOptionletDistribution::Bachelier,
            index,
            volatility,
            nominal_term_structure,
        )
    }

    /// The distribution optionlets are valued under.
    pub fn distribution(&self) -> YoYOptionletDistribution {
        self.distribution
    }

    /// The index the forwards are read off.
    pub fn index(&self) -> &Shared<YoYInflationIndex> {
        &self.index
    }

    /// The optionlet volatility surface.
    pub fn volatility(&self) -> &Handle<dyn YoYOptionletVolatilitySurface> {
        &self.volatility
    }

    /// The nominal curve the optionlets are discounted on.
    pub fn nominal_term_structure(&self) -> &Handle<dyn YieldTermStructure> {
        &self.nominal_term_structure
    }
}

impl AsObservable for YoYInflationCapFloorEngine {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}

impl PricingEngine for YoYInflationCapFloorEngine {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        self.base.arguments_mut()
    }

    fn results(&self) -> &dyn Results {
        self.base.results()
    }

    fn reset(&mut self) {
        self.base.reset();
    }

    /// `calculate` (`.cpp:51-128`).
    fn calculate(&mut self) -> QlResult<()> {
        let nominal = self.nominal_term_structure.current_link()?;
        let settlement = nominal.reference_date()?;
        let surface = self.volatility.current_link()?;
        let base_date = surface.base_date()?;
        let yoy_curve = self.index.yoy_inflation_term_structure().current_link()?;
        let no_lag = Period::new(0, TimeUnit::Days);
        let distribution = self.distribution;

        let arguments = self.base.arguments();
        let cap_floor_type = match arguments.cap_floor_type {
            Some(cap_floor_type) => cap_floor_type,
            None => fail!("cap/floor type not set"),
        };
        let has_cap = matches!(cap_floor_type, CapFloorType::Cap | CapFloorType::Collar);
        let has_floor = matches!(cap_floor_type, CapFloorType::Floor | CapFloorType::Collar);

        let n = arguments.start_dates.len();
        let mut values = vec![0.0; n];
        let mut std_devs = vec![0.0; n];
        let mut forwards = vec![0.0; n];
        let mut value = 0.0;

        for i in 0..n {
            // Expired optionlets are discarded but keep their zero entry, so
            // every additional result spans the whole leg.
            if arguments.pay_dates[i] <= settlement {
                continue;
            }
            let discounted_accrual = arguments.nominals[i]
                * arguments.gearings[i]
                * nominal.discount_date(arguments.pay_dates[i], false)?
                * arguments.accrual_times[i];

            let fixing_date = arguments.fixing_dates[i];
            // The natural fixing: the curve's own year-on-year rate, with no
            // convexity adjustment and no extrapolation.
            let forward = yoy_curve.yoy_rate_date(fixing_date, false)?;
            forwards[i] = forward;
            let determined = fixing_date <= base_date;

            let mut optionlet = 0.0;
            if has_cap {
                let strike = arguments.cap_rates[i].expect("cap rate set for cap/collar");
                if !determined {
                    std_devs[i] = surface.total_variance(fixing_date, strike, no_lag)?.sqrt();
                }
                // A determined optionlet keeps std-dev 0, at which every
                // formula collapses to its intrinsic value times the discount.
                optionlet = yoy_optionlet_price(
                    distribution,
                    OptionType::Call,
                    strike,
                    forward,
                    std_devs[i],
                    discounted_accrual,
                )?;
            }
            if has_floor {
                let strike = arguments.floor_rates[i].expect("floor rate set for floor/collar");
                // Re-read at the floor strike: on a smiling surface the two
                // strikes carry different variances (`.cpp:105-108`).
                if !determined {
                    std_devs[i] = surface.total_variance(fixing_date, strike, no_lag)?.sqrt();
                }
                let floorlet = yoy_optionlet_price(
                    distribution,
                    OptionType::Put,
                    strike,
                    forward,
                    std_devs[i],
                    discounted_accrual,
                )?;
                if cap_floor_type == CapFloorType::Floor {
                    optionlet = floorlet;
                } else {
                    // A collar is long the cap and short the floor.
                    optionlet -= floorlet;
                }
            }

            values[i] = optionlet;
            value += optionlet;
        }

        drop(nominal);
        drop(surface);
        drop(yoy_curve);

        let results = self.base.results_mut();
        results.value = Some(value);
        results.error_estimate = None;
        results.valuation_date = None;
        results.additional_results.insert(
            "optionletsPrice".to_string(),
            shared(values) as Shared<dyn Any>,
        );
        results.additional_results.insert(
            "optionletsAtmForward".to_string(),
            shared(forwards) as Shared<dyn Any>,
        );
        // A collar overwrote each std-dev at its floor strike, so the vector no
        // longer describes one option; C++ withholds it for that case
        // (`.cpp:126-127`).
        if cap_floor_type != CapFloorType::Collar {
            results.additional_results.insert(
                "optionletsStdDev".to_string(),
                shared(std_devs) as Shared<dyn Any>,
            );
        }
        Ok(())
    }
}

#[cfg(test)]
mod instrument_oracle {
    //! `test-suite/inflationcapfloor.cpp`, the oracle for the whole year-on-year
    //! cap/floor stack: real UK RPI history and a 5 % nominal curve, fifteen
    //! market quotes bootstrapped into a `PiecewiseYoYInflationCurve<Linear>`,
    //! and caps and floors priced off it under all three distributions.
    //!
    //! ## Two fixture quirks, both reproduced deliberately
    //!
    //! `CommonVars` (`.cpp:89-265`) carries two accidents that nonetheless
    //! decide its cached values, so the port copies them rather than tidying
    //! them:
    //!
    //! 1. **The RPI history overruns by two months** (`.cpp:126-141`). The
    //!    schedule runs to 13 August 2007 and is generated *backwards*, so it
    //!    holds 33 dates - January 2005 twice, then one per month - while only
    //!    31 figures are real; `fixData` pads with `-999.0`, landing a sentinel
    //!    in July and August 2007. Neither is ever read as a rate (every fixing
    //!    date here is 2008 or later, so the ratio index forecasts off the curve
    //!    and never consults the store), but they move the index's
    //!    `lastFixingDate`, and hence the curve's base date, from 1 July to
    //!    **1 August 2007** - the origin every number below is measured from.
    //! 2. **The observation lag is zero, not two months** (`.cpp:100`, `:150`).
    //!    `CommonVars` declares a member `Period observationLag` and never
    //!    assigns it: the `Period observationLag = Period(2,Months)` in the
    //!    constructor body is a *local* that shadows it. So the bootstrap
    //!    helpers observe two months back, while the leg, the volatility surface
    //!    and the parity swap all take the default-constructed `Period()`, zero
    //!    days. Pricing the leg at two months instead misses every cached value
    //!    by about 47 (they hold to 0.02 and 0.22 at zero), so the shadowed
    //!    member is what produced them.
    //!
    //! Both are internally consistent between the cap/floor and the swap, so
    //! parity and collar consistency would pass either way; only the cached
    //! values discriminate.

    use super::*;
    use crate::cashflows::{YoYInflationCoupon, YoYInflationLeg};
    use crate::handle::RelinkableHandle;
    use crate::indexes::inflation::UkRpi;
    use crate::indexes::inflationindex::{CpiInterpolationType, InflationIndex};
    use crate::instrument::Instrument;
    use crate::instruments::{SwapType, YearOnYearInflationSwap, YoYInflationCapFloor};
    use crate::interestrate::Compounding;
    use crate::math::interpolations::linear::Linear;
    use crate::pricingengines::DiscountingSwapEngine;
    use crate::quotes::SimpleQuote;
    use crate::settings::Settings;
    use crate::shared::{SharedMut, shared_mut};
    use crate::termstructures::inflation::inflationhelpers::{
        YearOnYearInflationSwapHelper, YoYInflationHelper,
    };
    use crate::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure;
    use crate::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve;
    use crate::termstructures::volatility::ConstantYoYOptionletVolatility;
    use crate::termstructures::yields::{FlatForward, Pillar};
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendar::Calendar;
    use crate::time::calendars::unitedkingdom::{self, UnitedKingdom};
    use crate::time::date::Month::{August, January};
    use crate::time::date::{Date, Day, Month, Year};
    use crate::time::dategenerationrule::DateGeneration;
    use crate::time::daycounter::DayCounter;
    use crate::time::daycounters::actualactual::{
        ActualActual, Convention as ActualActualConvention,
    };
    use crate::time::daycounters::thirty360::{Convention, Thirty360};
    use crate::time::frequency::Frequency;
    use crate::time::schedule::MakeSchedule;
    use crate::types::Volatility;

    /// UK RPI, thirty-one real figures then the two `-999.0` sentinels
    /// (`.cpp:132-137`). See the module docs.
    const FIX_DATA: [Real; 33] = [
        189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1,
        193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6,
        203.1, 204.4, 205.4, 206.2, 207.3, -999.0, -999.0,
    ];

    /// The fifteen quoted year-on-year swap rates, in per cent (`.cpp:152-168`).
    const YY_DATA: [(Day, Month, Year, Real); 15] = [
        (13, August, 2008, 2.95),
        (13, August, 2009, 2.95),
        (13, August, 2010, 2.93),
        (15, August, 2011, 2.955),
        (13, August, 2012, 2.945),
        (13, August, 2013, 2.985),
        (13, August, 2014, 3.01),
        (13, August, 2015, 3.035),
        (13, August, 2016, 3.055),
        (13, August, 2017, 3.075),
        (13, August, 2019, 3.105),
        (15, August, 2022, 3.135),
        (13, August, 2027, 3.155),
        (13, August, 2032, 3.145),
        (13, August, 2037, 3.145),
    ];

    const NOTIONAL: Real = 1_000_000.0;

    fn uk() -> Calendar {
        UnitedKingdom::new(unitedkingdom::Market::Settlement)
    }

    fn day_counter() -> DayCounter {
        Thirty360::with_convention(Convention::BondBasis)
    }

    /// The lag the leg, the surface and the parity swap observe: zero, from the
    /// shadowed `CommonVars` member. See the module docs.
    fn observation_lag() -> Period {
        Period::new(0, TimeUnit::Days)
    }

    struct Fixture {
        settings: Shared<Settings<Date>>,
        index: Shared<YoYInflationIndex>,
        nominal: Handle<dyn YieldTermStructure>,
        evaluation_date: Date,
        base_date: Date,
        _curve: Shared<PiecewiseYoYInflationCurve<Linear>>,
        _handle: RelinkableHandle<dyn YoYInflationTermStructure>,
    }

    /// `CommonVars` (`.cpp:109-187`).
    fn a_bootstrapped_market() -> Fixture {
        let settings = shared(Settings::<Date>::new());
        let evaluation_date = uk().adjust(
            Date::new(13, August, 2007),
            BusinessDayConvention::Following,
        );
        settings.set_evaluation_date(evaluation_date);

        let rpi_schedule = MakeSchedule::new()
            .from(Date::new(1, January, 2005))
            .to(Date::new(13, August, 2007))
            .with_tenor(Period::new(1, TimeUnit::Months))
            .with_calendar(uk())
            .with_convention(BusinessDayConvention::ModifiedFollowing)
            .build();
        let rpi = shared(UkRpi::new(Shared::clone(&settings)));
        for (date, &figure) in rpi_schedule.dates().iter().zip(FIX_DATA.iter()) {
            rpi.add_fixing(*date, figure).expect("a published figure");
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

        // Only the helpers see two months (`.cpp:150`, `:174`).
        let helper_lag = Period::new(2, TimeUnit::Months);
        let helpers: Vec<Shared<dyn YoYInflationHelper>> = YY_DATA
            .iter()
            .map(|&(day, month, year, rate)| {
                YearOnYearInflationSwapHelper::new(
                    Handle::new(shared(SimpleQuote::new(Some(rate / 100.0)))),
                    helper_lag,
                    Date::new(day, month, year),
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

        let base_date = rpi.last_fixing_date().expect("RPI has history");
        let curve = PiecewiseYoYInflationCurve::<Linear>::new(
            evaluation_date,
            base_date,
            YY_DATA[0].3 / 100.0,
            index.frequency(),
            day_counter(),
            helpers,
            None,
        )
        .expect("fifteen helpers");
        handle.link_to(Shared::clone(&curve) as Shared<dyn YoYInflationTermStructure>);

        Fixture {
            settings,
            index,
            nominal,
            evaluation_date,
            base_date,
            _curve: curve,
            _handle: handle,
        }
    }

    /// `makeYoYLeg` (`.cpp:190-201`), a plain annual leg.
    fn a_yoy_leg(fixture: &Fixture, length: i32) -> Vec<Shared<YoYInflationCoupon>> {
        let start = fixture.evaluation_date;
        let end = uk().advance_by_period(
            start,
            Period::new(length, TimeUnit::Years),
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
        .with_notional(NOTIONAL)
        .with_payment_day_counter(day_counter())
        .with_payment_adjustment(BusinessDayConvention::ModifiedFollowing)
        .coupons()
        .expect("a well-formed leg")
    }

    /// `makeEngine` (`.cpp:204-241`), a flat surface under one of the three
    /// distributions.
    fn an_engine(
        fixture: &Fixture,
        volatility: Volatility,
        distribution: YoYOptionletDistribution,
    ) -> SharedMut<dyn PricingEngine> {
        let surface = Handle::new(shared(ConstantYoYOptionletVolatility::new(
            volatility,
            0,
            uk(),
            BusinessDayConvention::ModifiedFollowing,
            day_counter(),
            observation_lag(),
            Frequency::Annual,
            false,
            -1.0,
            100.0,
            Shared::clone(&fixture.settings),
        )) as Shared<dyn YoYOptionletVolatilitySurface>);
        let index = Shared::clone(&fixture.index);
        let nominal = fixture.nominal.clone();
        let engine = match distribution {
            YoYOptionletDistribution::Black => {
                YoYInflationCapFloorEngine::black(index, surface, nominal)
            }
            YoYOptionletDistribution::UnitDisplaced => {
                YoYInflationCapFloorEngine::unit_displaced(index, surface, nominal)
            }
            YoYOptionletDistribution::Bachelier => {
                YoYInflationCapFloorEngine::bachelier(index, surface, nominal)
            }
        };
        shared_mut(engine) as SharedMut<dyn PricingEngine>
    }

    /// `makeYoYCapFloor` (`.cpp:244-264`).
    fn a_cap_floor(
        fixture: &Fixture,
        cap_floor_type: CapFloorType,
        coupons: Vec<Shared<YoYInflationCoupon>>,
        strike: Rate,
        volatility: Volatility,
        distribution: YoYOptionletDistribution,
    ) -> YoYInflationCapFloor {
        let mut instrument = match cap_floor_type {
            CapFloorType::Floor => {
                YoYInflationCapFloor::floor(coupons, vec![strike], Shared::clone(&fixture.settings))
            }
            _ => YoYInflationCapFloor::cap(coupons, vec![strike], Shared::clone(&fixture.settings)),
        }
        .expect("a well-formed cap/floor");
        instrument
            .base_mut()
            .set_pricing_engine(an_engine(fixture, volatility, distribution));
        instrument
    }

    const DISTRIBUTIONS: [YoYOptionletDistribution; 3] = [
        YoYOptionletDistribution::Black,
        YoYOptionletDistribution::UnitDisplaced,
        YoYOptionletDistribution::Bachelier,
    ];
    const LENGTHS: [i32; 3] = [1, 5, 10];
    const STRIKES: [Rate; 3] = [0.01, 0.03, 0.07];
    const VOLS: [Volatility; 2] = [0.001, 0.15];

    /// The `-999.0` sentinels push the curve's time origin from 1 July to
    /// 1 August 2007. Pinned here so a change names its cause rather than
    /// surfacing as a cached value drifting by a few units.
    #[test]
    fn the_sentinel_fixings_put_the_curve_base_date_in_august() {
        let fixture = a_bootstrapped_market();
        assert_eq!(fixture.base_date, Date::new(1, August, 2007));
    }

    /// `testParity` (`.cpp:388-450`): cap - floor == swap, to `1e-6` on a
    /// notional of `1e6`. Model-independent, and the reason the year-on-year
    /// instrument needs no special definition: unlike a nominal cap/floor it
    /// keeps its first optionlet, so the strip spans the swap exactly. It holds
    /// under all three distributions because it depends on none of them - the
    /// option values cancel, leaving the forward against the strike.
    #[test]
    fn a_cap_less_a_floor_reprices_the_swap() {
        let fixture = a_bootstrapped_market();
        let from = fixture
            .nominal
            .current_link()
            .expect("a linked nominal curve")
            .reference_date()
            .expect("a reference date");

        for distribution in DISTRIBUTIONS {
            for length in LENGTHS {
                for strike in STRIKES {
                    for volatility in VOLS {
                        let coupons = a_yoy_leg(&fixture, length);
                        let mut cap = a_cap_floor(
                            &fixture,
                            CapFloorType::Cap,
                            coupons.clone(),
                            strike,
                            volatility,
                            distribution,
                        );
                        let mut floor = a_cap_floor(
                            &fixture,
                            CapFloorType::Floor,
                            coupons,
                            strike,
                            volatility,
                            distribution,
                        );

                        let schedule = MakeSchedule::new()
                            .from(from)
                            .to(from + Period::new(length, TimeUnit::Years))
                            .with_tenor(Period::new(1, TimeUnit::Years))
                            .with_calendar(uk())
                            .with_convention(BusinessDayConvention::Unadjusted)
                            .backwards()
                            .build();
                        let mut swap = YearOnYearInflationSwap::new(
                            SwapType::Payer,
                            NOTIONAL,
                            schedule.clone(),
                            strike,
                            day_counter(),
                            schedule,
                            Shared::clone(&fixture.index),
                            observation_lag(),
                            CpiInterpolationType::Flat,
                            0.0,
                            day_counter(),
                            uk(),
                            BusinessDayConvention::ModifiedFollowing,
                            Shared::clone(&fixture.settings),
                        )
                        .expect("both legs are fully specified");
                        swap.base_mut()
                            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                                fixture.nominal.clone(),
                                None,
                                None,
                                None,
                                Shared::clone(&fixture.settings),
                            ))
                                as SharedMut<dyn PricingEngine>);

                        let parity = (cap.npv().expect("the curve prices it")
                            - floor.npv().expect("the curve prices it"))
                            - swap.npv().expect("the curve prices it");
                        assert!(
                            parity.abs() < 1e-6,
                            "put/call parity violated by {parity} at {distribution:?}, \
                             {length}y, strike {strike}, vol {volatility}"
                        );
                    }
                }
            }
        }
    }

    /// `testConsistency` (`.cpp:268-377`): cap - floor == collar, to `1e-6`,
    /// plus the per-optionlet recomposition.
    ///
    /// The recomposition is run *unconditionally* here. In C++ it sits inside
    /// the `BOOST_FAIL` branch of the collar check (`.cpp:298-368`), so it only
    /// ever executes once the collar identity has already failed - which is to
    /// say never, leaving `optionlet(n)` untested. Un-nesting it is what gives
    /// that accessor real coverage: summing the optionlets has to rebuild the
    /// whole instrument's NPV, which pins both the strike each optionlet
    /// carries and the coupon it is written on.
    #[test]
    fn a_collar_is_a_cap_less_a_floor_and_each_is_its_optionlets() {
        let fixture = a_bootstrapped_market();

        for distribution in DISTRIBUTIONS {
            for length in LENGTHS {
                for strike in STRIKES {
                    for volatility in VOLS {
                        let coupons = a_yoy_leg(&fixture, length);
                        let mut cap = a_cap_floor(
                            &fixture,
                            CapFloorType::Cap,
                            coupons.clone(),
                            strike,
                            volatility,
                            distribution,
                        );
                        let mut floor = a_cap_floor(
                            &fixture,
                            CapFloorType::Floor,
                            coupons.clone(),
                            strike,
                            volatility,
                            distribution,
                        );
                        let mut collar = YoYInflationCapFloor::collar(
                            coupons.clone(),
                            vec![strike],
                            vec![strike],
                            Shared::clone(&fixture.settings),
                        )
                        .expect("a well-formed collar");
                        collar.base_mut().set_pricing_engine(an_engine(
                            &fixture,
                            volatility,
                            distribution,
                        ));

                        let context = format!(
                            "{distribution:?}, {length}y, strike {strike}, vol {volatility}"
                        );
                        let cap_npv = cap.npv().expect("the curve prices it");
                        let floor_npv = floor.npv().expect("the curve prices it");
                        let collar_npv = collar.npv().expect("the curve prices it");
                        let consistency = (cap_npv - floor_npv) - collar_npv;
                        assert!(
                            consistency.abs() < 1e-6,
                            "collar inconsistent by {consistency} at {context}"
                        );

                        for instrument in [&mut cap, &mut floor, &mut collar] {
                            let mut sum = 0.0;
                            for m in 0..coupons.len() {
                                let mut optionlet =
                                    instrument.optionlet(m).expect("m is within the leg");
                                optionlet.base_mut().set_pricing_engine(an_engine(
                                    &fixture,
                                    volatility,
                                    distribution,
                                ));
                                sum += optionlet.npv().expect("the curve prices it");
                            }
                            let whole = instrument.npv().expect("the curve prices it");
                            assert!(
                                (whole - sum).abs() < 1e-6,
                                "optionlets sum to {sum}, not {whole}, at {context}"
                            );
                        }
                    }
                }
            }
        }
    }

    /// `testCachedValue` (`.cpp:452-522`): a two-year cap and floor struck at
    /// 2.95 % on 1 % volatility, against the values QuantLib caches.
    ///
    /// These are the only assertions here that would notice the fixture being
    /// subtly wrong: parity and consistency are internal identities that hold
    /// whatever the curve says, while these six numbers pin the curve, the base
    /// date, the observation lag and all three distributions at once. The
    /// tolerances are QuantLib's own.
    ///
    /// The C++ comments alongside them read "N.B. notionals are 10e6"
    /// (`.cpp:474`, `:493`, `:512`) and are stale: `CommonVars` sets
    /// `nominals(1,1000000)` (`.cpp:110`), and 1e6 is what reproduces them.
    #[test]
    fn a_two_year_cap_and_floor_match_the_cached_values() {
        let fixture = a_bootstrapped_market();
        let strike = 0.0295;
        let coupons = a_yoy_leg(&fixture, 2);

        for (distribution, cached_cap, cached_floor, tolerance) in [
            (YoYOptionletDistribution::Black, 219.452, 314.641, 0.02),
            (
                YoYOptionletDistribution::UnitDisplaced,
                9114.61,
                9209.8,
                0.22,
            ),
            (YoYOptionletDistribution::Bachelier, 8852.4, 8947.59, 0.22),
        ] {
            let mut cap = a_cap_floor(
                &fixture,
                CapFloorType::Cap,
                coupons.clone(),
                strike,
                0.01,
                distribution,
            );
            let mut floor = a_cap_floor(
                &fixture,
                CapFloorType::Floor,
                coupons.clone(),
                strike,
                0.01,
                distribution,
            );

            let cap_npv = cap.npv().expect("the curve prices it");
            let floor_npv = floor.npv().expect("the curve prices it");
            assert!(
                (cap_npv - cached_cap).abs() < tolerance,
                "{distribution:?} cap is {cap_npv}, cached {cached_cap}"
            );
            assert!(
                (floor_npv - cached_floor).abs() < tolerance,
                "{distribution:?} floor is {floor_npv}, cached {cached_floor}"
            );
        }
    }
}
