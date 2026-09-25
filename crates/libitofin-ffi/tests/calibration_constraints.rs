use itofin_ffi::boundary::{Context, ItofinError};
use itofin_ffi::constraint_api::{ItofinCalibrationOptions, itofin_boundary_constraint_new};
use itofin_ffi::models_api::itofin_model_calibrate_with_options;
use libitofin::handle::Handle;
use libitofin::interestrate::Compounding;
use libitofin::math::optimization::constraint::{BoundaryConstraint, Constraint};
use libitofin::math::optimization::endcriteria::EndCriteria;
use libitofin::math::optimization::levenbergmarquardt::LevenbergMarquardt;
use libitofin::math::optimization::method::OptimizationMethod;
use libitofin::models::calibrationhelper::{
    BlackCalibrationHelper, CalibrationErrorType, CalibrationHelper,
};
use libitofin::models::equity::HestonModelHelper;
use libitofin::models::{HestonModel, calibrate};
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
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;

const QUOTES: [(i32, f64, f64); 9] = [
    (6, 80.0, 0.23648109919405938),
    (6, 100.0, 0.2002947592621531),
    (6, 120.0, 0.18031832589884542),
    (12, 80.0, 0.23146092911030172),
    (12, 100.0, 0.2025441016279331),
    (12, 120.0, 0.183784462198752),
    (24, 80.0, 0.22726138410502447),
    (24, 100.0, 0.20640988517573639),
    (24, 120.0, 0.19266034047177458),
];

#[derive(Clone, Copy, Debug)]
enum Case {
    Boundary,
    FixedRho,
    Weighted,
}

impl Case {
    fn weights(self) -> Vec<f64> {
        match self {
            Self::Weighted => vec![5.0, 5.0, 5.0, 1.0, 1.0, 1.0, 1.0, 1.0, 1.0],
            _ => Vec::new(),
        }
    }

    fn fixed(self) -> Vec<bool> {
        match self {
            Self::FixedRho => vec![false, false, false, true, false],
            _ => Vec::new(),
        }
    }

    fn constraint(self) -> Option<Box<dyn Constraint>> {
        match self {
            Self::Boundary => Some(Box::new(BoundaryConstraint::new(-0.75, 1.1))),
            _ => None,
        }
    }
}

struct Fixture {
    model: SharedMut<HestonModel>,
    helpers: Vec<SharedMut<HestonModelHelper>>,
    criteria: EndCriteria,
}

fn fixture() -> Fixture {
    let today = Date::new(15, Month::January, 2026);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    let flat = |rate: f64| -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            today,
            rate,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let risk_free = flat(0.03);
    let dividend = flat(0.01);
    let spot = Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>);
    let process = shared(HestonProcess::new(
        risk_free.clone(),
        dividend.clone(),
        spot,
        0.035,
        1.0,
        0.045,
        0.25,
        -0.4,
    ));
    let model = HestonModel::new(process).unwrap();
    let helpers = QUOTES
        .into_iter()
        .map(|(months, strike, volatility)| {
            let quote = Handle::new(shared(SimpleQuote::new(volatility)) as Shared<dyn Quote>);
            shared_mut(HestonModelHelper::new(
                Period::new(months, TimeUnit::Months),
                NullCalendar::new(),
                100.0,
                strike,
                quote,
                risk_free.clone(),
                dividend.clone(),
                CalibrationErrorType::PriceError,
                settings.clone(),
            ))
        })
        .collect();
    Fixture {
        model,
        helpers,
        criteria: EndCriteria::new(1000, Some(100), 1e-8, 1e-8, Some(1e-8)).unwrap(),
    }
}

fn parameters(model: &SharedMut<HestonModel>) -> [f64; 5] {
    let model = model.borrow();
    [
        model.theta(),
        model.kappa(),
        model.sigma(),
        model.rho(),
        model.v0(),
    ]
}

fn direct_core(case: Case) -> [f64; 5] {
    let fixture = fixture();
    let engine = shared_mut(AnalyticHestonEngine::new(fixture.model.clone(), 96).unwrap())
        as SharedMut<dyn PricingEngine>;
    for helper in &fixture.helpers {
        helper
            .borrow_mut()
            .base_mut()
            .set_pricing_engine(engine.clone());
    }
    let helpers: Vec<SharedMut<dyn CalibrationHelper>> = fixture
        .helpers
        .iter()
        .map(|helper| helper.clone() as SharedMut<dyn CalibrationHelper>)
        .collect();
    calibrate(
        &fixture.model,
        &helpers,
        &mut LevenbergMarquardt::default(),
        &fixture.criteria,
        case.constraint(),
        case.weights(),
        case.fixed(),
    )
    .unwrap();
    parameters(&fixture.model)
}

fn through_c_abi(case: Case) -> [f64; 5] {
    let fixture = fixture();
    let mut context = Context::new();
    let model = context.insert(fixture.model.clone()).unwrap();
    let helpers: Vec<u64> = fixture
        .helpers
        .iter()
        .map(|helper| context.insert(helper.clone()).unwrap())
        .collect();
    let method = context
        .insert(shared_mut(LevenbergMarquardt::default()) as SharedMut<dyn OptimizationMethod>)
        .unwrap();
    let criteria = context.insert(fixture.criteria).unwrap();
    let mut error = ItofinError {
        code: 0,
        message: [0; 1024],
    };
    let constraint = if matches!(case, Case::Boundary) {
        let mut id = 0;
        let status = unsafe {
            itofin_boundary_constraint_new(&mut context, -0.75, 1.1, &mut id, &mut error)
        };
        assert_eq!(status, 0);
        id
    } else {
        0
    };
    let weights = case.weights();
    let fixed: Vec<u8> = case.fixed().into_iter().map(u8::from).collect();
    let options = ItofinCalibrationOptions {
        constraint,
        weights: if weights.is_empty() {
            std::ptr::null()
        } else {
            weights.as_ptr()
        },
        weights_len: weights.len(),
        fix_parameters: if fixed.is_empty() {
            std::ptr::null()
        } else {
            fixed.as_ptr()
        },
        fix_parameters_len: fixed.len(),
    };
    let status = unsafe {
        itofin_model_calibrate_with_options(
            &mut context,
            model,
            0,
            helpers.as_ptr(),
            helpers.len(),
            method,
            criteria,
            96,
            0,
            &options,
            &mut error,
        )
    };
    assert_eq!(status, 0, "C calibration error code {}", error.code);
    parameters(&fixture.model)
}

#[test]
fn c_calibration_options_equal_direct_core_on_the_same_fixture() {
    for case in [Case::Boundary, Case::FixedRho, Case::Weighted] {
        let core = direct_core(case);
        let ffi = through_c_abi(case);
        for (got, want) in ffi.into_iter().zip(core) {
            assert!((got - want).abs() <= 1e-12, "{case:?}: {ffi:?} != {core:?}");
        }
    }
}
