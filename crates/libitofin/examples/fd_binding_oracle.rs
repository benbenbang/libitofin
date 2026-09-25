use std::fmt;

use libitofin::exercise::{AmericanExercise, BermudanExercise, EuropeanExercise, Exercise};
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{OneAssetOption, PlainVanillaPayoff};
use libitofin::interestrate::Compounding;
use libitofin::methods::finitedifferences::solvers::FdmSchemeDesc;
use libitofin::option::OptionType::Put;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::FdBlackScholesVanillaEngine;
use libitofin::processes::{BlackScholesMertonProcess, GeneralizedBlackScholesProcess};
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;

struct Values {
    npv: f64,
    delta: f64,
    gamma: f64,
    theta: f64,
}

impl fmt::Display for Values {
    fn fmt(&self, f: &mut fmt::Formatter<'_>) -> fmt::Result {
        write!(
            f,
            "{{\"npv\":{:?},\"delta\":{:?},\"gamma\":{:?},\"theta\":{:?}}}",
            self.npv, self.delta, self.gamma, self.theta
        )
    }
}

fn price(
    settings: &Shared<Settings<Date>>,
    process: &Shared<GeneralizedBlackScholesProcess>,
    exercise: Shared<dyn Exercise>,
) -> Values {
    let mut option = OneAssetOption::new(
        shared(PlainVanillaPayoff::new(Put, 100.0)),
        exercise,
        Shared::clone(settings),
    );
    let engine = shared_mut(FdBlackScholesVanillaEngine::new(
        Shared::clone(process),
        200,
        200,
        0,
        FdmSchemeDesc::douglas(),
    ));
    option
        .base_mut()
        .set_pricing_engine(engine as SharedMut<dyn PricingEngine>);
    let values = Values {
        npv: option.npv().expect("FD NPV"),
        delta: option.delta().expect("FD delta"),
        gamma: option.gamma().expect("FD gamma"),
        theta: option.theta().expect("FD theta"),
    };
    assert!(
        [values.npv, values.delta, values.gamma, values.theta]
            .into_iter()
            .all(f64::is_finite),
        "FD oracle contains a non-finite result"
    );
    values
}

fn main() {
    let today = Date::new(15, Month::January, 2025);
    let expiry = today + Period::new(1, TimeUnit::Years);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    let flat_rate = |rate| {
        Handle::new(shared(FlatForward::with_rate(
            today,
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let process = shared(BlackScholesMertonProcess::new(
        Handle::new(shared(SimpleQuote::new(80.0)) as Shared<dyn Quote>),
        flat_rate(0.0),
        flat_rate(0.05),
        Handle::new(shared(BlackConstantVol::new(
            today,
            None,
            0.25,
            Actual365Fixed::new(),
        )) as Shared<dyn BlackVolTermStructure>),
    ));
    let european = price(
        &settings,
        &process,
        shared(EuropeanExercise::new(expiry)) as Shared<dyn Exercise>,
    );
    let american = price(
        &settings,
        &process,
        shared(AmericanExercise::over(today, expiry).expect("American exercise"))
            as Shared<dyn Exercise>,
    );
    let bermudan = price(
        &settings,
        &process,
        shared(
            BermudanExercise::new(
                [3, 6, 9, 12]
                    .map(|month| today + Period::new(month, TimeUnit::Months))
                    .into(),
                false,
            )
            .expect("Bermudan exercise"),
        ) as Shared<dyn Exercise>,
    );
    println!("{{\"european\":{european},\"american\":{american},\"bermudan\":{bermudan}}}");
}
