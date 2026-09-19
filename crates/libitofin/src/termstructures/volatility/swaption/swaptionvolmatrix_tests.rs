//! QuantLib 1.43 constructor, Black-engine and observable-handle oracles for #570.

use super::*;
use crate::currency::Currency;
use crate::handle::RelinkableHandle;
use crate::indexes::{SwapIndex, ibor::euribor::Euribor};
use crate::instrument::Instrument;
use crate::instruments::MakeSwaption;
use crate::interestrate::Compounding;
use crate::math::solver1d::Solver1D;
use crate::math::solvers1d::brent::Brent;
use crate::pricingengines::swaption::{BlackSwaptionEngine, CashAnnuityModel};
use crate::quotes::SimpleQuote;
use crate::shared::shared;
use crate::termstructures::{yields::FlatForward, yieldtermstructure::YieldTermStructure};
use crate::test_support::{Flag, as_observer};
use crate::time::calendars::target::Target;
use crate::time::date::Month;
use crate::time::daycounters::{
    actual365fixed::Actual365Fixed,
    thirty360::{Convention, Thirty360},
};
use crate::time::frequency::Frequency;
use crate::time::timeunit::TimeUnit;

const BDC: BusinessDayConvention = BusinessDayConvention::ModifiedFollowing;
const DATA: [[f64; 4]; 6] = [
    [0.1300, 0.1560, 0.1390, 0.1220],
    [0.1440, 0.1580, 0.1460, 0.1260],
    [0.1600, 0.1590, 0.1470, 0.1290],
    [0.1640, 0.1470, 0.1370, 0.1220],
    [0.1400, 0.1300, 0.1250, 0.1100],
    [0.1130, 0.1090, 0.1070, 0.0930],
];
const ORACLE: &str = include_str!(concat!(
    env!("CARGO_MANIFEST_DIR"),
    "/tests/fixtures/swaption_matrix/quantlib.csv"
));

fn today() -> Date {
    Date::new(15, Month::June, 2026)
}
fn options() -> Vec<Period> {
    [
        (1, TimeUnit::Months),
        (6, TimeUnit::Months),
        (1, TimeUnit::Years),
        (5, TimeUnit::Years),
        (10, TimeUnit::Years),
        (30, TimeUnit::Years),
    ]
    .into_iter()
    .map(|(n, unit)| Period::new(n, unit))
    .collect()
}
fn swaps() -> Vec<Period> {
    [1, 5, 10, 30]
        .into_iter()
        .map(|n| Period::new(n, TimeUnit::Years))
        .collect()
}
fn dates() -> Vec<Date> {
    options()
        .iter()
        .map(|&p| Target::new().advance_by_period(today(), p, BDC, false))
        .collect()
}
fn matrix() -> Matrix {
    let mut matrix = Matrix::with_size(6, 4);
    for (i, row) in DATA.iter().enumerate() {
        for (j, &value) in row.iter().enumerate() {
            matrix[(i, j)] = value;
        }
    }
    matrix
}

struct Fixture {
    settings: Shared<Settings<Date>>,
    quote: Shared<SimpleQuote>,
    link: RelinkableHandle<dyn Quote>,
    surfaces: Vec<Shared<SwaptionVolatilityMatrix>>,
}
impl Fixture {
    fn new(flat: bool) -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let quote = shared(SimpleQuote::new(DATA[0][0]));
        let link = RelinkableHandle::new(quote.clone() as Shared<dyn Quote>);
        let mut handles = matrix_to_handles(&matrix());
        handles[0][0] = link.handle();
        let moving = if flat {
            SwaptionVolatilityMatrix::moving_flat
        } else {
            SwaptionVolatilityMatrix::moving
        };
        let fixed = if flat {
            SwaptionVolatilityMatrix::new_flat
        } else {
            SwaptionVolatilityMatrix::new
        };
        let dc = Actual365Fixed::new();
        let vt = VolatilityType::ShiftedLognormal;
        let shifts = Matrix::default();
        let surfaces = vec![
            moving(
                Target::new(),
                BDC,
                options(),
                swaps(),
                handles.clone(),
                dc.clone(),
                vt,
                vec![],
                settings.clone(),
            )
            .unwrap(),
            SwaptionVolatilityMatrix::fixed_quotes(
                today(),
                Target::new(),
                BDC,
                options(),
                swaps(),
                handles,
                dc.clone(),
                vt,
                vec![],
                flat,
            )
            .unwrap(),
            SwaptionVolatilityMatrix::moving_matrix(
                Target::new(),
                BDC,
                options(),
                swaps(),
                &matrix(),
                dc.clone(),
                vt,
                &shifts,
                settings.clone(),
                flat,
            )
            .unwrap(),
            fixed(
                today(),
                Target::new(),
                BDC,
                options(),
                swaps(),
                &matrix(),
                dc.clone(),
                vt,
                &shifts,
            )
            .unwrap(),
            SwaptionVolatilityMatrix::with_option_dates(
                today(),
                Target::new(),
                BDC,
                dates(),
                swaps(),
                &matrix(),
                dc,
                vt,
                &shifts,
                flat,
            )
            .unwrap(),
        ]
        .into_iter()
        .map(shared)
        .collect();
        Self {
            settings,
            quote,
            link,
            surfaces,
        }
    }
}

