//! QuantLib v1.44 `test-suite/optionletstripper.cpp` OptionletStripper2 Ibor cases.
//!
//! The OptionletStripper1 half of `testIborReferenceDateCompatibility` (`:1303-1319`)
//! is not ported: it reads `OptionletStripper1::capFloorPrices`, which the Rust
//! stripper does not expose.
use libitofin::handle::Handle;
use libitofin::indexes::IborIndex;
use libitofin::indexes::ibor::Euribor;
use libitofin::instrument::Instrument;
use libitofin::instruments::{CapFloorType, MakeCapFloor};
use libitofin::interestrate::Compounding;
use libitofin::math::matrix::Matrix;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::BlackCapFloorEngine;
use libitofin::quotes::make_quote_handle;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::volatility::*;
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention::Following;
use libitofin::time::calendars::target::Target;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit::{Days, Months, Years};

struct Vars {
    settings: Shared<Settings<Date>>,
    curve: Handle<dyn YieldTermStructure>,
    index: Shared<IborIndex>,
    tenors: Vec<Period>,
    strikes: Vec<f64>,
}

fn vars() -> Vars {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(Date::new(28, Month::October, 2013));
    let curve = Handle::new(shared(FlatForward::moving_with_rate(
        0,
        Target::new(),
        0.04,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
        settings.clone(),
    )) as Shared<dyn YieldTermStructure>);
    let index = shared(Euribor::six_months(curve.clone(), settings.clone()));
    Vars {
        settings,
        curve,
        index,
        tenors: (1..=10).map(|n| Period::new(n, Years)).collect(),
        strikes: (1..=10).map(|n| n as f64 / 100.0).collect(),
    }
}

fn surface(v: &Vars, settlement_days: u32) -> Shared<CapFloorTermVolSurface> {
    shared(
        CapFloorTermVolSurface::moving_from_matrix(
            settlement_days,
            Target::new(),
            Following,
            v.tenors.clone(),
            v.strikes.clone(),
            &Matrix::filled(v.tenors.len(), v.strikes.len(), 0.18),
            Actual365Fixed::new(),
            v.settings.clone(),
        )
        .unwrap(),
    )
}

fn stripper1(v: &Vars, surface: Shared<CapFloorTermVolSurface>) -> Shared<OptionletStripper1> {
    shared(
        OptionletStripper1::new(
            surface,
            v.index.clone(),
            Handle::empty(),
            1e-6,
            100,
            VolatilityType::ShiftedLognormal,
            0.0,
            None,
        )
        .unwrap(),
    )
}

fn atm_curve(
    v: &Vars,
    settlement_days: u32,
    tenors: Vec<Period>,
    vol: f64,
) -> Shared<CapFloorTermVolCurve> {
    let quotes = tenors
        .iter()
        .map(|_| make_quote_handle(vol).handle())
        .collect();
    shared(
        CapFloorTermVolCurve::moving(
            settlement_days,
            Target::new(),
            Following,
            tenors,
            quotes,
            Actual365Fixed::new(),
            v.settings.clone(),
        )
        .unwrap(),
    )
}

fn cap_npv(v: &Vars, tenor: Period, strike: f64, engine: SharedMut<dyn PricingEngine>) -> f64 {
    MakeCapFloor::new(
        CapFloorType::Cap,
        tenor,
        v.index.clone(),
        strike,
        Period::new(0, Days),
        v.settings.clone(),
    )
    .with_pricing_engine(engine)
    .build()
    .unwrap()
    .npv()
    .unwrap()
}

/// `testIborReferenceDateCompatibility` (`optionletstripper.cpp:1285`), Stripper2 half.
#[test]
fn ibor_reference_date_compatibility() {
    let v = vars();
    let first = stripper1(&v, surface(&v, 2));
    let atm = atm_curve(&v, 2, v.tenors.clone(), 0.18);
    let second = OptionletStripper2::new(first, atm).unwrap();
    let strikes = second.atm_cap_floor_strikes().unwrap();
    let prices = second.atm_cap_floor_prices().unwrap();
    let legacy = shared_mut(
        BlackCapFloorEngine::with_flat_vol(
            v.curve.clone(),
            make_quote_handle(0.18).handle(),
            Actual365Fixed::new(),
            0.0,
            v.settings.clone(),
        )
        .unwrap(),
    ) as SharedMut<dyn PricingEngine>;
    for (i, &tenor) in v.tenors.iter().enumerate() {
        let npv = cap_npv(&v, tenor, strikes[i], legacy.clone());
        assert!(
            (prices[i] - npv).abs() < 1e-12,
            "{tenor}: {} vs {npv}",
            prices[i]
        );
    }
}

