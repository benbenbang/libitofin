use libitofin::math::interpolations::linear::Linear;
use libitofin::shared::shared;
use libitofin::termstructures::TermStructure;
use libitofin::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, ZeroInflationTermStructure,
};
use libitofin::termstructures::inflation::interpolatedzeroinflationcurve::ZeroInflationCurve;
use libitofin::termstructures::inflation::seasonality::{
    KerkhofSeasonality, MultiplicativePriceSeasonality, Seasonality,
};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::daycounters::thirty360::{Convention, Thirty360};
use libitofin::time::frequency::Frequency;

const FACTORS: [f64; 12] = [
    1.20, 1.004, 0.997, 1.006, 0.995, 1.003, 0.991, 1.008, 0.998, 1.005, 0.996, 1.002,
];

fn close(actual: f64, expected: f64, context: &str) {
    if expected.is_infinite() {
        assert_eq!(actual, expected, "{context}");
    } else {
        assert!(
            (actual - expected).abs() < 1.0e-13,
            "{context}: actual {actual:.17}, expected {expected:.17}"
        );
    }
}

#[test]
fn kerkhof_numerics_and_curve_consumers_match_quantlib() {
    let mut discriminating = 0;
    let mut count = 0;
    for line in include_str!("fixtures/kerkhof_seasonality.csv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split(',').collect();
        let date = |i: usize| Date::from_serial(fields[i].parse().unwrap());
        let number = |i: usize| fields[i].parse::<f64>().unwrap();
        let frequency = match fields[3] {
            "12" => Frequency::Monthly,
            "4" => Frequency::Quarterly,
            other => panic!("unexpected frequency {other}"),
        };
        let dc = if fields[4] == "A365" {
            Actual365Fixed::new()
        } else {
            Thirty360::with_convention(Convention::BondBasis)
        };
        let curve = ZeroInflationCurve::new(
            Date::new(13, Month::August, 2007),
            vec![date(1), Date::new(1, Month::January, 2015)],
            vec![0.02, 0.035],
            frequency,
            dc,
            Linear,
            None,
        )
        .unwrap();
        let seasonality = shared(KerkhofSeasonality::new(date(0), FACTORS.to_vec()).unwrap());
        close(
            seasonality.seasonality_factor(date(2)).unwrap(),
            number(5),
            line,
        );
        close(
            seasonality
                .correct_zero_rate(date(2), 0.027, &curve)
                .unwrap(),
            number(6),
            line,
        );
        let multiplicative =
            MultiplicativePriceSeasonality::new(date(0), Frequency::Monthly, FACTORS.to_vec())
                .unwrap();
        close(
            multiplicative
                .correct_zero_rate(date(2), 0.027, &curve)
                .unwrap(),
            number(7),
            line,
        );
        discriminating += usize::from((number(6) - number(7)).abs() > 1.0e-6);
        assert_eq!(seasonality.is_consistent(&curve).unwrap(), fields[8] == "1");
        let error = seasonality
            .correct_yoy_rate(date(2), 0.027, &curve)
            .unwrap_err();
        assert!(error.message().contains("not defined on YoY rates"));
        curve.set_seasonality(Some(seasonality)).unwrap();
        if date(2) < date(1) {
            assert!(curve.zero_rate_date(date(2), true).is_err());
        } else {
            close(
                curve.zero_rate_date(date(2), true).unwrap(),
                number(9),
                line,
            );
            let time = curve.time_from_reference(date(2)).unwrap();
            let raw = curve.zero_rate(time, true).unwrap();
            curve.set_seasonality(None).unwrap();
            close(curve.zero_rate(time, true).unwrap(), raw, line);
        }
        count += 1;
    }
    assert_eq!(count, 72);
    assert!(discriminating > 40);
}

#[test]
fn kerkhof_rejects_invalid_shapes_and_preserves_a_rejected_replacement() {
    let base = Date::new(31, Month::January, 2007);
    let mut seasonality = KerkhofSeasonality::new(base, FACTORS.to_vec()).unwrap();
    for count in [0, 1, 11, 13, 24, 36] {
        let invalid = vec![1.0; count];
        assert!(KerkhofSeasonality::new(base, invalid.clone()).is_err());
        assert!(
            seasonality
                .set(Date::new(1, Month::July, 2008), invalid)
                .is_err()
        );
        assert_eq!(seasonality.seasonality_base_date(), base);
        assert_eq!(seasonality.frequency(), Frequency::Monthly);
        assert_eq!(seasonality.seasonality_factors(), FACTORS);
    }
    seasonality
        .set(Date::new(1, Month::July, 2008), vec![1.0; 12])
        .unwrap();
    assert_eq!(seasonality.seasonality_factor(base).unwrap(), 1.0);
}

#[test]
fn kerkhof_uses_calendar_month_indices_and_ignores_element_zero_and_years() {
    let mut factors = FACTORS;
    factors[0] = f64::NAN;
    for month in 1..=12 {
        let anchor = Date::new(1, Month::from_ordinal(month), 2007);
        let seasonality = KerkhofSeasonality::new(anchor, factors.to_vec()).unwrap();
        for year in [1901, 2006, 2007, 2008, 2199] {
            let anniversary = Date::new(15, anchor.month(), year);
            assert_eq!(seasonality.seasonality_factor(anniversary).unwrap(), 1.0);
        }
    }
    let seasonality =
        KerkhofSeasonality::new(Date::new(1, Month::January, 2007), factors.to_vec()).unwrap();
    assert_eq!(
        seasonality
            .seasonality_factor(Date::new(1, Month::February, 2008))
            .unwrap(),
        FACTORS[1]
    );
}

#[test]
fn kerkhof_yoy_curve_reports_unsupported_correction_and_can_be_cleared() {
    use libitofin::termstructures::inflation::inflationtermstructure::YoYInflationTermStructure;
    use libitofin::termstructures::inflation::interpolatedyoyinflationcurve::YoYInflationCurve;

    let base = Date::new(1, Month::July, 2007);
    let curve = YoYInflationCurve::new(
        Date::new(13, Month::August, 2007),
        vec![base, Date::new(1, Month::January, 2015)],
        vec![0.02, 0.035],
        Frequency::Monthly,
        Actual365Fixed::new(),
        Linear,
        Some(shared(
            KerkhofSeasonality::new(base, FACTORS.to_vec()).unwrap(),
        )),
    )
    .unwrap();
    let query = Date::new(15, Month::August, 2008);
    assert!(curve.check_seasonality().is_ok());
    assert!(
        curve
            .yoy_rate_date(query, false)
            .unwrap_err()
            .message()
            .contains("not defined on YoY rates")
    );
    curve.set_seasonality(None).unwrap();
    assert!(curve.yoy_rate_date(query, false).unwrap().is_finite());
}
