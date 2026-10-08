use super::*;
use crate::exercise::{EuropeanExercise, Exercise};
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::{PlainVanillaPayoff, StrikedTypePayoff, VanillaOption};
use crate::interestrate::Compounding;
use crate::models::HestonModel;
use crate::models::calibrationhelper::CalibrationHelper;
use crate::models::equity::hestonmodel::FellerConstraint;
use crate::models::model::{CalibratedModelHolder, calibrate};
use crate::option::OptionType;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::vanilla::analytichestonengine::AnalyticHestonEngine;
use crate::processes::HestonProcess;
use crate::quotes::make_quote_handle;
use crate::settings::Settings;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::{Date, Month};
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;
use std::rc::Rc;

struct PriceHelper {
    option: VanillaOption,
    calls: Rc<Cell<usize>>,
}
impl CalibrationHelper for PriceHelper {
    fn calibration_error(&mut self) -> QlResult<f64> {
        self.calls.set(self.calls.get() + 1);
        Ok(self.option.npv()? - 7.965567455405804)
    }
}
struct HestonFixture {
    model: SharedMut<HestonModel>,
    helpers: Vec<SharedMut<dyn CalibrationHelper>>,
    calls: Rc<Cell<usize>>,
}

fn heston_fixture() -> HestonFixture {
    let reference = Date::new(1, Month::January, 2026);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference);
    let flat = || {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            0.0,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let process = shared(HestonProcess::new(
        flat(),
        flat(),
        make_quote_handle(100.0).handle(),
        0.09,
        1.0,
        0.04,
        1e-7,
        0.0,
    ));
    let model = HestonModel::new(process).unwrap();
    let payoff =
        shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)) as Shared<dyn StrikedTypePayoff>;
    let exercise =
        shared(EuropeanExercise::new(Date::new(1, Month::January, 2027))) as Shared<dyn Exercise>;
    let mut option = VanillaOption::new(payoff, exercise, settings);
    option.base_mut().set_pricing_engine(shared_mut(
        AnalyticHestonEngine::new(model.clone(), 64).unwrap(),
    ) as SharedMut<dyn PricingEngine>);
    let calls = Rc::new(Cell::new(0));
    let helper = shared_mut(PriceHelper {
        option,
        calls: calls.clone(),
    }) as SharedMut<dyn CalibrationHelper>;
    HestonFixture {
        model,
        helpers: vec![helper],
        calls,
    }
}
fn heston_method(common: Common) -> HybridSimulatedAnnealing {
    HybridSimulatedAnnealing::new(
        Bounds {
            lower: vec![0.01],
            upper: vec![0.10],
        },
        HybridSimulatedAnnealingOptions::default(),
        common,
    )
    .unwrap()
}

/// At constant variance 0.04 and negligible vol-of-vol, the analytic Heston
/// call tends to the independent Black value 100*(2*Phi(0.1)-1).
#[test]
fn named_heston_price_fit_rebuilds_process_and_preserves_fixed_parameters() {
    let HestonFixture {
        model,
        helpers,
        calls,
    } = heston_fixture();
    let mut method = heston_method(Common::default());
    calibrate(
        &model,
        &helpers,
        &mut method,
        &criteria(),
        Some(Box::new(FellerConstraint)),
        vec![],
        vec![true, true, true, true, false],
    )
    .unwrap();
    let result = method.last_result().unwrap();
    let model = model.borrow();
    assert_eq!(model.theta(), 0.04);
    assert_eq!(model.kappa(), 1.0);
    assert_eq!(model.sigma(), 1e-7);
    assert_eq!(model.rho(), 0.0);
    assert!((model.v0() - 0.04).abs() < 1e-5);
    assert_eq!(model.process().v0(), model.v0());
    assert_eq!(result.x, vec![model.v0()]);
    assert!(result.fun < 1e-4);
    assert_eq!(
        model.calibrated_model().function_evaluation() as usize,
        result.nfev + 1
    );
    assert_eq!(calls.get(), result.nfev + 1);
}

#[test]
fn named_heston_budget_is_exhaustion_with_final_residual_accounted() {
    let HestonFixture {
        model,
        helpers,
        calls,
    } = heston_fixture();
    let mut method = heston_method(Common {
        maxfev: Some(1),
        ..Common::default()
    });
    calibrate(
        &model,
        &helpers,
        &mut method,
        &criteria(),
        Some(Box::new(FellerConstraint)),
        vec![],
        vec![true, true, true, true, false],
    )
    .unwrap();
    let result = method.last_result().unwrap();
    assert_eq!(
        (result.status, result.nit, result.nfev),
        (Termination::MaxEvaluations, 0, 1)
    );
    assert!(!result.success);
    assert_eq!(model.borrow().v0(), 0.09);
    assert_eq!(model.borrow().calibrated_model().function_evaluation(), 2);
    assert_eq!(calls.get(), 2);
}