/// `testTermVolatilityStripping2IborFrontStub` (`optionletstripper.cpp:1214-1249,1362`).
#[test]
fn ibor_front_stub_is_stripped() {
    let v = vars();
    let first = stripper1(&v, surface(&v, 0));
    let tenors = vec![Period::new(9, Months), Period::new(1, Years)];
    let atm = atm_curve(&v, 0, tenors.clone(), 0.19);
    let second = shared(OptionletStripper2::new(first, atm).unwrap());
    let prices = second.atm_cap_floor_prices().unwrap();
    assert!(second.spreads_vol().unwrap().iter().all(|s| s.is_finite()));
    let strikes = second.atm_cap_floor_strikes().unwrap();
    let adapter = StrippedOptionletAdapter::new(second, v.settings.clone()).unwrap();
    adapter.enable_extrapolation();
    let vol = Handle::new(shared(adapter) as Shared<dyn OptionletVolatilityStructure>);
    let engine = shared_mut(BlackCapFloorEngine::new(v.curve.clone(), vol, None).unwrap())
        as SharedMut<dyn PricingEngine>;
    let npv = cap_npv(&v, tenors[0], strikes[0], engine);
    assert!((npv - prices[0]).abs() < 2.5e-8, "{npv} vs {}", prices[0]);
}

/// `testOptionletStripper2RejectsEmptyIborCap` (`optionletstripper.cpp:1368`).
#[test]
fn empty_ibor_cap_is_rejected() {
    let v = vars();
    let first = stripper1(&v, surface(&v, 0));
    let tenors = vec![Period::new(6, Months), Period::new(1, Years)];
    let atm = atm_curve(&v, 0, tenors, 0.18);
    let second = OptionletStripper2::new(first, atm).unwrap();
    assert!(second.atm_cap_floor_prices().is_err());
}

/// `testOptionletStripper2ToleratesMismatchedReferenceDate` (`optionletstripper.cpp:1389`).
#[test]
fn mismatched_reference_date_is_tolerated() {
    let v = vars();
    let first = stripper1(&v, surface(&v, 0));
    let atm = atm_curve(&v, 2, v.tenors.clone(), 0.18);
    let second = OptionletStripper2::new(first, atm).unwrap();
    assert!(second.atm_cap_floor_prices().is_ok());
}

/// Each ATM cap adjusts exactly its own optionlets (`optionletstripper2.cpp:98-99,125`).
#[test]
fn atm_adjustment_stops_at_the_last_cap_optionlet() {
    let v = vars();
    let first = stripper1(&v, surface(&v, 0));
    let tenors = vec![Period::new(1, Years), Period::new(2, Years)];
    let second = OptionletStripper2::new(first, atm_curve(&v, 0, tenors, 0.19)).unwrap();
    let atm = second.atm_cap_floor_strikes().unwrap();
    let holds = |i: usize, strike: f64| second.optionlet_strikes(i).unwrap().contains(&strike);
    assert!(holds(0, atm[0]) && holds(0, atm[1]));
    assert!(!holds(1, atm[0]) && holds(1, atm[1]));
    assert!(!holds(3, atm[0]) && !holds(3, atm[1]));
}

fn oracle(case: &str) -> Vec<[f64; 3]> {
    include_str!("fixtures/optionlet_stripper2_v144/oracle.txt")
        .lines()
        .filter_map(|line| {
            let fields: Vec<&str> = line.split(' ').collect();
            (fields[0] == case).then(|| std::array::from_fn(|k| fields[k + 2].parse().unwrap()))
        })
        .collect()
}

