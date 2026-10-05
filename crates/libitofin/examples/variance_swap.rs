use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::VarianceSwap;
use libitofin::interestrate::Compounding;
use libitofin::position::Position;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::ReplicatingVarianceSwapEngine;
use libitofin::processes::BlackScholesMertonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn main() -> libitofin::errors::QlResult<()> {
    let today = Date::new(5, Month::October, 2026);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    let dc = Actual365Fixed::new();
    let curve = |rate| {
        Handle::new(shared(FlatForward::with_rate(
            today,
            rate,
            dc.clone(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let process = shared(BlackScholesMertonProcess::new(
        Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
        curve(0.0),
        curve(0.03),
        Handle::new(shared(BlackConstantVol::new(today, None, 0.20, dc.clone()))
            as Shared<dyn BlackVolTermStructure>),
    ));
    let calls: Vec<f64> = (100..=150).step_by(5).map(f64::from).collect();
    let puts: Vec<f64> = (50..=100).step_by(5).map(f64::from).collect();
    let engine = shared_mut(ReplicatingVarianceSwapEngine::new(
        process, 5.0, &calls, &puts,
    )?);
    let mut swap = VarianceSwap::new(Position::Long, 0.04, 50_000.0, today, today + 365, settings)?;
    swap.base_mut()
        .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
    println!("annualized variance: {:.10}", swap.variance()?);
    println!("NPV: {:.10}", swap.npv()?);
    println!("purchased option weights: {}", swap.option_weights()?.len());
    Ok(())
}
