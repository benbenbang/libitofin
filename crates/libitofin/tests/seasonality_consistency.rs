use libitofin::math::interpolations::linear::Linear;
use libitofin::termstructures::inflation::interpolatedzeroinflationcurve::ZeroInflationCurve;
use libitofin::termstructures::inflation::seasonality::{
    MultiplicativePriceSeasonality, Seasonality,
};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn frequency(value: &str) -> Frequency {
    match value {
        "4" => Frequency::Quarterly,
        "12" => Frequency::Monthly,
        "52" => Frequency::Weekly,
        "365" => Frequency::Daily,
        _ => panic!("unsupported fixture frequency: {value}"),
    }
}

#[test]
fn multi_year_consistency_matches_the_quantlib_oracle() {
    for line in include_str!("fixtures/seasonality_consistency.csv")
        .lines()
        .skip(1)
    {
        let fields: Vec<_> = line.split(',').collect();
        let name = fields[0];
        let mut factors = vec![1.0; fields[5].parse().unwrap()];
        for adjustment in fields[6].split(';').filter(|value| !value.is_empty()) {
            let (index, value) = adjustment.split_once(':').unwrap();
            factors[index.parse::<usize>().unwrap()] = value.parse().unwrap();
        }
        let base = Date::new(
            1,
            Month::from_ordinal(fields[4].parse().unwrap()),
            fields[3].parse().unwrap(),
        );
        let curve = ZeroInflationCurve::new(
            Date::new(13, Month::August, 2007),
            vec![base, Date::new(13, Month::August, 2012)],
            vec![0.02, 0.03],
            frequency(fields[2]),
            Actual365Fixed::new(),
            Linear,
            None,
        )
        .unwrap();
        let seasonality = MultiplicativePriceSeasonality::new(
            Date::new(31, Month::January, 2007),
            frequency(fields[1]),
            factors,
        )
        .unwrap();
        let actual = seasonality.is_consistent(&curve);
        if fields[7] == "true" {
            assert!(actual.unwrap(), "{name}");
        } else {
            let error = actual.expect_err(name);
            assert!(
                error.message().contains("seasonality is inconsistent"),
                "{name}: {error}"
            );
        }
    }
}

#[test]
fn anniversary_checks_report_the_supported_date_boundary() {
    for (year, count, accepted) in [
        (2199, 12, true),
        (2198, 24, true),
        (2199, 24, false),
        (2198, 36, false),
    ] {
        let base = Date::new(1, Month::January, year);
        let curve = ZeroInflationCurve::new(
            Date::new(1, Month::February, year),
            vec![base, Date::max_date()],
            vec![0.02, 0.03],
            Frequency::Monthly,
            Actual365Fixed::new(),
            Linear,
            None,
        )
        .unwrap();
        let seasonality =
            MultiplicativePriceSeasonality::new(base, Frequency::Monthly, vec![1.0; count])
                .unwrap();
        let result = seasonality.is_consistent(&curve);
        if accepted {
            assert!(result.unwrap());
        } else {
            assert!(
                result
                    .unwrap_err()
                    .message()
                    .contains("supported date range")
            );
        }
    }
}
