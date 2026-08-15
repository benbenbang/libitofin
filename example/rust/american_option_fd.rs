//! American options priced by finite differences with
//! `FdBlackScholesVanillaEngine`.
//!
//! Reproduces the first half of the `testFdValues` oracle of
//! `test-suite/americanoption.cpp` (`:375-427`): four rows of the Ju-1998
//! reference table (`:256-322`) priced on a 100 by 400 grid under the Douglas
//! scheme, which QuantLib checks to within 8e-2.
//!
//! The same rows are also priced as European options with the closed-form
//! `AnalyticEuropeanEngine`, so the early-exercise premium the free-boundary
//! rollback recovers is visible next to each value. The long-dated
//! dividend-paying calls are what make this oracle discriminate: their premium
//! runs from roughly 1.7 to 5.3, so an engine that rolled back with no exercise
//! condition and returned the European price would miss by tens of tolerances.
//!
//! The wiring is copied from the `test_fd_values` module in
//! `crates/libitofin/src/pricingengines/vanilla/fdblackscholesvanillaengine.rs`
//! and the flat `test_market` it prices on. See `monte_carlo.rs` for the same
//! problem under Longstaff-Schwartz Monte Carlo.

use libitofin::exercise::{AmericanExercise, EuropeanExercise, Exercise};
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{OneAssetOption, PlainVanillaPayoff};
use libitofin::interestrate::Compounding;
use libitofin::methods::finitedifferences::solvers::FdmSchemeDesc;
use libitofin::option::OptionType::{self, Call, Put};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::AnalyticEuropeanEngine;
use libitofin::pricingengines::vanilla::FdBlackScholesVanillaEngine;
use libitofin::processes::{BlackScholesMertonProcess, GeneralizedBlackScholesProcess};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;
use libitofin::types::{Rate, Real, Size, Time, Volatility};

/// The grid of the `pdeEngine` of `americanoption.cpp:399`.
const T_GRID: Size = 100;
const X_GRID: Size = 400;

/// `tolerance` of `americanoption.cpp:389`.
const TOLERANCE: Real = 8.0e-2;

fn today() -> Date {
    Date::new(15, Month::June, 2026)
}

/// `timeToDays` from `test-suite/utilities.hpp`: the year fraction rounded to
/// whole days on the 360-day year the market's Actual/360 curves count on.
fn time_to_days(t: Time) -> i32 {
    (t * 360.0).round() as i32
}

/// One row of the Ju table (`americanoption.cpp:256-322`).
struct JuValue {
    option_type: OptionType,
    strike: Real,
    spot: Real,
    q: Rate,
    r: Rate,
    t: Time,
    vol: Volatility,
    expected: Real,
}

/// The flat market of `test-suite/europeanoption.cpp`: quote-backed flat curves
/// on Actual/360, so a single process can be re-pointed at each Ju row.
struct Market {
    settings: Shared<Settings<Date>>,
    spot: Shared<SimpleQuote>,
    q_rate: Shared<SimpleQuote>,
    r_rate: Shared<SimpleQuote>,
    vol: Shared<SimpleQuote>,
    process: Shared<GeneralizedBlackScholesProcess>,
}

impl Market {
    /// Moving a quote notifies the process, which notifies the engine, which
    /// invalidates the instrument's cached price (D1).
    fn set(&self, spot: Real, q: Rate, r: Rate, vol: Volatility) {
        self.spot.set_value(spot);
        self.q_rate.set_value(q);
        self.r_rate.set_value(r);
        self.vol.set_value(vol);
    }
}

fn quote_handle(quote: &Shared<SimpleQuote>) -> Handle<dyn Quote> {
    Handle::new(Shared::clone(quote) as Shared<dyn Quote>)
}

fn flat_rate(reference: Date, quote: &Shared<SimpleQuote>) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::new(
        reference,
        quote_handle(quote),
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>
}

fn flat_vol(reference: Date, quote: &Shared<SimpleQuote>) -> Shared<dyn BlackVolTermStructure> {
    shared(BlackConstantVol::with_quote(
        reference,
        None, // no calendar override
        quote_handle(quote),
        Actual360::new(),
    )) as Shared<dyn BlackVolTermStructure>
}