fn oracle_stripper(
    vol_type: VolatilityType,
    displacement: f64,
    vol: impl Fn(usize, f64) -> f64,
    atm_vols: [f64; 3],
) -> OptionletStripper2 {
    let v = vars();
    let tenors: Vec<Period> = [1, 2, 3, 5, 7, 10]
        .into_iter()
        .map(|n| Period::new(n, Years))
        .collect();
    let strikes = vec![0.01, 0.02, 0.04, 0.06, 0.1];
    let mut vols = Matrix::filled(tenors.len(), strikes.len(), 0.0);
    for i in 0..tenors.len() {
        for (j, &strike) in strikes.iter().enumerate() {
            vols[(i, j)] = vol(i, strike);
        }
    }
    let surface = shared(
        CapFloorTermVolSurface::moving_from_matrix(
            0,
            Target::new(),
            Following,
            tenors,
            strikes,
            &vols,
            Actual365Fixed::new(),
            v.settings.clone(),
        )
        .unwrap(),
    );
    let first = shared(
        OptionletStripper1::new(
            surface,
            v.index.clone(),
            Handle::empty(),
            1e-10,
            100,
            vol_type,
            displacement,
            None,
        )
        .unwrap(),
    );
    let atm_tenors = [1, 3, 5].map(|n| Period::new(n, Years)).to_vec();
    let quotes = atm_vols
        .iter()
        .map(|&vol| make_quote_handle(vol).handle())
        .collect();
    let atm = CapFloorTermVolCurve::moving(
        0,
        Target::new(),
        Following,
        atm_tenors,
        quotes,
        Actual365Fixed::new(),
        v.settings.clone(),
    )
    .unwrap();
    OptionletStripper2::new(first, shared(atm)).unwrap()
}

fn assert_matches_oracle(second: &OptionletStripper2, case: &str) {
    let strikes = second.atm_cap_floor_strikes().unwrap();
    let prices = second.atm_cap_floor_prices().unwrap();
    let spreads = second.spreads_vol().unwrap();
    let expected = oracle(case);
    assert_eq!(expected.len(), 3);
    for (i, [strike, price, spread]) in expected.into_iter().enumerate() {
        assert!((strikes[i] - strike).abs() < 1e-12, "{case} {i} strike");
        assert!((prices[i] - price).abs() < 2.5e-8, "{case} {i} price");
        assert!(
            (spreads[i] - spread).abs() < 1e-8,
            "{case} {i} spread {} vs {spread}",
            spreads[i]
        );
    }
}

/// Bracketed spread solve (`optionletstripper2.cpp:176-207`): positive and negative roots.
#[test]
fn spread_solve_matches_v1_44_on_both_brackets() {
    for (case, atm) in [("lognormal", 0.20), ("lognormal_below", 0.17)] {
        let second = oracle_stripper(VolatilityType::ShiftedLognormal, 0.0, |_, _| 0.18, [atm; 3]);
        assert_matches_oracle(&second, case);
    }
}

/// A root beyond the 0.1 spread cap is an error (`optionletstripper2.cpp:199-204`).
#[test]
fn spread_beyond_the_cap_is_rejected() {
    let second = oracle_stripper(
        VolatilityType::ShiftedLognormal,
        0.0,
        |_, _| 0.18,
        [0.40; 3],
    );
    let error = second.spreads_vol().unwrap_err().to_string();
    assert!(error.contains("greater than 0.1"), "{error}");
}

/// An ATM price under the zero-optionlet-vol floor is an error (`optionletstripper2.cpp:182-195`).
#[test]
fn atm_price_below_the_attainable_minimum_is_rejected() {
    let expected = include_str!("fixtures/optionlet_stripper2_v144/oracle.txt")
        .lines()
        .find_map(|line| line.strip_prefix("steep error "))
        .unwrap();
    let second = oracle_stripper(
        VolatilityType::ShiftedLognormal,
        0.0,
        |i, _| 0.05 + 0.05 * i as f64,
        [0.01; 3],
    );
    let error = second.spreads_vol().unwrap_err().to_string();
    assert!(error.contains(expected), "{error}");
}

/// ATM and spread pricing follow the source vol type and displacement (`optionletstripper.cpp:253-283`).
#[test]
fn vol_type_and_displacement_route_the_atm_pricing() {
    let displaced = oracle_stripper(
        VolatilityType::ShiftedLognormal,
        0.01,
        |_, _| 0.18,
        [0.20; 3],
    );
    assert_matches_oracle(&displaced, "displaced");
    let normal = oracle_stripper(
        VolatilityType::Normal,
        0.0,
        |i, strike| 0.008 + 0.0002 * i as f64 + 0.002 * strike,
        [0.0085, 0.0095, 0.0100],
    );
    assert_matches_oracle(&normal, "normal");
}
