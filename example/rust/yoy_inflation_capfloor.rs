//! Year-on-year inflation caps and floors priced off a bootstrapped YoY curve.
//!
//! Part A reproduces the six cached values of `test-suite/inflationcapfloor.cpp`
//! `testCachedValue` (`:452-522`): a two-year cap and floor struck at 2.95 % on
//! 1 % volatility, valued under all three optionlet distributions (Black,
//! unit-displaced Black, Bachelier).
//!
//! Part B is `testParity` (`:388-450`): cap - floor reprices the plain
//! `YearOnYearInflationSwap` over the same leg, model-independently.
//!
//! Part C builds a cap through `MakeYoYInflationCapFloor` struck at the money
//! off the nominal curve, and shows the trim-before-fill order: read as a single
//! optionlet, the strike is the surviving coupon's rate rather than the whole
//! leg's.
//!
//! The market is the port's `instrument_oracle` fixture in
//! `crates/libitofin/src/pricingengines/inflation/inflationcapfloorengines.rs`,
//! copied step for step; the builder calls are those of the `tests` module in
//! `crates/libitofin/src/instruments/makeyoyinflationcapfloor.rs`.
//!
//! Two fixture quirks are deliberate, and are what reproduce the cached values:
//! the RPI history overruns by two `-999.0` sentinels, which move the curve's
//! base date from 1 July to 1 August 2007, and the leg observes a ZERO lag while
//! only the bootstrap helpers observe two months.

use libitofin::cashflow::{CashFlow, Leg};
use libitofin::cashflows::{
    CashFlows, YoYInflationCoupon, YoYInflationLeg, YoYOptionletDistribution,
};
use libitofin::errors::QlResult;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::Index; // brings add_fixing / last_fixing_date into scope
use libitofin::indexes::inflation::UkRpi;
use libitofin::indexes::inflationindex::{CpiInterpolationType, InflationIndex, YoYInflationIndex};
use libitofin::instrument::Instrument;
use libitofin::instruments::{
    CapFloorType, MakeYoYInflationCapFloor, SwapType, YearOnYearInflationSwap, YoYInflationCapFloor,
};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::{DiscountingSwapEngine, YoYInflationCapFloorEngine};
use libitofin::quotes::SimpleQuote;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::inflation::inflationhelpers::{
    YearOnYearInflationSwapHelper, YoYInflationHelper,
};
use libitofin::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure;
use libitofin::termstructures::inflation::piecewiseyoyinflationcurve::PiecewiseYoYInflationCurve;
use libitofin::termstructures::volatility::{
    ConstantYoYOptionletVolatility, YoYOptionletVolatilitySurface,
};
use libitofin::termstructures::yields::{FlatForward, Pillar};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::unitedkingdom::{Market, UnitedKingdom};
use libitofin::time::date::Month::{August, January};
use libitofin::time::date::{Date, Day, Month, Year};
use libitofin::time::dategenerationrule::DateGeneration;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actualactual::{
    ActualActual, Convention as ActualActualConvention,
};
use libitofin::time::daycounters::thirty360::{Convention, Thirty360};
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::schedule::MakeSchedule;
use libitofin::time::timeunit::TimeUnit;
use libitofin::types::{Rate, Real, Volatility};

/// UK RPI, thirty-one real figures then the two `-999.0` sentinels
/// (`inflationcapfloor.cpp:132-137`). See the module docs.
const FIX_DATA: [Real; 33] = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1,
    193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6,
    203.1, 204.4, 205.4, 206.2, 207.3, -999.0, -999.0,
];

/// The fifteen quoted year-on-year swap rates, in per cent
/// (`inflationcapfloor.cpp:152-168`).
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
    UnitedKingdom::new(Market::Settlement)
}

fn day_counter() -> DayCounter {
    Thirty360::with_convention(Convention::BondBasis)
}

/// The lag the leg, the volatility surface and the parity swap observe: zero.
/// Only the bootstrap helpers observe two months.
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

