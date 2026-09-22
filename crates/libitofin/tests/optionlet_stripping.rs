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

#[test]
fn original_overnight_surface_and_actual_overnight_cap_match_quantlib() {
    use libitofin::cashflows::OvernightLeg;
    use libitofin::indexes::{Index, ibor::Sofr};
    use libitofin::instruments::CapFloor;
    use libitofin::time::businessdayconvention::BusinessDayConvention::ModifiedFollowing;
    use libitofin::time::calendars::unitedstates::{Market, UnitedStates};
    use libitofin::time::daycounters::actual360::Actual360;
    use libitofin::time::schedule::MakeSchedule;
    use libitofin::time::timeunit::TimeUnit::Years;
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(15, Month::April, 2025));
    let input = rows(include_str!(
        "fixtures/optionlet_stripping/overnight_curve.txt"
    ));
    let dates = input
        .iter()
        .map(|v| Date::new(v[2] as i32, Month::from_ordinal(v[1] as i32), v[0] as i32))
        .collect();
    let rates = input.iter().map(|v| v[3]).collect();
    let curve = Handle::new(shared(
        ZeroCurve::new(dates, rates, Actual365Fixed::new(), Linear).unwrap(),
    ) as Shared<dyn YieldTermStructure>);
    let index = shared(Sofr::new(curve.clone(), settings.clone()));
    index
        .add_fixing(Date::new(15, Month::April, 2025), 0.0304)
        .unwrap();
    let values = rows(include_str!(
        "fixtures/optionlet_stripping/overnight_vols.txt"
    ));
    let mut vols = Matrix::filled(10, 3, 0.0);
    for i in 0..10 {
        for j in 0..3 {
            vols[(i, j)] = values[i][j];
        }
    }
    let calendar = UnitedStates::new(Market::FederalReserve);
    let surface = shared(
        CapFloorTermVolSurface::moving_from_matrix(
            2,
            calendar.clone(),
            ModifiedFollowing,
            (1..=10).map(|n| Period::new(n, Years)).collect(),
            vec![0.03, 0.035, 0.04],
            &vols,
            Actual360::new(),
            settings.clone(),
        )
        .unwrap(),
    );
    let stripper = shared(
        OptionletStripper1::new_overnight(
            surface,
            index.clone(),
            curve.clone(),
            1e-6,
            100,
            VolatilityType::Normal,
            0.0,
            Some(Period::new(3, Months)),
            OptionletStripperOptions {
                dont_throw: true,
                ..Default::default()
            },
        )
        .unwrap(),
    );
    let adapter = shared(StrippedOptionletAdapter::new(stripper, settings.clone()).unwrap());
    for (time, expected) in [
        (1.0, 2.02351996017615),
        (3.0, 0.055_689_881_276_521_85),
        (5.0, 0.07966341028613029),
    ] {
        let actual = adapter.volatility(time, 0.04, false).unwrap();
        assert!(
            (actual - expected).abs() < 1e-9,
            "time={time}: {actual} vs {expected}"
        );
    }
    let schedule = MakeSchedule::new()
        .from(Date::new(17, Month::April, 2025))
        .to(Date::new(17, Month::April, 2030))
        .with_tenor(Period::new(3, Months))
        .with_calendar(calendar)
        .with_convention(ModifiedFollowing)
        .forwards()
        .build();
    let coupons = OvernightLeg::new(schedule, index)
        .with_notional(1_000_000.0)
        .with_payment_adjustment(ModifiedFollowing)
        .with_payment_lag(2)
        .coupons()
        .unwrap();
    let mut cap =
        CapFloor::from_overnight(CapFloorType::Cap, coupons, vec![0.04], vec![], settings).unwrap();
    cap.base_mut().set_pricing_engine(shared_mut(
        BachelierCapFloorEngine::new(
            curve,
            Handle::new(adapter as Shared<dyn OptionletVolatilityStructure>),
        )
        .unwrap(),
    ));
    let price = cap.npv().unwrap();
    assert!(
        (price - 1_216_118.497_735_086).abs() < 2.5e-8,
        "overnight cap {price}"
    );
}

