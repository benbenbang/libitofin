//! Price a European option with the AnalyticEuropeanEngine and print
//! NPV + greeks (delta, gamma, vega, theta, rho).
//!
//! Mirrors the working `mixed_day_counters` M1-slice test in
//! `crates/libitofin/src/pricingengines/vanilla/mod.rs`. Every construction
//! step is the same one the crate's own tests exercise.

use libitofin::exercise::EuropeanExercise;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument; // brings `npv`, `base_mut` into scope
use libitofin::instruments::{EuropeanOption, PlainVanillaPayoff};
use libitofin::interestrate::Compounding;
use libitofin::option::OptionType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::AnalyticEuropeanEngine;
use libitofin::processes::BlackScholesMertonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;
use libitofin::types::Real;

fn main() {
    // --- Market date. D5: the evaluation date is set explicitly on an owned
    // Settings, not read from a global clock. ---
    let today = Date::new(15, Month::June, 2026);
    let expiry = today + 146; // Date + i64 shifts by calendar days

    let spot: Real = 100.0;
    let strike: Real = 105.0;
    let q_rate: Real = 0.04; // continuous dividend yield
    let r_rate: Real = 0.06; // continuous risk-free rate
    let vol: Real = 0.20; // flat Black vol

    // Settings is shared (Rc) so the option can register against its
    // evaluation-date observable and recompute if the date changes.
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);

    let dc = Actual360::new(); // DayCounter used for all three curves

    // --- Black-Scholes-Merton process: spot quote + dividend curve +
    // risk-free curve + Black vol surface, each wrapped in a Handle so it
    // can be relinked live. The engine discounts on the risk-free curve
    // embedded here. ---
    let process = shared(BlackScholesMertonProcess::new(
        Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>),
        Handle::new(shared(FlatForward::with_rate(
            today,
            q_rate,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>),
        Handle::new(shared(FlatForward::with_rate(
            today,
            r_rate,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>),
        Handle::new(shared(BlackConstantVol::new(today, None, vol, dc.clone()))
            as Shared<dyn BlackVolTermStructure>),
    ));

    // --- Instrument: a plain-vanilla call struck at 105, European exercise. ---
    let payoff = shared(PlainVanillaPayoff::new(OptionType::Call, strike));
    let exercise = shared(EuropeanExercise::new(expiry));
    let mut option = EuropeanOption::new(payoff, exercise, Shared::clone(&settings));

    // --- Attach the analytic engine. It is a SharedMut (Rc<RefCell<..>>)
    // because pricing mutates the engine's cached results. `base_mut()` and
    // `set_pricing_engine` come from the Instrument trait / InstrumentBase. ---
    let engine = shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&process)));
    option
        .base_mut()
        .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);

    // --- Results. Each accessor triggers `calculate()` lazily and returns a
    // Result (D4: explicit errors, e.g. if no engine/exercise were set). ---
    println!("NPV   = {:.6}", option.npv().unwrap());
    println!("delta = {:.6}", option.delta().unwrap());
    println!("gamma = {:.6}", option.gamma().unwrap());
    println!("vega  = {:.6}", option.vega().unwrap());
    println!("theta = {:.6}", option.theta().unwrap());
    println!("rho   = {:.6}", option.rho().unwrap());
}
