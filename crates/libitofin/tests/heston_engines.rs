use libitofin::exercise::EuropeanExercise;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{PlainVanillaPayoff, VanillaOption};
use libitofin::interestrate::Compounding;
use libitofin::models::{HestonModel, model::CalibratedModelHolder};
use libitofin::option::OptionType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::vanilla::coshestonengine::CosHestonEngine;
use libitofin::pricingengines::vanilla::exponentialfittinghestonengine::{
    ExponentialFittingControlVariate as Cv, ExponentialFittingHestonEngine,
};
use libitofin::processes::HestonProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn market(
    reference: Date,
    rates: [f64; 2],
    spot: f64,
    params: [f64; 5],
) -> (
    SharedMut<HestonModel>,
    Shared<Settings<Date>>,
    Shared<SimpleQuote>,
) {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(reference);
    let curve = |r| {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            r,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    };
    let quote = shared(SimpleQuote::new(spot));
    let p = shared(HestonProcess::new(
        curve(rates[0]),
        curve(rates[1]),
        Handle::new(quote.clone() as Shared<dyn Quote>),
        params[0],
        params[1],
        params[2],
        params[3],
        params[4],
    ));
    (HestonModel::new(p).unwrap(), settings, quote)
}
fn option(
    settings: Shared<Settings<Date>>,
    date: Date,
    strike: f64,
    kind: OptionType,
    engine: SharedMut<dyn PricingEngine>,
) -> VanillaOption {
    let mut o = VanillaOption::new(
        shared(PlainVanillaPayoff::new(kind, strike)),
        shared(EuropeanExercise::new(date)),
        settings,
    );
    o.base_mut().set_pricing_engine(engine);
    o
}
fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}, tolerance {tolerance}"
    );
}

#[test]
fn heston_cos_cached_prices_and_live_updates() {
    let reference = Date::new(7, Month::February, 2017);
    let (model, settings, spot) =
        market(reference, [0.15, 0.07], 100.0, [0.1, 4.0, 0.22, 1.8, -0.75]);
    let engine = shared_mut(CosHestonEngine::new(model.clone(), 25.0, 600).unwrap());
    for (kind, strike, expected) in [
        (OptionType::Call, 120.0, 9.364410588426075),
        (OptionType::Call, 250.0, 0.01036797658132471),
        (OptionType::Put, 80.0, 5.319092971836708),
        (OptionType::Put, 10.0, 0.01032681906278383),
    ] {
        let mut o = option(
            settings.clone(),
            reference + 365,
            strike,
            kind,
            engine.clone(),
        );
        close(o.npv().unwrap(), expected, 1e-10);
    }
    let mut o = option(
        settings.clone(),
        reference + 365,
        120.0,
        OptionType::Call,
        engine,
    );
    let before = o.npv().unwrap();
    spot.set_value(110.0);
    assert!(o.npv().unwrap() > before);
    spot.set_value(100.0);
    close(o.npv().unwrap(), before, 1e-12);
    let params = model.borrow().calibrated_model().params();
    let mut changed = params.clone();
    changed[0] *= 1.2;
    model.borrow_mut().set_params(&changed).unwrap();
    assert!((o.npv().unwrap() - before).abs() > 0.1);
    model.borrow_mut().set_params(&params).unwrap();
    close(o.npv().unwrap(), before, 1e-12);
    spot.set_value(f64::NAN);
    assert!(o.npv().is_err());
    spot.set_value(100.0);
    close(o.npv().unwrap(), before, 1e-12);
}

#[test]
fn heston_cos_cumulants_and_exponential_options_match_quantlib() {
    let reference = Date::new(7, Month::February, 2017);
    let (model, settings, _) = market(
        reference,
        [0.15, 0.075],
        100.0,
        [0.1, 4.0, 0.25, 0.4, -0.75],
    );
    let cos = CosHestonEngine::new(model.clone(), 16.0, 200).unwrap();
    let cvs = [
        Cv::Optimal,
        Cv::AndersenPiterbarg,
        Cv::AndersenPiterbargOptCV,
        Cv::AsymptoticChF,
        Cv::AngledContour,
        Cv::AngledContourNoCV,
    ];
    let lines: Vec<_> = include_str!("data/heston_engines/oracle.csv")
        .lines()
        .collect();
    for (kind, count) in [("c,", 13), ("f,", 78), ("p,", 42)] {
        assert_eq!(
            lines.iter().filter(|line| line.starts_with(kind)).count(),
            count,
            "missing QuantLib oracle rows {kind}"
        );
    }
    for line in lines {
        let row: Vec<_> = line.split(',').collect();
        let values: Vec<f64> = row[1..].iter().map(|x| x.parse().unwrap()).collect();
        if row[0] == "c" {
            let t = values[0];
            for (n, actual) in [cos.c1(t), cos.c2(t), cos.c3(t), cos.c4(t)]
                .into_iter()
                .enumerate()
            {
                close(actual, values[n + 1], 1e-12);
            }
            close(cos.mu_t(t).unwrap(), 0.075 * t, 1e-12);
            close(cos.chf(0.0, t).re, 1.0, 1e-14);
        } else if row[0] == "f" {
            let z = cos.chf(values[1], values[0]);
            close(z.re, values[2], 1e-12);
            close(z.im, values[3], 1e-12);
        } else {
            let scaling = if values[2] < 0.0 {
                None
            } else {
                Some(values[2])
            };
            let price_model = if values[0] == 3.0 {
                market(reference, [0.15, 0.075], 100.0, [0.01, 0.5, 0.01, 2.0, 0.0]).0
            } else {
                model.clone()
            };
            let engine = shared_mut(
                ExponentialFittingHestonEngine::new(
                    price_model,
                    cvs[values[0] as usize],
                    scaling,
                    values[1],
                )
                .unwrap(),
            );
            let mut o = option(
                settings.clone(),
                reference + 365,
                120.0,
                OptionType::Call,
                engine,
            );
            close(o.npv().unwrap(), values[3], 1e-10);
        }
    }
}