fn market() -> Market {
    // D5: the evaluation date is set explicitly on an owned Settings.
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today());
    let spot = shared(SimpleQuote::new(0.0));
    let q_rate = shared(SimpleQuote::new(0.0));
    let r_rate = shared(SimpleQuote::new(0.0));
    let vol = shared(SimpleQuote::new(0.0));
    let process = shared(BlackScholesMertonProcess::new(
        quote_handle(&spot),
        Handle::new(flat_rate(today(), &q_rate)),
        Handle::new(flat_rate(today(), &r_rate)),
        Handle::new(flat_vol(today(), &vol)),
    ));
    Market {
        settings,
        spot,
        q_rate,
        r_rate,
        vol,
        process,
    }
}

/// The Ju row priced under the free-boundary rollback.
fn american_price(market: &Market, ju: &JuValue) -> Real {
    market.set(ju.spot, ju.q, ju.r, ju.vol);

    let exercise = AmericanExercise::over(today(), today() + time_to_days(ju.t))
        .expect("the window opens before it closes");
    let mut option = OneAssetOption::new(
        shared(PlainVanillaPayoff::new(ju.option_type, ju.strike)),
        shared(exercise) as Shared<dyn Exercise>,
        Shared::clone(&market.settings),
    );
    let engine = shared_mut(FdBlackScholesVanillaEngine::new(
        Shared::clone(&market.process),
        T_GRID,
        X_GRID,
        0, // damping steps
        FdmSchemeDesc::douglas(),
    ));
    option
        .base_mut()
        .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
    option.npv().expect("the grid prices it")
}

/// The same row without the early-exercise right, in closed form.
fn european_price(market: &Market, ju: &JuValue) -> Real {
    market.set(ju.spot, ju.q, ju.r, ju.vol);

    let mut option = OneAssetOption::new(
        shared(PlainVanillaPayoff::new(ju.option_type, ju.strike)),
        shared(EuropeanExercise::new(today() + time_to_days(ju.t))) as Shared<dyn Exercise>,
        Shared::clone(&market.settings),
    );
    let engine = shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&market.process)));
    option
        .base_mut()
        .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
    option.npv().expect("the closed form prices it")
}

fn main() {
    let market = market();
    let rows = [
        JuValue {
            option_type: Put,
            strike: 40.0,
            spot: 40.0,
            q: 0.0,
            r: 0.0488,
            t: 0.3333,
            vol: 0.2,
            expected: 1.576,
        },
        JuValue {
            option_type: Put,
            strike: 45.0,
            spot: 40.0,
            q: 0.0,
            r: 0.0488,
            t: 0.5833,
            vol: 0.2,
            expected: 5.260,
        },
        JuValue {
            option_type: Call,
            strike: 100.0,
            spot: 100.0,
            q: 0.07,
            r: 0.03,
            t: 3.0,
            vol: 0.2,
            expected: 9.065,
        },
        JuValue {
            option_type: Call,
            strike: 100.0,
            spot: 120.0,
            q: 0.07,
            r: 0.03,
            t: 3.0,
            vol: 0.2,
            expected: 21.398,
        },
    ];

    println!(
        "American options on a {T_GRID}x{X_GRID} Douglas grid, against the Ju-1998 table \
         (tolerance {TOLERANCE})"
    );
    println!(
        "  type   K      S      q     r       T       finite diff       Ju   European  premium"
    );
    for ju in &rows {
        let american = american_price(&market, ju);
        let european = european_price(&market, ju);
        println!(
            "  {:<5} {:>6} {:>6} {:>5} {:>7} {:>7} {american:>11.6} {:>8} {european:>10.5} \
             {:>8.5}",
            format!("{:?}", ju.option_type),
            ju.strike,
            ju.spot,
            ju.q,
            ju.r,
            ju.t,
            ju.expected,
            american - european
        );
        assert!((american - ju.expected).abs() <= TOLERANCE);
    }
}