fn swap_index(
    tenor: Period,
    curve: Handle<dyn YieldTermStructure>,
    settings: Shared<Settings<Date>>,
) -> Shared<SwapIndex> {
    let ibor = if tenor.length() > 1 {
        Euribor::six_months(curve, settings.clone())
    } else {
        Euribor::three_months(curve, settings.clone())
    };
    shared(SwapIndex::new(
        "EuriborSwapIsdaFixA".into(),
        tenor,
        2,
        Currency::eur(),
        Target::new(),
        Period::new(1, TimeUnit::Years),
        BDC,
        Thirty360::with_convention(Convention::BondBasis),
        shared(ibor),
        settings,
    ))
}

#[test]
fn five_constructor_forms_recover_nodes_and_black_vols_against_quantlib() {
    let fixture = Fixture::new(false);
    let curve = Handle::new(shared(FlatForward::with_rate(
        today(),
        0.05,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    )) as Shared<dyn YieldTermStructure>);
    let mut count = 0;
    for line in ORACLE.lines().filter(|line| line.starts_with("node,")) {
        let fields: Vec<_> = line.split(',').collect();
        let form: usize = fields[1].parse().unwrap();
        let i: usize = fields[2].parse().unwrap();
        let j: usize = fields[3].parse().unwrap();
        let expected_vol: f64 = fields[4].parse().unwrap();
        let expected_npv: f64 = fields[5].parse().unwrap();
        let oracle_recovered: f64 = fields[6].parse().unwrap();
        let surface = &fixture.surfaces[form];
        let grid = surface.discrete_grid().unwrap();
        assert_eq!(grid.option_dates, dates());
        for vol in [
            surface
                .volatility_tenors(options()[i], swaps()[j], 0.05, false)
                .unwrap(),
            surface
                .volatility(dates()[i], grid.swap_lengths[j], 0.05, false)
                .unwrap(),
            surface
                .volatility_time(grid.option_times[i], grid.swap_lengths[j], 0.05, false)
                .unwrap(),
        ] {
            assert!((vol - expected_vol).abs() <= 1e-16, "{line}: vol={vol}");
        }
        let engine = shared_mut(BlackSwaptionEngine::new(
            curve.clone(),
            Handle::new(surface.clone() as Shared<dyn SwaptionVolatilityStructure>),
            CashAnnuityModel::DiscountCurve,
            fixture.settings.clone(),
        ));
        let index = swap_index(swaps()[j], curve.clone(), fixture.settings.clone());
        let mut swaption = MakeSwaption::new(index.clone(), options()[i], None)
            .with_pricing_engine(engine)
            .build()
            .unwrap();
        assert_eq!(swaption.exercise().dates()[0], dates()[i]);
        {
            let underlying = swaption.underlying().borrow();
            assert_eq!(
                surface
                    .swap_length(
                        underlying.fixed_schedule().start_date(),
                        underlying.maturity_date().unwrap()
                    )
                    .unwrap(),
                grid.swap_lengths[j]
            );
        }
        let npv = swaption.npv().unwrap();
        assert!((npv - expected_npv).abs() <= 1e-12, "{line}: npv={npv}");
        let vol_quote = shared(SimpleQuote::new(expected_vol * 0.98));
        let flat_engine = shared_mut(BlackSwaptionEngine::with_flat_vol(
            curve.clone(),
            Handle::new(vol_quote.clone() as Shared<dyn Quote>),
            Actual365Fixed::new(),
            0.0,
            CashAnnuityModel::DiscountCurve,
            fixture.settings.clone(),
        ));
        let mut flat_option = MakeSwaption::new(index, options()[i], None)
            .with_pricing_engine(flat_engine)
            .build()
            .unwrap();
        let recovered = Brent::new()
            .with_max_evaluations(100)
            .solve_bracketed(
                |vol| {
                    vol_quote.set_value(vol);
                    flat_option.npv().unwrap() - npv
                },
                1e-6,
                expected_vol * 0.98,
                1e-6,
                4.0,
            )
            .unwrap();
        assert!(
            (recovered - expected_vol).abs() <= 1e-6,
            "{line}: recovered={recovered}"
        );
        assert!((recovered - oracle_recovered).abs() <= 1e-6);
        count += 1;
    }
    assert_eq!(count, 120);
}

