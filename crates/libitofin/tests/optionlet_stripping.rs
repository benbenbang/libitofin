use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::IborIndex;
use libitofin::indexes::ibor::Euribor;
use libitofin::instrument::Instrument;
use libitofin::instruments::{CapFloorType, MakeCapFloor};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::math::matrix::Matrix;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::{BachelierCapFloorEngine, BlackCapFloorEngine};
use libitofin::quotes::make_quote_handle;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::volatility::*;
use libitofin::termstructures::yields::{FlatForward, ZeroCurve};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention::Following;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit::{Days, Months};

fn settings(normal: bool) -> Shared<Settings<Date>> {
    let result = shared(Settings::new());
    result.set_evaluation_date(if normal {
        Date::new(30, Month::April, 2015)
    } else {
        Date::new(28, Month::October, 2013)
    });
    result
}
fn flat(rate: f64, settings: Shared<Settings<Date>>) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::moving_with_rate(
        0,
        Target::new(),
        rate,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
        settings,
    ))
}
fn rows(text: &str) -> Vec<Vec<f64>> {
    text.lines()
        .map(|line| line.split(',').map(|v| v.parse().unwrap()).collect())
        .collect()
}
fn tenors() -> Vec<Period> {
    [
        12, 18, 24, 36, 48, 60, 72, 84, 96, 108, 120, 144, 180, 240, 300, 360,
    ]
    .into_iter()
    .map(|n| Period::new(n, Months))
    .collect()
}
type Market = (
    Shared<CapFloorTermVolSurface>,
    Shared<IborIndex>,
    Handle<dyn YieldTermStructure>,
    Vec<Vec<f64>>,
);

fn market(normal: bool, settings: Shared<Settings<Date>>) -> Market {
    let data = rows(if normal {
        include_str!("fixtures/optionlet_stripping/normal.txt")
    } else {
        include_str!("fixtures/optionlet_stripping/lognormal.txt")
    });
    let (discount, forward) = if normal {
        let dates = rows(include_str!("fixtures/optionlet_stripping/curve_dates.txt"));
        let rates = rows(include_str!("fixtures/optionlet_stripping/curve_rates.txt"));
        let curves: Vec<_> = (0..2)
            .map(|i| {
                Handle::new(shared(
                    ZeroCurve::new(
                        dates[i]
                            .iter()
                            .map(|&v| Date::from_serial(v as i32))
                            .collect(),
                        rates[i].clone(),
                        Actual365Fixed::new(),
                        Linear,
                    )
                    .unwrap(),
                ) as Shared<dyn YieldTermStructure>)
            })
            .collect();
        (curves[0].clone(), curves[1].clone())
    } else {
        let curve = Handle::new(flat(0.04, settings.clone()));
        (curve.clone(), curve)
    };
    let mut vols = Matrix::filled(data.len() - 1, data[0].len(), 0.0);
    for i in 1..data.len() {
        for j in 0..data[0].len() {
            vols[(i - 1, j)] = data[i][j];
        }
    }
    let surface = shared(
        CapFloorTermVolSurface::moving_from_matrix(
            0,
            Target::new(),
            Following,
            tenors(),
            data[0].clone(),
            &vols,
            Actual365Fixed::new(),
            settings.clone(),
        )
        .unwrap(),
    );
    (
        surface,
        shared(Euribor::six_months(forward, settings)),
        discount,
        data,
    )
}

