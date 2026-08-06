//! Self-contained `libitofin` example: Monte Carlo option pricing.
//!
//! (a) Prices a European call with `MCEuropeanEngine` and compares it to the
//!     closed-form `AnalyticEuropeanEngine`, checking the MC result lands within
//!     3 standard errors of the analytic price (the crate's own convergence pin,
//!     mceuropeanengine.rs:448-471).
//! (b) Prices an American put with `MCAmericanEngine` (Longstaff-Schwartz),
//!     printing NPV, error estimate, and exercise probability. The fixture is the
//!     one from mcamericanengine.rs:640-717 (QuantLib mclongstaffschwartzengine.cpp
//!     testAmericanOption, i=j=0), so the numbers reproduce ~2.0544 +/- ~0.0178.
//!
//! Run as a binary in a crate that depends on `libitofin` (e.g. add
//! `libitofin = "0.2"` to Cargo.toml).

use libitofin::exercise::{AmericanExercise, EuropeanExercise, Exercise};
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
use libitofin::interestrate::Compounding;
use libitofin::math::randomnumbers::rngtraits::PseudoRandom;
use libitofin::option::OptionType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::{
    AnalyticEuropeanEngine, MakeMcAmericanEngine, MakeMcEuropeanEngine,
};
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::types::{Rate, Real, Volatility};

/// A flat continuously-compounded discount curve on Actual/365 Fixed.
fn flat_curve(reference: Date, rate: Rate) -> Handle<dyn YieldTermStructure> {
    Handle::new(shared(FlatForward::with_rate(
        reference,
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>)
}

/// A Black-Scholes-Merton process: spot quote + dividend, risk-free, and vol curves.
fn bs_process(
    reference: Date,
    spot: Real,
    q: Rate,
    r: Rate,
    vol: Volatility,
) -> Shared<GeneralizedBlackScholesProcess> {
    let spot = Handle::new(shared(SimpleQuote::new(spot)) as Shared<dyn Quote>);
    let vol = Handle::new(shared(BlackConstantVol::new(
        reference,
        None, // no calendar override
        vol,
        Actual365Fixed::new(),
    )) as Shared<dyn BlackVolTermStructure>);
    shared(GeneralizedBlackScholesProcess::new(
        spot,
        flat_curve(reference, q),
        flat_curve(reference, r),
        vol,
    ))
}

fn main() {
    // ==================================================================
    // (a) European: Monte Carlo vs analytic.
    // ==================================================================
    {
        let today = Date::new(15, Month::June, 2026);
        let maturity = Date::new(15, Month::June, 2027); // ~1y on Act/365F
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);

        // spot=100, q=2%, r=5%, sigma=20%; ATM call struck at 100.
        let process = bs_process(today, 100.0, 0.02, 0.05, 0.20);
        let strike = 100.0;

        // Analytic reference (closed-form Black-Scholes).
        let mut analytic_opt = VanillaOption::new(
            shared(PlainVanillaPayoff::new(OptionType::Call, strike))
                as Shared<dyn StrikedTypePayoff>,
            shared(EuropeanExercise::new(maturity)) as Shared<dyn Exercise>,
            Shared::clone(&settings),
        );
        analytic_opt
            .base_mut()
            .set_pricing_engine(
                shared_mut(AnalyticEuropeanEngine::new(Shared::clone(&process)))
                    as SharedMut<dyn PricingEngine>,
            );
        let analytic = analytic_opt.npv().unwrap();

        // Monte Carlo engine: 1 time step, 40_000 pseudo-random paths, seed 42
        // (the oracle's fixture, mceuropeanengine.rs:430-446).
        let mc_engine = MakeMcEuropeanEngine::<PseudoRandom>::new(Shared::clone(&process))
            .with_steps(1)
            .with_samples(40_000)
            .with_seed(42)
            .build()
            .unwrap();
        let mut mc_opt = VanillaOption::new(
            shared(PlainVanillaPayoff::new(OptionType::Call, strike))
                as Shared<dyn StrikedTypePayoff>,
            shared(EuropeanExercise::new(maturity)) as Shared<dyn Exercise>,
            Shared::clone(&settings),
        );
        mc_opt
            .base_mut()
            .set_pricing_engine(shared_mut(mc_engine) as SharedMut<dyn PricingEngine>);

        let mc = mc_opt.npv().unwrap();
        let se = mc_opt.error_estimate().unwrap();

        println!("=== European call (S=100, K=100, r=5%, q=2%, sigma=20%, T~1y) ===");
        println!("  analytic NPV       = {analytic:.6}");
        println!("  MC NPV             = {mc:.6}");
        println!("  MC error estimate  = {se:.6}");
        println!(
            "  |MC - analytic|    = {:.6}  (converged: {})",
            (mc - analytic).abs(),
            (mc - analytic).abs() < 3.0 * se
        );
    }

    // ==================================================================
    // (b) American put: Longstaff-Schwartz Monte Carlo.
    //     Fixture reproduces QuantLib's own MC value on this problem.
    // ==================================================================
    {
        let eval_date = Date::new(15, Month::May, 1998);
        let settlement = Date::new(17, Month::May, 1998); // curve reference date
        let maturity = Date::new(17, Month::May, 1999);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(eval_date);

        // spot=36, q=0, r=6%, sigma=20%; put struck at 36. Curves referenced to
        // the settlement date (deliberately != evaluation date, as in the oracle).
        let process = bs_process(settlement, 36.0, 0.0, 0.06, 0.20);

        let payoff =
            shared(PlainVanillaPayoff::new(OptionType::Put, 36.0)) as Shared<dyn StrikedTypePayoff>;
        let exercise =
            shared(AmericanExercise::over(settlement, maturity).unwrap()) as Shared<dyn Exercise>;

        // 75 steps, antithetic variate, absolute tolerance 0.02, seed 42,
        // regression order 3 over the Monomial basis, 2048 calibration paths.
        let engine = MakeMcAmericanEngine::<PseudoRandom>::new(Shared::clone(&process))
            .with_steps(75)
            .with_antithetic_variate(true)
            .with_absolute_tolerance(0.02)
            .with_seed(42)
            .with_polynomial_order(3)
            .build()
            .unwrap();

        let mut american = VanillaOption::new(payoff, exercise, Shared::clone(&settings));
        american
            .base_mut()
            .set_pricing_engine(shared_mut(engine) as SharedMut<dyn PricingEngine>);

        let npv = american.npv().unwrap();
        let se = american.error_estimate().unwrap();
        let exercise_prob = american.result::<Real>("exerciseProbability").unwrap();

        println!();
        println!("=== American put (S=36, K=36, r=6%, q=0, sigma=20%, T~1y) ===");
        println!("  MC NPV                = {npv:.6}   (QuantLib ref ~2.054422)");
        println!("  MC error estimate     = {se:.6}");
        println!("  exercise probability  = {exercise_prob:.6}");
    }
}
