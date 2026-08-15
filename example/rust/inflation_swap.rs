//! Build UK RPI with fixings, bootstrap a PiecewiseZeroInflationCurve, and
//! price a ZeroCouponInflationSwap off it. Distilled from the reprice oracle in
//! `crates/libitofin/src/termstructures/inflation/piecewisezeroinflationcurve.rs`
//! (`mod zero_term_structure_oracle`, the port of inflation.cpp testZeroTermStructure).
//!
//! Wiring notes:
//! - D5: `Settings` is an explicit shared handle, not a global; the eval date is set on it.
//! - D11: fixings live on the index via `add_fixing` (needs the `Index` trait in scope),
//!   NOT on a global IndexManager.
//! - The index is built on an EMPTY `RelinkableHandle` and only relinked to the curve
//!   once bootstrapped (the standalone swaps forecast through this link).
//! - The curve's `base_date` is the index's last published period
//!   (`index.last_fixing_date()`), which precedes the reference/eval date.

use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::Index; // brings add_fixing into scope
use libitofin::indexes::inflation::UkRpi;
use libitofin::indexes::inflationindex::{CpiInterpolationType, ZeroInflationIndex};
use libitofin::instrument::Instrument; // npv() and base_mut()
use libitofin::instruments::{SwapType, ZeroCouponInflationSwap};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::inflation::inflationhelpers::{
    ZeroCouponInflationSwapHelper, ZeroInflationHelper,
};
use libitofin::termstructures::inflation::inflationtermstructure::ZeroInflationTermStructure;
use libitofin::termstructures::inflation::piecewisezeroinflationcurve::PiecewiseZeroInflationCurve;
use libitofin::termstructures::yields::{FlatForward, Pillar};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendar::Calendar;
use libitofin::time::calendars::unitedkingdom::{Market, UnitedKingdom};
use libitofin::time::date::Date;
use libitofin::time::date::Month::{August, January, July};
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::thirty360::{Convention, Thirty360};
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;
use libitofin::types::Rate;

/// UK RPI, published monthly from January 2005 to July 2007 (inflation.cpp:342-348).
const FIX_DATA: [f64; 31] = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1,
    193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6,
    203.1, 204.4, 205.4, 206.2, 207.3,
];

/// The quoted zero-coupon inflation swap rates, in per cent (inflation.cpp:358-373).
fn zc_data() -> Vec<(Date, Rate)> {
    vec![
        (Date::new(13, August, 2008), 2.93),
        (Date::new(13, August, 2009), 2.95),
        (Date::new(13, August, 2010), 2.965),
        (Date::new(15, August, 2011), 2.98),
        (Date::new(13, August, 2012), 3.0),
        (Date::new(13, August, 2014), 3.06),
        (Date::new(13, August, 2017), 3.175),
        (Date::new(13, August, 2019), 3.243),
        (Date::new(15, August, 2022), 3.293),
        (Date::new(14, August, 2027), 3.338),
        (Date::new(13, August, 2032), 3.348),
        (Date::new(15, August, 2037), 3.348),
        (Date::new(13, August, 2047), 3.308),
        (Date::new(13, August, 2057), 3.228),
    ]
}