#[test]
fn heston_engines_validate_parameters_and_cos_truncation() {
    let reference = Date::new(22, Month::August, 2022);
    let (model, settings, _) = market(reference, [0.0, 0.0], 100.0, [0.007, 0.8, 0.007, 0.1, -0.2]);
    for l in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(CosHestonEngine::new(model.clone(), l, 200).is_err());
    }
    assert!(CosHestonEngine::new(model.clone(), 16.0, 0).is_err());
    for s in [0.0, -1.0, f64::NAN, f64::INFINITY] {
        assert!(
            ExponentialFittingHestonEngine::new(model.clone(), Cv::Optimal, Some(s), -0.5).is_err()
        );
    }
    for alpha in [f64::NAN, f64::INFINITY] {
        assert!(
            ExponentialFittingHestonEngine::new(model.clone(), Cv::Optimal, None, alpha).is_err()
        );
    }
    assert!(
        ExponentialFittingHestonEngine::new(model.clone(), Cv::AsymptoticChF, None, -0.3).is_err()
    );
    for (kind, k, expected) in [
        (OptionType::Call, 200.0, 0.0),
        (OptionType::Put, 200.0, 100.0),
        (OptionType::Call, 1.0, 99.0),
        (OptionType::Put, 1.0, 0.0),
    ] {
        let engine = shared_mut(CosHestonEngine::new(model.clone(), 16.0, 200).unwrap());
        close(
            option(settings.clone(), reference + 1, k, kind, engine)
                .npv()
                .unwrap(),
            expected,
            1e-7,
        );
    }
}

#[test]
fn heston_exponential_fitting_extreme_moneyness_grid() {
    let reference = Date::new(13, Month::May, 2020);
    let (model, settings, _) = market(
        reference,
        [0.0507, 0.0469],
        1.0,
        [0.04, 2.5, 0.06, 0.75, -0.6],
    );
    let engine =
        shared_mut(ExponentialFittingHestonEngine::new(model, Cv::Optimal, None, -0.5).unwrap());
    let prices: [f64; 44] = [
        1.1631865252540813e-58,
        1.0642682227325847e-49,
        6.928964891104221e-16,
        8.195155262862632e-06,
        0.0006256081784763905,
        0.004172613793719457,
        0.0006256081784763905,
        8.195155262862632e-06,
        1.9230890129674141e-10,
        1.5732790182236811e-23,
        5.78305150432851e-58,
        3.560818869100988e-48,
        2.948907119421251e-23,
        1.5418175778109073e-11,
        0.0003679600118798473,
        0.004938861061060398,
        0.022715234326559378,
        0.004938861061060398,
        0.0003679600118798473,
        3.0665347440778457e-06,
        8.866652412793489e-11,
        1.5120681237170887e-20,
        4.1850671986540164e-29,
        2.466377868975599e-15,
        1.7533878491056367e-08,
        0.002847891760802183,
        0.019913309706468846,
        0.0776848755698912,
        0.019913309706468846,
        0.002847891760802183,
        0.00012462190796343504,
        2.5975531956669226e-07,
        1.1385311474312472e-12,
        4.276120738921142e-39,
        1.0838745207590666e-25,
        4.151795229444638e-11,
        0.0013415773288065313,
        0.029018582813884912,
        0.1764052130885542,
        0.029018582813884912,
        0.0013415773288065313,
        5.436740742819919e-06,
        6.514439210402305e-11,
        9.257569993947093e-21,
    ];
    let moneyness: [f64; 11] = [-20.0, -10.0, -5.0, 2.5, 1.0, 0.0, 1.0, 2.5, 5.0, 10.0, 20.0];
    for (i, days) in [1, 31, 365, 3652].into_iter().enumerate() {
        let t = days as f64 / 365.0;
        let df = (-0.0507 * t).exp();
        let forward = ((0.0507 - 0.0469) * t).exp();
        for (j, m) in moneyness.into_iter().enumerate() {
            let strike = (-m * (0.06 * t).sqrt()).exp() * forward;
            for kind in [OptionType::Call, OptionType::Put] {
                let intrinsic = match kind {
                    OptionType::Call => forward - strike,
                    OptionType::Put => strike - forward,
                };
                let expected = prices[11 * i + j] + intrinsic.max(0.0) * df;
                close(
                    option(
                        settings.clone(),
                        reference + days,
                        strike,
                        kind,
                        engine.clone(),
                    )
                    .npv()
                    .unwrap(),
                    expected,
                    1e-8,
                );
            }
        }
    }
}
