use super::*;
use crate::handle::Handle;
use crate::indexes::{Index, inflation::UkRpi};
use crate::interestrate::Compounding;
use crate::math::{interpolations::linear::Linear, matrix::Matrix};
use crate::pricingengines::InterpolatingCpiCapFloorEngine;
use crate::shared::{shared, shared_mut};
use crate::termstructures::inflation::cpicapfloortermpricesurface::{
    CpiCapFloorSurfaceConfig, CpiCapFloorTermPriceSurface,
};
use crate::termstructures::inflation::inflationtermstructure::ZeroInflationTermStructure;
use crate::termstructures::inflation::interpolatedzeroinflationcurve::ZeroInflationCurve;
use crate::termstructures::{yields::FlatForward, yieldtermstructure::YieldTermStructure};
use crate::time::{
    calendars::NullCalendar, date::Month, daycounters::actual365fixed::Actual365Fixed,
    frequency::Frequency,
};

fn d(day: i32, month: Month, year: i32) -> Date {
    Date::new(day, month, year)
}
fn rows(values: &[[f64; 2]]) -> Matrix {
    let mut out = Matrix::with_size(values.len(), 2);
    for (i, row) in values.iter().enumerate() {
        out.row_mut(i).copy_from_slice(row);
    }
    out
}
fn market() -> CpiCapFloorSurfaceConfig {
    let today = d(1, Month::September, 2026);
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(today);
    let dc = Actual365Fixed::new();
    let curve = shared(
        ZeroInflationCurve::new(
            today,
            vec![d(1, Month::July, 2026), d(1, Month::July, 2040)],
            vec![0.03, 0.03],
            Frequency::Monthly,
            dc.clone(),
            Linear,
            None,
        )
        .unwrap(),
    );
    let index = shared(
        UkRpi::new(settings.clone())
            .with_term_structure(Handle::new(curve as Shared<dyn ZeroInflationTermStructure>)),
    );
    index.add_fixing(d(1, Month::June, 2026), 200.0).unwrap();
    index.add_fixing(d(1, Month::July, 2026), 200.0).unwrap();
    let discount = shared(FlatForward::with_rate(
        today,
        0.05,
        dc.clone(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    CpiCapFloorSurfaceConfig {
        nominal: 1.0,
        base_rate: 0.03,
        observation_lag: Period::new(3, TimeUnit::Months),
        calendar: NullCalendar::new(),
        business_day_convention: BusinessDayConvention::Unadjusted,
        day_counter: dc,
        index,
        interpolation: CpiInterpolationType::Flat,
        discount: Handle::new(discount as Shared<dyn YieldTermStructure>),
        cap_strikes: vec![0.04, 0.06],
        floor_strikes: vec![0.0, 0.02],
        maturities: vec![
            Period::new(1, TimeUnit::Years),
            Period::new(3, TimeUnit::Years),
        ],
        cap_prices: rows(&[[0.03, 0.06], [0.01, 0.02]]),
        floor_prices: rows(&[[0.005, 0.01], [0.02, 0.04]]),
        settings,
    }
}
fn config(
    a: &CpiCapFloorSurfaceConfig,
    kind: OptionType,
    interpolation: CpiInterpolationType,
) -> CpiCapFloorConfig {
    CpiCapFloorConfig {
        option_type: kind,
        nominal: 1e6,
        start_date: d(1, Month::September, 2026),
        base_cpi: 200.0,
        maturity: d(15, Month::September, 2028),
        fixing_calendar: a.calendar.clone(),
        fixing_convention: BusinessDayConvention::Unadjusted,
        payment_calendar: a.calendar.clone(),
        payment_convention: BusinessDayConvention::Unadjusted,
        strike: 0.05,
        index: a.index.clone(),
        observation_lag: a.observation_lag,
        interpolation,
        settings: a.settings.clone(),
    }
}
#[test]
fn cpi_price_surface_grid_interpolation_and_quoted_units() {
    let a = market();
    let settings = a.settings.clone();
    let mut c = config(&a, OptionType::Call, CpiInterpolationType::Flat);
    let surface = shared(CpiCapFloorTermPriceSurface::new(a).unwrap());
    let date = d(1, Month::September, 2027);
    assert!((surface.cap_price(date, 0.04).unwrap() - 0.03).abs() < 1e-14);
    assert!((surface.floor_price(date, 0.02).unwrap() - 0.02).abs() < 1e-14);
    assert!((surface.cap_price(date, 0.05).unwrap() - 0.02).abs() < 1e-14);
    assert!((surface.cap_price(date, 0.08).unwrap() + 0.01).abs() < 1e-14);
    let engine = shared_mut(InterpolatingCpiCapFloorEngine::new(surface.clone()));
    let mut first = CpiCapFloor::new(c.clone()).unwrap();
    first.set_pricing_engine(engine.clone());
    let value = first.npv().unwrap();
    c.nominal = 7.0;
    c.base_cpi = 123.0;
    c.start_date = d(15, Month::July, 2026);
    let mut second = CpiCapFloor::new(c.clone()).unwrap();
    second.set_pricing_engine(engine.clone());
    assert!((value - second.npv().unwrap()).abs() < 1e-14);
    let period_start = d(1, Month::September, 2028);
    assert!((value - surface.cap_price(period_start, c.strike).unwrap()).abs() < 1e-14);
    settings.set_evaluation_date(d(16, Month::September, 2028));
    assert!(first.is_expired().unwrap());
    assert_eq!(first.npv().unwrap(), 0.0);
    assert_eq!(surface.reference_date(), d(1, Month::September, 2026));
    assert!((surface.cap_price(date, 0.04).unwrap() - 0.03).abs() < 1e-14);
}
#[test]
fn cpi_capfloor_linear_lag_and_dates() {
    let a = market();
    let mut c = config(&a, OptionType::Put, CpiInterpolationType::Linear);
    c.observation_lag = Period::new(6, TimeUnit::Months);
    let surface = shared(CpiCapFloorTermPriceSurface::new(a).unwrap());
    let start = d(1, Month::June, 2028);
    let end = d(1, Month::July, 2028);
    let expected = surface.floor_price(start, c.strike).unwrap()
        + (surface.floor_price(end, c.strike).unwrap()
            - surface.floor_price(start, c.strike).unwrap())
            * 14.0
            / 30.0;
    let mut instrument = CpiCapFloor::new(c.clone()).unwrap();
    instrument.set_pricing_engine(shared_mut(InterpolatingCpiCapFloorEngine::new(
        surface.clone(),
    )));
    assert_eq!(instrument.fixing_date().unwrap(), d(15, Month::March, 2028));
    assert_eq!(
        instrument.pay_date().unwrap(),
        d(15, Month::September, 2028)
    );
    assert!((instrument.npv().unwrap() - expected).abs() < 1e-14);
    c.observation_lag = Period::new(2, TimeUnit::Months);
    let mut wrong = CpiCapFloor::new(c).unwrap();
    wrong.set_pricing_engine(shared_mut(InterpolatingCpiCapFloorEngine::new(surface)));
    assert!(wrong.npv().is_err());
}
#[test]
fn cpi_surface_snapshots_parity_inputs() {
    let a = market();
    let index = a.index.clone();
    let settings = a.settings.clone();
    let surface = CpiCapFloorTermPriceSurface::new(a).unwrap();
    let date = d(1, Month::September, 2027);
    let previous = surface.cap_price(date, 0.0).unwrap();
    let curve = shared(
        ZeroInflationCurve::new(
            d(1, Month::September, 2026),
            vec![d(1, Month::July, 2026), d(1, Month::July, 2040)],
            vec![0.09, 0.09],
            Frequency::Monthly,
            Actual365Fixed::new(),
            Linear,
            None,
        )
        .unwrap(),
    );
    let new_index = shared(
        UkRpi::new(settings.clone())
            .with_term_structure(Handle::new(curve as Shared<dyn ZeroInflationTermStructure>)),
    );
    let mut fresh = market();
    fresh.settings = settings;
    fresh.index = new_index;
    let rebuilt = CpiCapFloorTermPriceSurface::new(fresh).unwrap();
    assert!((surface.cap_price(date, 0.0).unwrap() - previous).abs() < 1e-14);
    assert!((rebuilt.cap_price(date, 0.0).unwrap() - previous).abs() > 0.001);
    assert!(
        index
            .term_structure()
            .current_link()
            .unwrap()
            .zero_rate_date(date, false)
            .is_ok()
    );
}
#[test]
fn cpi_capfloor_rejects_invalid_and_boundary_inputs_without_panics() {
    for unit in [TimeUnit::Days, TimeUnit::Weeks] {
        let mut a = market();
        a.observation_lag = Period::new(30, unit);
        assert!(CpiCapFloorTermPriceSurface::new(a).is_err());
    }
    for y in [0, -1, i32::MAX] {
        let mut a = market();
        a.maturities[1] = Period::new(y, TimeUnit::Years);
        assert!(CpiCapFloorTermPriceSurface::new(a).is_err());
    }
    for k in [f64::NAN, f64::INFINITY, -1.0, f64::MAX] {
        let mut a = market();
        a.cap_strikes[1] = k;
        assert!(CpiCapFloorTermPriceSurface::new(a).is_err());
    }
    let mut a = market();
    a.cap_prices[(0, 0)] = 0.0;
    assert!(CpiCapFloorTermPriceSurface::new(a).is_err());
    let a = market();
    let mut c = config(&a, OptionType::Call, CpiInterpolationType::Flat);
    c.observation_lag = Period::new(i32::MAX, TimeUnit::Years);
    assert!(CpiCapFloor::new(c).is_err());
    let a = market();
    let surface = CpiCapFloorTermPriceSurface::new(a).unwrap();
    assert!(surface.cap_price(Date::null(), 0.04).is_err());
    assert!(
        surface
            .cap_price(d(1, Month::September, 2027), f64::NAN)
            .is_err()
    );
}