#[test]
fn original_flat_and_nonflat_stripper2_smile_nodes_remain_unchanged() {
    use libitofin::time::timeunit::TimeUnit::Years;
    for flat_surface in [false, true] {
        let settings = settings(!flat_surface);
        let (nonflat, index, _, data) = market(false, settings.clone());
        let periods = if flat_surface {
            (1..=10).map(|n| Period::new(n, Years)).collect()
        } else {
            tenors()
        };
        let strikes = if flat_surface {
            (1..=10).map(|n| n as f64 / 100.0).collect()
        } else {
            data[0].clone()
        };
        let surface = if flat_surface {
            shared(
                CapFloorTermVolSurface::moving_from_matrix(
                    0,
                    Target::new(),
                    Following,
                    periods.clone(),
                    strikes.clone(),
                    &Matrix::filled(10, 10, 0.18),
                    Actual365Fixed::new(),
                    settings.clone(),
                )
                .unwrap(),
            )
        } else {
            nonflat
        };
        let quotes = if flat_surface {
            vec![0.18; 10]
        } else {
            vec![
                0.090304, 0.12180, 0.13077, 0.14832, 0.15570, 0.15816, 0.15932, 0.16035, 0.15951,
                0.15855, 0.15754, 0.15459, 0.15163, 0.14575, 0.14175, 0.13889,
            ]
        };
        let atm = shared(
            CapFloorTermVolCurve::moving(
                0,
                Target::new(),
                Following,
                periods.clone(),
                quotes
                    .into_iter()
                    .map(|v| make_quote_handle(v).handle())
                    .collect(),
                Actual365Fixed::new(),
                settings.clone(),
            )
            .unwrap(),
        );
        let first = shared(
            OptionletStripper1::new(
                surface,
                index,
                Handle::empty(),
                1e-6,
                100,
                VolatilityType::ShiftedLognormal,
                0.0,
                None,
            )
            .unwrap(),
        );
        let first_adapter = StrippedOptionletAdapter::new(first.clone(), settings.clone()).unwrap();
        let second = shared(OptionletStripper2::new(first, atm).unwrap());
        let second_adapter = StrippedOptionletAdapter::new(second, settings).unwrap();
        for &period in &periods {
            for &strike in &strikes {
                let first = first_adapter
                    .volatility_tenor(period, strike, true)
                    .unwrap();
                let second = second_adapter
                    .volatility_tenor(period, strike, true)
                    .unwrap();
                assert!((first - second).abs() < 2.5e-8);
            }
        }
    }
}

#[test]
fn atm_curve_and_stripper_report_date_errors_and_recover() {
    use libitofin::time::timeunit::TimeUnit::Years;
    let settings = settings(false);
    let quotes = vec![
        make_quote_handle(0.18).handle(),
        make_quote_handle(0.19).handle(),
    ];
    let invalid = CapFloorTermVolCurve::moving(
        0,
        Target::new(),
        Following,
        vec![Period::new(1, Years), Period::new(1000, Years)],
        quotes.clone(),
        Actual365Fixed::new(),
        settings.clone(),
    );
    assert!(invalid.is_err());
    let curve = CapFloorTermVolCurve::moving(
        0,
        Target::new(),
        Following,
        vec![Period::new(1, Years), Period::new(3, Years)],
        quotes,
        Actual365Fixed::new(),
        settings.clone(),
    )
    .unwrap();
    let before = curve.option_times().unwrap();
    let (surface, index, _, _) = market(false, settings.clone());
    for period in [
        Period::new(1, libitofin::time::timeunit::TimeUnit::Hours),
        Period::new(i32::MAX, Years),
    ] {
        assert!(
            OptionletStripper1::new(
                surface.clone(),
                index.clone(),
                Handle::empty(),
                1e-6,
                100,
                VolatilityType::ShiftedLognormal,
                0.0,
                Some(period)
            )
            .is_err()
        );
    }
    let stripper = OptionletStripper1::new_with_options(
        surface,
        index,
        Handle::empty(),
        1e-6,
        100,
        VolatilityType::ShiftedLognormal,
        0.0,
        None,
        OptionletStripperOptions {
            dont_throw: true,
            ..Default::default()
        },
    )
    .unwrap();
    let rates = stripper.atm_optionlet_rates().unwrap();
    settings.reset_evaluation_date();
    assert!(stripper.atm_optionlet_rates().is_err());
    assert!(curve.option_times().is_err());
    settings.set_evaluation_date(Date::max_date());
    assert!(curve.option_times().is_err());
    assert!(stripper.atm_optionlet_rates().is_err());
    settings.set_evaluation_date(Date::new(28, Month::October, 2013));
    assert_eq!(curve.option_times().unwrap(), before);
    assert_eq!(stripper.atm_optionlet_rates().unwrap(), rates);
}
