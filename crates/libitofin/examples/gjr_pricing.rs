//! Price a European GJR-GARCH option and observe a live spot update.

use libitofin::errors::QlResult;
use libitofin::exercise::EuropeanExercise;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{PlainVanillaPayoff, VanillaOption};
use libitofin::interestrate::Compounding;
use libitofin::math::randomnumbers::rngtraits::PseudoRandom;
use libitofin::models::{CalibratedModelHolder, GjrGarchModel};
use libitofin::option::OptionType;
use libitofin::pricingengines::vanilla::{AnalyticGjrGarchEngine, MakeMcEuropeanGjrGarchEngine};
use libitofin::processes::{GjrGarchDiscretization, GjrGarchParameters, GjrGarchProcess};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn main() -> QlResult<()> {
    let today = Date::new(3, Month::October, 2026);
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);
    let spot = shared(SimpleQuote::new(100.0));
    let curve = |rate| {
        Handle::new(shared(FlatForward::with_rate(
            today,
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let process = shared(GjrGarchProcess::new(
        curve(0.05),
        curve(0.02),
        Handle::new(Shared::clone(&spot) as Shared<dyn Quote>),
        GjrGarchParameters {
            v0: 0.00016,
            omega: 0.000002,
            alpha: 0.04,
            beta: 0.9,
            gamma: 0.06,
            lambda: 0.1,
            days_per_year: 252.0,
        },
        GjrGarchDiscretization::FullTruncation,
    )?);
    let model = GjrGarchModel::new(Shared::clone(&process))?;
    let mut option = VanillaOption::new(
        shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)),
        shared(EuropeanExercise::new(today + 365)),
        settings,
    );
    option
        .base_mut()
        .set_pricing_engine(shared_mut(AnalyticGjrGarchEngine::new(SharedMut::clone(
            &model,
        ))));
    println!("Analytic GJR NPV: {}", option.npv()?);
    let mc = MakeMcEuropeanGjrGarchEngine::<PseudoRandom>::new(process)
        .with_steps_per_year(252)
        .with_samples(4096)
        .with_seed(42)
        .with_antithetic_variate(true)
        .build()?;
    option.base_mut().set_pricing_engine(shared_mut(mc));
    println!("Monte Carlo GJR NPV: {}", option.npv()?);
    println!("Monte Carlo standard error: {}", option.error_estimate()?);
    println!(
        "Daily omega,alpha,beta,gamma,lambda,v0: {:?}",
        model.borrow().calibrated_model().params()
    );
    spot.set_value(105.0);
    println!("After live spot update: {}", option.npv()?);
    Ok(())
}