/// `CommonVars` (`inflationcapfloor.cpp:109-187`): UK RPI with monthly history,
/// a flat 5 % nominal curve, and a year-on-year curve bootstrapped from the
/// fifteen quotes above.
fn a_bootstrapped_market() -> QlResult<Fixture> {
    let settings = shared(Settings::<Date>::new());
    let evaluation_date = uk().adjust(
        Date::new(13, August, 2007),
        BusinessDayConvention::Following,
    );
    settings.set_evaluation_date(evaluation_date);

    // D11: fixings live on the index, not on a global IndexManager.
    let rpi_schedule = MakeSchedule::new()
        .from(Date::new(1, January, 2005))
        .to(Date::new(13, August, 2007))
        .with_tenor(Period::new(1, TimeUnit::Months))
        .with_calendar(uk())
        .with_convention(BusinessDayConvention::ModifiedFollowing)
        .build();
    let rpi = shared(UkRpi::new(Shared::clone(&settings)));
    for (date, &figure) in rpi_schedule.dates().iter().zip(FIX_DATA.iter()) {
        rpi.add_fixing(*date, figure)?;
    }

    // The YoY index forecasts through an empty relinkable handle, relinked to
    // the curve once it is bootstrapped (D2).
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

    // Only the helpers see two months (`inflationcapfloor.cpp:150`, `:174`).
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

    // Bootstrap is lazy: the first read off the curve solves it.
    let base_date = rpi.last_fixing_date()?;
    let curve = PiecewiseYoYInflationCurve::<Linear>::new(
        evaluation_date,
        base_date,
        YY_DATA[0].3 / 100.0, // base YoY rate
        index.frequency(),
        day_counter(),
        helpers,
        None, // no seasonality correction
    )?;
    handle.link_to(Shared::clone(&curve) as Shared<dyn YoYInflationTermStructure>);

    Ok(Fixture {
        settings,
        index,
        nominal,
        evaluation_date,
        base_date,
        _curve: curve,
        _handle: handle,
    })
}

/// `makeYoYLeg` (`inflationcapfloor.cpp:190-201`), a plain annual leg.
fn a_yoy_leg(fixture: &Fixture, length: i32) -> QlResult<Vec<Shared<YoYInflationCoupon>>> {
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
}

/// `makeEngine` (`inflationcapfloor.cpp:204-241`): a flat optionlet surface
/// under one of the three distributions.
fn an_engine(
    fixture: &Fixture,
    volatility: Volatility,
    distribution: YoYOptionletDistribution,
) -> SharedMut<dyn PricingEngine> {
    let surface = Handle::new(shared(ConstantYoYOptionletVolatility::new(
        volatility,
        0, // settlement days
        uk(),
        BusinessDayConvention::ModifiedFollowing,
        day_counter(),
        observation_lag(),
        Frequency::Annual,
        false, // index is not interpolated
        -1.0,  // minimum strike
        100.0, // maximum strike
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

/// `makeYoYCapFloor` (`inflationcapfloor.cpp:244-264`): the instrument with its
/// engine already attached.
fn a_cap_floor(
    fixture: &Fixture,
    cap_floor_type: CapFloorType,
    coupons: Vec<Shared<YoYInflationCoupon>>,
    strike: Rate,
    volatility: Volatility,
    distribution: YoYOptionletDistribution,
) -> QlResult<YoYInflationCapFloor> {
    let mut instrument = match cap_floor_type {
        CapFloorType::Floor => {
            YoYInflationCapFloor::floor(coupons, vec![strike], Shared::clone(&fixture.settings))
        }
        _ => YoYInflationCapFloor::cap(coupons, vec![strike], Shared::clone(&fixture.settings)),
    }?;
    instrument
        .base_mut()
        .set_pricing_engine(an_engine(fixture, volatility, distribution));
    Ok(instrument)
}

fn main() -> QlResult<()> {
    let fixture = a_bootstrapped_market()?;
    println!("evaluation date        : {}", fixture.evaluation_date);
    println!("curve base date        : {}", fixture.base_date);
    println!();

    part_a_cached_values(&fixture)?;
    println!();
    part_b_parity(&fixture)?;
    println!();
    part_c_at_the_money_builder(&fixture)?;
    Ok(())
}

// ---------------------------------------------------------------------------
// Part A: the six values QuantLib caches for a 2Y cap and floor.
// ---------------------------------------------------------------------------
fn part_a_cached_values(fixture: &Fixture) -> QlResult<()> {
    println!("== Part A: 2Y cap/floor struck at 2.95 % on 1 % vol vs the cached values ==");
    let strike = 0.0295;
    let coupons = a_yoy_leg(fixture, 2)?;

    // (distribution, cached cap, cached floor) from inflationcapfloor.cpp:474,
    // :493 and :512. QuantLib's own tolerances are 0.02 / 0.22 / 0.22.
    for (distribution, cached_cap, cached_floor) in [
        (YoYOptionletDistribution::Black, 219.452, 314.641),
        (YoYOptionletDistribution::UnitDisplaced, 9114.61, 9209.8),
        (YoYOptionletDistribution::Bachelier, 8852.4, 8947.59),
    ] {
        let mut cap = a_cap_floor(
            fixture,
            CapFloorType::Cap,
            coupons.clone(),
            strike,
            0.01,
            distribution,
        )?;
        let mut floor = a_cap_floor(
            fixture,
            CapFloorType::Floor,
            coupons.clone(),
            strike,
            0.01,
            distribution,
        )?;

        println!(
            "  {distribution:<13?} cap   = {:>10.4}  (cached {cached_cap})",
            cap.npv()?
        );
        println!(
            "  {distribution:<13?} floor = {:>10.4}  (cached {cached_floor})",
            floor.npv()?
        );
    }
    Ok(())
}

// ---------------------------------------------------------------------------
// Part B: cap - floor is the year-on-year swap, whatever the distribution.
// ---------------------------------------------------------------------------
fn part_b_parity(fixture: &Fixture) -> QlResult<()> {
    println!("== Part B: cap - floor == YoY swap (5Y, strike 3 %, vol 15 %) ==");
    let strike = 0.03;
    let volatility = 0.15;
    let length = 5;
    let coupons = a_yoy_leg(fixture, length)?;
    let from = fixture
        .nominal
        .current_link()?
        .reference_date()
        .expect("a fixed-reference curve has one");

    // The swap's own annual schedule over the same span, generated backwards.
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
        schedule.clone(), // fixed schedule
        strike,           // fixed rate
        day_counter(),    // fixed day counter
        schedule,         // yoy schedule
        Shared::clone(&fixture.index),
        observation_lag(),
        CpiInterpolationType::Flat,
        0.0,           // spread
        day_counter(), // yoy day counter
        uk(),
        BusinessDayConvention::ModifiedFollowing,
        Shared::clone(&fixture.settings),
    )?;
    swap.base_mut()
        .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
            fixture.nominal.clone(),
            None,
            None,
            None,
            Shared::clone(&fixture.settings),
        )) as SharedMut<dyn PricingEngine>);
    let swap_npv = swap.npv()?;

    for distribution in [
        YoYOptionletDistribution::Black,
        YoYOptionletDistribution::UnitDisplaced,
        YoYOptionletDistribution::Bachelier,
    ] {
        let mut cap = a_cap_floor(
            fixture,
            CapFloorType::Cap,
            coupons.clone(),
            strike,
            volatility,
            distribution,
        )?;
        let mut floor = a_cap_floor(
            fixture,
            CapFloorType::Floor,
            coupons.clone(),
            strike,
            volatility,
            distribution,
        )?;
        let parity = (cap.npv()? - floor.npv()?) - swap_npv;
        println!("  {distribution:<13?} cap - floor - swap = {parity:.3e}");
    }
    println!("  swap NPV = {swap_npv:.6}");
    Ok(())
}