#[test]
fn every_form_observes_only_its_live_inputs_and_handle_relinks() {
    for flat in [false, true] {
        let fixture = Fixture::new(flat);
        for line in ORACLE.lines().filter(|line| line.starts_with("observe,")) {
            let fields: Vec<_> = line.split(',').collect();
            let form: usize = fields[1].parse().unwrap();
            let expected: Vec<f64> = fields[2..6].iter().map(|x| x.parse().unwrap()).collect();
            let surface = &fixture.surfaces[form];
            let value = || surface.volatility(dates()[0], 1.0, 0.02, false).unwrap();
            assert!((value() - expected[0]).abs() <= 1e-16);
            let flag = Flag::new();
            surface.observable().register_observer(&as_observer(&flag));
            fixture
                .settings
                .set_evaluation_date(Date::new(15, Month::June, 2025));
            assert_eq!(Flag::is_up(&flag), form == 0 || form == 2);
            assert!((value() - expected[1]).abs() <= 1e-16, "{line}");
            fixture.settings.set_evaluation_date(today());
            assert!((value() - expected[0]).abs() <= 1e-16);
            let current = fixture.link.handle().current_link().unwrap();
            fixture
                .link
                .link_to(fixture.quote.clone() as Shared<dyn Quote>);
            fixture.quote.set_value(0.13);
            Flag::lower(&flag);
            fixture.quote.set_value(0.2);
            assert_eq!(Flag::is_up(&flag), form < 2);
            assert!((value() - expected[2]).abs() <= 1e-16);
            Flag::lower(&flag);
            fixture
                .link
                .link_to(shared(SimpleQuote::new(0.3)) as Shared<dyn Quote>);
            assert_eq!(Flag::is_up(&flag), form < 2);
            assert!((value() - expected[3]).abs() <= 1e-16);
            Flag::lower(&flag);
            fixture.quote.set_value(0.4);
            assert!(!Flag::is_up(&flag));
            assert!((value() - expected[3]).abs() <= 1e-16);
            fixture.link.link_to(current);
            fixture.quote.set_value(0.13);
        }
    }
}

#[test]
fn explicit_dates_validate_shape_and_preserve_irregular_nodes_and_copied_shifts() {
    let option_dates = vec![today() + 17, today() + 113, today() + 401];
    let mut values = Matrix::with_size(3, 2);
    let mut shifts = Matrix::with_size(3, 2);
    for i in 0..3 {
        for j in 0..2 {
            values[(i, j)] = 0.1 + i as f64 * 0.03 + j as f64 * 0.01;
            shifts[(i, j)] = 0.01 + i as f64 * 0.002 + j as f64 * 0.001;
        }
    }
    let build = |dates: Vec<Date>, vols: &Matrix, shifts: &Matrix, flat| {
        SwaptionVolatilityMatrix::with_option_dates(
            today(),
            Target::new(),
            BDC,
            dates,
            swaps()[..2].to_vec(),
            vols,
            Actual365Fixed::new(),
            VolatilityType::ShiftedLognormal,
            shifts,
            flat,
        )
    };
    let surface = build(option_dates.clone(), &values, &shifts, true).unwrap();
    let expected_vol = values[(2, 1)];
    let expected_shift = shifts[(2, 1)];
    values[(2, 1)] = 9.0;
    shifts[(2, 1)] = 8.0;
    assert_eq!(surface.discrete_grid().unwrap().option_dates, option_dates);
    assert!(
        (surface.volatility_time(10.0, 20.0, 0.05, true).unwrap() - expected_vol).abs() <= 1e-16
    );
    assert!((surface.shift_time(10.0, 20.0, true).unwrap() - expected_shift).abs() <= 1e-16);
    for invalid in [
        vec![],
        vec![today(), today() + 1, today() + 2],
        vec![today() + 2, today() + 2, today() + 3],
        vec![today() + 3, today() + 2, today() + 4],
    ] {
        assert!(build(invalid, &values, &shifts, false).is_err());
    }
    assert!(
        build(
            option_dates.clone(),
            &Matrix::with_size(2, 2),
            &shifts,
            false
        )
        .is_err()
    );
    assert!(
        build(
            option_dates.clone(),
            &values,
            &Matrix::with_size(3, 1),
            false
        )
        .is_err()
    );
    assert!(build(option_dates, &Matrix::with_size(3, 1), &shifts, false).is_err());
}
