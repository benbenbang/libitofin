use super::*;
use crate::handle::Handle;
use crate::instruments::CapFloorType;
use crate::interestrate::Compounding;
use crate::math::array::Array;
use crate::shared::shared;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::test_support::{Flag, as_observer};
use crate::time::date::{Date, Month};
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual360::Actual360;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;
use std::any::Any;

fn reference() -> Date {
    Date::new(15, Month::January, 2026)
}
fn model() -> SharedMut<HullWhite> {
    let curve: Shared<dyn YieldTermStructure> = shared(FlatForward::with_rate(
        reference(),
        0.03,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    HullWhite::new(Handle::new(curve), 0.05, 0.01).unwrap()
}
fn args(kind: CapFloorType) -> CapFloorArguments {
    let dates = [
        Date::new(15, Month::January, 2027),
        Date::new(15, Month::July, 2027),
        Date::new(15, Month::January, 2028),
        Date::new(15, Month::July, 2028),
        Date::new(15, Month::January, 2029),
        Date::new(15, Month::July, 2029),
        Date::new(15, Month::January, 2030),
    ];
    let dc = Actual360::new();
    CapFloorArguments {
        cap_floor_type: Some(kind),
        start_dates: dates[..6].to_vec(),
        fixing_dates: dates[..6].to_vec(),
        end_dates: dates[1..].to_vec(),
        accrual_times: dates
            .windows(2)
            .map(|p| dc.year_fraction(p[0], p[1]))
            .collect(),
        cap_rates: vec![
            matches!(kind, CapFloorType::Cap | CapFloorType::Collar)
                .then_some((0.04 - 0.002) / 1.3);
            6
        ],
        floor_rates: vec![
            matches!(kind, CapFloorType::Floor | CapFloorType::Collar)
                .then_some((0.025 - 0.002) / 1.3);
            6
        ],
        forwards: vec![Some(0.03); 6],
        gearings: vec![1.3; 6],
        nominals: vec![1e6; 6],
    }
}
fn price(engine: &mut TreeCapFloorEngine, args: CapFloorArguments) -> f64 {
    *(engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<CapFloorArguments>()
        .unwrap() = args;
    engine.calculate().unwrap();
    (engine.results() as &dyn Any)
        .downcast_ref::<InstrumentResults>()
        .unwrap()
        .value
        .unwrap()
}

#[test]
fn quantlib_tree_cap_floor_and_collar_oracle() {
    for (steps, expected) in [
        (
            30,
            [20870.787228708203, 5165.620888415584, 15705.166340292615],
        ),
        (
            100,
            [20952.042402177198, 5296.2732577287425, 15655.76914444846],
        ),
    ] {
        for (kind, want) in [CapFloorType::Cap, CapFloorType::Floor, CapFloorType::Collar]
            .into_iter()
            .zip(expected)
        {
            let got = price(
                &mut TreeCapFloorEngine::new(model(), steps).unwrap(),
                args(kind),
            );
            assert!(
                (got - want).abs() < 1e-8,
                "{kind:?} {steps}: {got} != {want}"
            );
        }
    }
}

#[test]
fn fixed_grid_matches_steps_and_rejects_missing_nodes() {
    let dc: DayCounter = Actual365Fixed::new();
    let times = DiscretizedCapFloor::new(&args(CapFloorType::Cap), reference(), &dc)
        .unwrap()
        .mandatory_times();
    let grid = TimeGrid::with_mandatory_times(&times, 30).unwrap();
    let mut engine = TreeCapFloorEngine::with_time_grid(model(), grid).unwrap();
    let want = price(&mut engine, args(CapFloorType::Cap));
    let mut shifted = args(CapFloorType::Cap);
    shifted.start_dates[0] += 1;
    *(engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<CapFloorArguments>()
        .unwrap() = shifted;
    assert!(engine.calculate().is_err());
    assert!((price(&mut engine, args(CapFloorType::Cap)) - want).abs() < 1e-12);
    assert!(TreeCapFloorEngine::new(model(), 0).is_err());
    assert!(TreeCapFloorEngine::with_time_grid(model(), TimeGrid::default()).is_err());
}

#[test]
fn known_fixing_asset_pays_intrinsic_without_negative_grid_nodes() {
    let dc: DayCounter = Actual365Fixed::new();
    let start = reference() - 30;
    let end = reference() + 150;
    for (kind, want) in [
        (CapFloorType::Cap, 10000.0),
        (CapFloorType::Floor, 5000.0),
        (CapFloorType::Collar, 5000.0),
    ] {
        let arguments = CapFloorArguments {
            cap_floor_type: Some(kind),
            start_dates: vec![start],
            fixing_dates: vec![start],
            end_dates: vec![end],
            accrual_times: vec![0.5],
            cap_rates: vec![Some(0.02)],
            floor_rates: vec![Some(0.05)],
            forwards: vec![Some(0.04)],
            gearings: vec![1.0],
            nominals: vec![1e6],
        };
        let mut asset = DiscretizedCapFloor::new(&arguments, reference(), &dc).unwrap();
        let t = dc.year_fraction(reference(), end);
        let lattice: Shared<dyn Lattice> = shared(
            model()
                .borrow()
                .tree(TimeGrid::new(t, 30).unwrap())
                .unwrap(),
        );
        asset.initialize(lattice, t).unwrap();
        let pv = asset.present_value().unwrap();
        assert!(
            (pv - want * (-0.03 * t).exp()).abs() < 1e-8,
            "{kind:?}: {pv}"
        );
    }
}

#[test]
fn invalid_arguments_fail_and_model_updates_reprice() {
    let model = model();
    let mut engine = TreeCapFloorEngine::new(SharedMut::clone(&model), 30).unwrap();
    let original = price(&mut engine, args(CapFloorType::Cap));
    let mut broken = args(CapFloorType::Cap);
    broken.cap_rates[0] = None;
    *(engine.arguments_mut() as &mut dyn Any)
        .downcast_mut::<CapFloorArguments>()
        .unwrap() = broken;
    assert!(engine.calculate().is_err());
    assert!(
        DiscretizedCapFloor::new(
            &args(CapFloorType::Cap),
            Date::null(),
            &Actual365Fixed::new()
        )
        .is_err()
    );
    let mut overflowing = args(CapFloorType::Cap);
    overflowing.nominals[0] = f64::MAX;
    overflowing.gearings[0] = f64::MAX;
    assert!(DiscretizedCapFloor::new(&overflowing, reference(), &Actual365Fixed::new()).is_err());
    let flag = Flag::new();
    engine.observable().register_observer(&as_observer(&flag));
    model
        .borrow_mut()
        .set_params(&Array::from([0.05, 0.02]))
        .unwrap();
    assert!(Flag::is_up(&flag));
    assert!(price(&mut engine, args(CapFloorType::Cap)) > original);
}

#[test]
fn tree_converges_to_analytic_cap_value() {
    use crate::pricingengines::AnalyticCapFloorEngine;
    use crate::settings::Settings;
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let mut analytic = AnalyticCapFloorEngine::new(model(), settings);
    *(analytic.arguments_mut() as &mut dyn Any)
        .downcast_mut::<CapFloorArguments>()
        .unwrap() = args(CapFloorType::Cap);
    analytic.calculate().unwrap();
    let expected = analytic
        .results()
        .as_instrument_results()
        .unwrap()
        .value
        .unwrap();
    assert!((expected - 20922.731408433203).abs() < 1e-8);
    let coarse = price(
        &mut TreeCapFloorEngine::new(model(), 30).unwrap(),
        args(CapFloorType::Cap),
    );
    let fine = price(
        &mut TreeCapFloorEngine::new(model(), 600).unwrap(),
        args(CapFloorType::Cap),
    );
    assert!((fine - expected).abs() < (coarse - expected).abs());
    assert!((fine - expected).abs() / expected < 1e-3);
}

#[test]
fn normal_cap_helpers_match_quantlib_and_calibrate_tree_sigma() {
    use crate::indexes::ibor::Euribor;
    use crate::instrument::Instrument;
    use crate::math::optimization::endcriteria::EndCriteria;
    use crate::math::optimization::levenbergmarquardt::LevenbergMarquardt;
    use crate::models::calibrationhelper::{
        BlackCalibrationHelper, CalibrationErrorType, CalibrationHelper,
    };
    use crate::models::model::{calibrate, calibration_value};
    use crate::models::shortrate::calibrationhelpers::CapHelper;
    use crate::quotes::{Quote, SimpleQuote};
    use crate::settings::Settings;
    use crate::shared::shared_mut;
    use crate::termstructures::volatility::VolatilityType;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;

    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference());
    let model = model();
    let curve = model.borrow().term_structure().clone();
    let index = shared(Euribor::six_months(curve.clone(), settings));
    let engine = shared_mut(TreeCapFloorEngine::new(SharedMut::clone(&model), 30).unwrap())
        as SharedMut<dyn PricingEngine>;
    let expected = [
        (0.005205045229677168, 0.004968391846595176),
        (0.01054887990396735, 0.009882366923411948),
        (0.016771397474380823, 0.015464703896558584),
        (0.023719601355028135, 0.021512584929889333),
    ];
    let mut helpers: Vec<SharedMut<dyn CalibrationHelper>> = Vec::new();
    for (length, (market, model_value)) in (2..=5).zip(expected) {
        let quote = Handle::new(shared(SimpleQuote::new(0.01)) as Shared<dyn Quote>);
        let mut helper = CapHelper::try_new(
            Period::new(length, TimeUnit::Years),
            quote,
            Shared::clone(&index),
            Frequency::Annual,
            Actual365Fixed::new(),
            false,
            curve.clone(),
            CalibrationErrorType::RelativePriceError,
            VolatilityType::Normal,
            0.0,
        )
        .unwrap();
        helper
            .base_mut()
            .set_pricing_engine(SharedMut::clone(&engine));
        assert!((helper.market_value().unwrap() - market).abs() < 1e-12);
        assert!((helper.model_value().unwrap() - model_value).abs() < 1e-12);
        assert!((helper.cap().unwrap().borrow_mut().npv().unwrap() - model_value).abs() < 1e-12);
        helpers.push(shared_mut(helper));
    }
    let initial = model.borrow().calibrated_model().params();
    let before = calibration_value(&model, &initial, &helpers).unwrap();
    let mut method = LevenbergMarquardt::new(1e-8, 1e-8, 1e-8, false);
    let end = EndCriteria::new(10000, Some(100), 1e-6, 1e-8, Some(1e-8)).unwrap();
    calibrate(
        &model,
        &helpers,
        &mut method,
        &end,
        None,
        Vec::new(),
        vec![true, false],
    )
    .unwrap();
    let fitted = model.borrow().calibrated_model().params();
    assert!((fitted[0] - 0.05).abs() < 1e-15);
    assert!(
        (fitted[1] - 0.010705079712885954).abs() < 1e-8,
        "sigma={}",
        fitted[1]
    );
    assert!(model.borrow().calibrated_model().end_criteria().succeeded());
    let after = calibration_value(&model, &fitted, &helpers).unwrap();
    assert!(after < before);
}