// ---------------------------------------------------------------------------
// Part C: MakeYoYInflationCapFloor, at-the-money strikes, and atm_rate.
// ---------------------------------------------------------------------------
fn part_c_at_the_money_builder(fixture: &Fixture) -> QlResult<()> {
    println!("== Part C: MakeYoYInflationCapFloor at the money ==");
    let curve = fixture.nominal.current_link()?;

    let builder = || {
        MakeYoYInflationCapFloor::new(
            CapFloorType::Cap,
            Shared::clone(&fixture.index),
            5, // length in years
            uk(),
            observation_lag(),
            CpiInterpolationType::Flat,
            Shared::clone(&fixture.settings),
        )
        .with_nominal(NOTIONAL)
    };

    // A strike left unset is filled at the money off the nominal curve: the rate
    // that reprices the leg the builder just laid down.
    let whole_leg = builder().with_atm_strike(fixture.nominal.clone()).build()?;
    println!(
        "  5Y cap, atm strike       = {:.10}",
        whole_leg.cap_rates()[0]
    );
    println!(
        "  5Y cap, atm_rate()       = {:.10}",
        whole_leg.atm_rate(curve.as_ref())?
    );

    // atm_rate is CashFlows::atm_rate over the instrument's own leg, off the
    // curve's reference date.
    let leg: Leg = whole_leg
        .yoy_leg()
        .iter()
        .map(|coupon| Shared::clone(coupon) as Shared<dyn CashFlow>)
        .collect();
    let by_hand = CashFlows::atm_rate(
        &leg,
        curve.as_ref(),
        &fixture.settings,
        Some(false), // include settlement date flows
        Some(curve.reference_date().expect("a reference date")),
        None, // npv date
        None, // target npv
    )?;
    println!("  the same by hand         = {by_hand:.10}");

    // as_optionlet keeps only the last coupon, and the trim happens BEFORE the
    // at-the-money fill: the strike is that coupon's rate, not the whole leg's.
    // This market is near flat, so the two sit under ten basis points apart; on
    // a curve with real slope the gap runs to a couple of hundred.
    let optionlet = builder()
        .as_optionlet(true)
        .with_atm_strike(fixture.nominal.clone())
        .build()?;
    println!(
        "  last-coupon optionlet    = {:.10}  ({} coupon)",
        optionlet.cap_rates()[0],
        optionlet.yoy_leg().len()
    );
    Ok(())
}
