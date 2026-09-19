//! QuantLib 1.43 constructor, Black-engine and observable-handle oracles for #570.

use super::*;
use crate::currency::Currency;
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
    surfaces: Vec<Shared<SwaptionVolatilityMatrix>>,
}
impl Fixture {
    fn new(flat: bool) -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let handles = matrix_to_handles(&matrix());
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
        Self { settings, surfaces }
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
