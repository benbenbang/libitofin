use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::optimization::conjugategradient::ConjugateGradient;
use libitofin::math::optimization::endcriteria::EndCriteria;
use libitofin::math::optimization::method::OptimizationMethod;
use libitofin::math::optimization::simplex::Simplex;
use libitofin::math::optimization::steepestdescent::SteepestDescent;
use libitofin::models::calibrationhelper::{
    BlackCalibrationHelper, CalibrationErrorType, CalibrationHelper,
};
use libitofin::models::equity::HestonModelHelper;
use libitofin::models::{CalibratedModelHolder, HestonModel, calibrate};
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use libitofin::processes::HestonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::calendars::nullcalendar::NullCalendar;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;

fn calibrate_heston(method: &mut dyn OptimizationMethod) -> ([f64; 5], i32) {
    let today = Date::new(15, Month::January, 2026);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    let flat = |rate| -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            today,
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let process = shared(HestonProcess::new(
        flat(0.04),
        flat(0.50),
        Handle::new(shared(SimpleQuote::new(1.0)) as Shared<dyn Quote>),
        0.01,
        0.2,
        0.02,
        0.3,
        -0.75,
    ));
    let model = HestonModel::new(process).unwrap();
    let engine = shared_mut(AnalyticHestonEngine::new(model.clone(), 96).unwrap())
        as SharedMut<dyn PricingEngine>;
    let strikes = [
        [0x3fefac53e80821cf, 0x3feec1d93a138c3f, 0x3fedde266b06edcb],
        [0x3feee6fc4e5c066b, 0x3fedad1ea01315b6, 0x3fec7fb4cb280859],
        [0x3fedfc74e7ab4083, 0x3fec86124a9aed52, 0x3feb21f1fa31573d],
        [0x3feb422c40cceb6b, 0x3fe96481fc737590, 0x3fe7a78a19df52fa],
        [0x3fe8a167781bf00b, 0x3fe6938b2fef7da4, 0x3fe4b18a04b47ab2],
        [0x3fe632e5c3c88016, 0x3fe4128a7c687e73, 0x3fe22653d92d71bf],
        [0x3fdd08fc2bc78913, 0x3fd92e6fb34bcd0d, 0x3fd5d6d3fbc52310],
    ];
    let maturities = [
        (1, TimeUnit::Months),
        (2, TimeUnit::Months),
        (3, TimeUnit::Months),
        (6, TimeUnit::Months),
        (9, TimeUnit::Months),
        (1, TimeUnit::Years),
        (2, TimeUnit::Years),
    ];
    let mut helpers: Vec<SharedMut<dyn CalibrationHelper>> = Vec::new();
    for ((length, unit), row) in maturities.into_iter().zip(strikes) {
        for bits in row {
            let helper = shared_mut(HestonModelHelper::new(
                Period::new(length, unit),
                NullCalendar::new(),
                1.0,
                f64::from_bits(bits),
                Handle::new(shared(SimpleQuote::new(0.1)) as Shared<dyn Quote>),
                flat(0.04),
                flat(0.50),
                CalibrationErrorType::RelativePriceError,
                settings.clone(),
            ));
            helper
                .borrow_mut()
                .base_mut()
                .set_pricing_engine(engine.clone());
            helpers.push(helper as SharedMut<dyn CalibrationHelper>);
        }
    }
    let criteria = EndCriteria::new(400, Some(40), 1e-8, 1e-8, Some(1e-8)).unwrap();
    calibrate(&model, &helpers, method, &criteria, None, vec![], vec![]).unwrap();
    let calibrated = model.borrow();
    let params = [
        calibrated.v0(),
        calibrated.kappa(),
        calibrated.theta(),
        calibrated.sigma(),
        calibrated.rho(),
    ];
    assert!(params.into_iter().all(f64::is_finite));
    (params, calibrated.calibrated_model().end_criteria() as i32)
}

fn main() {
    let methods: [(&str, Box<dyn OptimizationMethod>); 3] = [
        ("simplex", Box::new(Simplex::new(0.1))),
        ("conjugate_gradient", Box::new(ConjugateGradient::new())),
        ("steepest_descent", Box::new(SteepestDescent::new())),
    ];
    let rows: Vec<String> = methods
        .into_iter()
        .map(|(name, mut method)| {
            let (params, end) = calibrate_heston(&mut *method);
            format!("\"{name}\":{{\"params\":{params:?},\"end\":{end}}}")
        })
        .collect();
    println!("{{{}}}", rows.join(","));
}