#[test]
fn original_nonflat_lognormal_and_normal_roundtrip_oracles() {
    for normal in [false, true] {
        let settings = settings(normal);
        let (surface, index, discount, data) = market(normal, settings.clone());
        let kind = if normal {
            VolatilityType::Normal
        } else {
            VolatilityType::ShiftedLognormal
        };
        let stripper = shared(
            OptionletStripper1::new(
                surface,
                index.clone(),
                discount.clone(),
                1e-6,
                100,
                kind,
                0.0,
                None,
            )
            .unwrap(),
        );
        let adapter = shared(StrippedOptionletAdapter::new(stripper, settings.clone()).unwrap());
        adapter.enable_extrapolation();
        let vol = Handle::new(adapter as Shared<dyn OptionletVolatilityStructure>);
        let stripped: SharedMut<dyn PricingEngine> = if normal {
            shared_mut(BachelierCapFloorEngine::new(discount.clone(), vol).unwrap())
        } else {
            shared_mut(BlackCapFloorEngine::new(discount.clone(), vol, None).unwrap())
        };
        for (i, tenor) in tenors().into_iter().enumerate() {
            for (j, &strike) in data[0].iter().enumerate() {
                let mut cap = MakeCapFloor::new(
                    CapFloorType::Cap,
                    tenor,
                    index.clone(),
                    strike,
                    Period::new(0, Days),
                    settings.clone(),
                )
                .with_pricing_engine(stripped.clone())
                .build()
                .unwrap();
                let actual = cap.npv().unwrap();
                let quote = make_quote_handle(data[i + 1][j]).handle();
                let reference: SharedMut<dyn PricingEngine> = if normal {
                    shared_mut(
                        BachelierCapFloorEngine::with_flat_vol(
                            discount.clone(),
                            quote,
                            Actual365Fixed::new(),
                            settings.clone(),
                        )
                        .unwrap(),
                    )
                } else {
                    shared_mut(
                        BlackCapFloorEngine::with_flat_vol(
                            discount.clone(),
                            quote,
                            Actual365Fixed::new(),
                            0.0,
                            settings.clone(),
                        )
                        .unwrap(),
                    )
                };
                cap.base_mut().set_pricing_engine(reference);
                let expected = cap.npv().unwrap();
                assert!(
                    (actual - expected).abs() < 2.5e-8,
                    "normal={normal} tenor={tenor} strike={strike}: {actual} vs {expected}"
                );
            }
        }
    }
}

#[test]
fn original_floating_switch_strike_updates_after_curve_relink() {
    for par in [false, true] {
        let settings = settings(false);
        settings.set_using_at_par_coupons(par);
        let (surface, _, _, _) = market(false, settings.clone());
        let forward = RelinkableHandle::new(flat(0.03, settings.clone()));
        let index = shared(Euribor::six_months(forward.handle(), settings.clone()));
        let stripper = OptionletStripper1::new(
            surface,
            index,
            Handle::empty(),
            1e-6,
            100,
            VolatilityType::ShiftedLognormal,
            0.0,
            None,
        )
        .unwrap();
        assert!(
            (stripper.switch_strike().unwrap() - if par { 0.02981223 } else { 0.02981258 }).abs()
                < 2.5e-8
        );
        forward.link_to(flat(0.05, settings));
        assert!(
            (stripper.switch_strike().unwrap() - if par { 0.0499371 } else { 0.0499381 }).abs()
                < 2.5e-8
        );
    }
}

#[test]
fn approximation_seeded_black_and_normal_inverse_prices() {
    use libitofin::option::OptionType::{Call, Put};
    use libitofin::pricingengines::blackformula::bachelier_black_formula;
    use libitofin::pricingengines::{
        bachelier_black_formula_implied_vol, black_formula, black_formula_implied_std_dev,
    };
    for kind in [Call, Put] {
        for strike in [0.01, 0.03, 0.04, 0.06, 0.10] {
            let premium = black_formula(kind, strike, 0.04, 0.35, 0.96, 0.01).unwrap();
            let result = black_formula_implied_std_dev(
                kind, strike, 0.04, premium, 0.96, 0.01, None, 1e-12, 200,
            )
            .unwrap();
            assert!((result - 0.35).abs() < 1e-11);
            let premium = bachelier_black_formula(kind, strike, -0.01, 0.025, 0.96).unwrap();
            let result =
                bachelier_black_formula_implied_vol(kind, strike, -0.01, 4.0, premium, 0.96)
                    .unwrap();
            assert!((result - 0.0125).abs() < 1e-12);
        }
    }
    assert!(
        black_formula_implied_std_dev(Call, 0.03, 0.04, -0.01, 1.0, 0.0, None, 1e-12, 200).is_err()
    );
    assert!(bachelier_black_formula_implied_vol(Call, 0.03, 0.04, 1.0, 0.001, 1.0).is_err());
}
