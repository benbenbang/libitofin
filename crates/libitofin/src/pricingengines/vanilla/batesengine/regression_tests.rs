use super::tests::*;
use crate::time::daycounters::thirty360::{Convention, Thirty360};

#[test]
fn null_dates_are_rejected_before_inspecting_day_counter() {
    let market = Market::new(&parameters());
    let curve = |date| {
        shared(FlatForward::with_rate(
            date,
            0.03,
            Thirty360::with_convention(Convention::European),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>
    };
    let construct = |date| {
        BatesProcess::new(
            Handle::new(curve(date)),
            Handle::new(curve(reference())),
            quote_handle(&market.spot),
            0.04,
            1.5,
            0.05,
            0.3,
            -0.6,
            0.4,
            -0.1,
            0.2,
        )
    };
    assert!(construct(Date::null()).is_err());
    let process = construct(reference()).unwrap();
    assert!(process.time(&Date::null()).is_err());
    let model = BatesModel::new(shared(process)).unwrap();
    let mut engine = BatesEngine::new(model, 144).unwrap();
    let arguments = (engine.arguments_mut() as &mut dyn std::any::Any)
        .downcast_mut::<OptionArguments>()
        .unwrap();
    arguments.payoff = Some(shared(PlainVanillaPayoff::new(OptionType::Call, 100.0)));
    arguments.exercise = Some(shared(EuropeanExercise::new(Date::null())));
    assert!(engine.calculate().is_err());
    market.risk_handle.link_to(curve(Date::null()));
    assert!(
        market
            .model
            .borrow()
            .process()
            .time(&(reference() + 360))
            .is_err()
    );
    let mut call = market.option(OptionType::Call, 100.0);
    assert!(call.npv().is_err());
}

#[test]
fn valid_low_level_parameter_write_is_read_authoritatively_by_engine() {
    let market = Market::new(&parameters());
    let mut call = market.option(OptionType::Call, 100.0);
    let original = call.npv().unwrap();
    let mut updated = parameters();
    updated[0] = 0.08;
    updated[4] = 0.09;
    market
        .model
        .borrow_mut()
        .calibrated_model_mut()
        .set_params(&updated)
        .unwrap();
    let actual = call.npv().unwrap();
    let baseline = Market::new(&updated);
    let expected = baseline.option(OptionType::Call, 100.0).npv().unwrap();
    assert!((actual - expected).abs() < 1e-12);
    assert!((actual - original).abs() > 1.0);
}