fn main() -> Result<(), Box<dyn std::error::Error>> {
    let evaluation_date = Date::new(13, August, 2007);
    let calendar: Calendar = UnitedKingdom::new(Market::Settlement);
    let day_counter: DayCounter = Thirty360::with_convention(Convention::BondBasis);
    let observation_lag = Period::new(3, TimeUnit::Months);

    // D5: explicit Settings handle with the evaluation date set on it.
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(evaluation_date);

    // Index on an empty relinkable handle; relinked to the curve once it exists.
    let hz: RelinkableHandle<dyn ZeroInflationTermStructure> = RelinkableHandle::empty();
    let index: Shared<ZeroInflationIndex> =
        shared(UkRpi::new(Shared::clone(&settings)).with_term_structure(hz.handle()));

    // D11: fixings on the index. Monthly schedule from 1 Jan 2005, first-of-month keys.
    let first_fixing_date = Date::new(1, January, 2005);
    for (i, fixing) in FIX_DATA.iter().enumerate() {
        let date = first_fixing_date + Period::new(i as i32, TimeUnit::Months);
        index.add_fixing(date, *fixing)?;
    }

    // Flat 5% continuous nominal discount curve (Actual/360).
    let nominal_ts: Handle<dyn YieldTermStructure> = Handle::new(shared(FlatForward::with_rate(
        evaluation_date,
        0.05,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
        as Shared<dyn YieldTermStructure>);

    // One zero-coupon inflation swap helper per quoted pillar.
    let helpers: Vec<Shared<dyn ZeroInflationHelper>> = zc_data()
        .iter()
        .map(|(maturity, rate)| {
            ZeroCouponInflationSwapHelper::new(
                Handle::new(shared(SimpleQuote::new(Some(rate / 100.0))) as Shared<dyn Quote>),
                observation_lag,
                *maturity,
                calendar.clone(),
                BusinessDayConvention::ModifiedFollowing,
                day_counter.clone(),
                &index, // note: &Shared<ZeroInflationIndex>
                CpiInterpolationType::Flat,
                Pillar::LastRelevantDate,
                Shared::clone(&settings),
            )
            .expect("a three-month lag covers UK RPI's availability")
                as Shared<dyn ZeroInflationHelper>
        })
        .collect();

    // Base date = the last published period; precedes the reference date.
    let base_date = index.last_fixing_date()?;
    assert_eq!(base_date, Date::new(1, July, 2007));

    // Bootstrap the curve (lazy: runs on first read). Only Linear is constructible.
    let curve = PiecewiseZeroInflationCurve::<Linear>::new(
        evaluation_date,
        base_date,
        Frequency::Monthly,
        day_counter.clone(),
        helpers,
        None, // no seasonality correction
    )?;

    // Link the index's forecast handle to the freshly bootstrapped curve.
    hz.link_to(Shared::clone(&curve) as Shared<dyn ZeroInflationTermStructure>);

    // Price a standalone 5Y swap on the real 5% nominal curve, struck at its quote.
    let maturity = Date::new(13, August, 2012);
    let mut swap = ZeroCouponInflationSwap::new(
        SwapType::Payer,  // Payer = pays inflation, receives fixed
        1_000_000.0,      // notional
        evaluation_date,  // start
        maturity,         // maturity (raw, pre-adjustment)
        calendar.clone(), // fixed calendar
        BusinessDayConvention::ModifiedFollowing,
        day_counter.clone(),
        3.0 / 100.0, // fixed rate
        Shared::clone(&index),
        observation_lag,
        CpiInterpolationType::Flat,
        None, // inflation calendar -> fixed leg's
        None, // inflation convention -> fixed leg's
        Shared::clone(&settings),
    )?;

    let engine = DiscountingSwapEngine::new(
        nominal_ts.clone(),
        None, // include_settlement_date_flows
        None, // settlement_date -> curve ref date
        None, // npv_date -> curve ref date
        Shared::clone(&settings),
    );
    swap.base_mut()
        .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);

    println!("5Y ZCIS NPV      = {:.6}", swap.npv()?); // ~0: on-market
    println!("5Y ZCIS fairRate = {:.6}%", swap.fair_rate()? * 100.0); // 3.0%

    // Off-market strike (2%) to show a real non-zero NPV.
    let mut off = ZeroCouponInflationSwap::new(
        SwapType::Payer,
        1_000_000.0,
        evaluation_date,
        maturity,
        calendar.clone(),
        BusinessDayConvention::ModifiedFollowing,
        day_counter.clone(),
        2.0 / 100.0,
        Shared::clone(&index),
        observation_lag,
        CpiInterpolationType::Flat,
        None,
        None,
        Shared::clone(&settings),
    )?;
    let engine2 = DiscountingSwapEngine::new(
        nominal_ts.clone(),
        None,
        None,
        None,
        Shared::clone(&settings),
    );
    off.base_mut()
        .set_pricing_engine(shared_mut(engine2) as SharedMut<dyn PricingEngine>);
    println!("5Y ZCIS @2% NPV  = {:.6}", off.npv()?); // -42823.6725

    Ok(())
}
