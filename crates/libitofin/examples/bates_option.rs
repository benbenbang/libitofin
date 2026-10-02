//! Price a European Bates option and observe a live spot update.

use libitofin::errors::QlResult;
use libitofin::exercise::EuropeanExercise;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{PlainVanillaPayoff, VanillaOption};
use libitofin::interestrate::Compounding;
use libitofin::models::{BatesModel, CalibratedModelHolder};
use libitofin::option::OptionType;
use libitofin::pricingengines::vanilla::BatesEngine;
use libitofin::processes::BatesProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn main() -> QlResult<()> {
    let today = Date::new(2, Month::October, 2026);
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
    let process = shared(BatesProcess::new(
        curve(0.03),
        curve(0.01),
        Handle::new(spot.clone() as Shared<dyn Quote>),
        0.04,
        1.5,
        0.04,
        0.3,
        -0.5,
        0.7,
        -0.12,
        0.18,
    )?);
    let model = BatesModel::new(process)?;
    let engine = shared_mut(BatesEngine::new(model.clone(), 144)?);
    let mut option = VanillaOption::new(
        shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)),
        shared(EuropeanExercise::new(today + 365)),
        settings,
    );
    option.base_mut().set_pricing_engine(engine);
    println!("Bates NPV: {}", option.npv()?);
    spot.set_value(105.0);
    println!("After spot update: {}", option.npv()?);
    println!(
        "Calibration parameter order: {:?}",
        model.borrow().calibrated_model().params()
    );
    Ok(())
}
