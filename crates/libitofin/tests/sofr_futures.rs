use libitofin::cashflows::RateAveraging;
use libitofin::handle::Handle;
use libitofin::indexes::ibor::sofr::Sofr;
use libitofin::indexes::index::Index;
use libitofin::instrument::Instrument;
use libitofin::instruments::OvernightIndexFuture;
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::bootstraphelper::RateHelper;
use libitofin::termstructures::bootstraptraits::Discount;
use libitofin::termstructures::yields::{
    FlatForward, OvernightIndexFutureRateHelper, PiecewiseYieldCurve, Pillar, SofrFutureRateHelper,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;

fn quote(value: f64) -> Handle<dyn Quote> {
    Handle::new(shared(SimpleQuote::new(value)) as Shared<dyn Quote>)
}
fn settings(today: Date) -> Shared<Settings<Date>> {
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    settings
}
fn near(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        actual.is_finite() && (actual - expected).abs() <= tolerance,
        "{actual} != {expected}"
    );
}

#[test]
fn quantlib_bootstrap_and_juneteenth_prices_and_curve_nodes() {
    for june in [false, true] {
        let today = if june {
            Date::new(27, Month::June, 2024)
        } else {
            Date::new(26, Month::October, 2018)
        };
        let settings = settings(today);
        let index = Sofr::new(Handle::empty(), settings.clone());
        if june {
            for day in [18, 20, 21, 24, 25, 26, 27] {
                index
                    .add_fixing(Date::new(day, Month::June, 2024), 0.02)
                    .unwrap();
            }
        } else {
            for (day, rate) in [
                (1, 0.0222),
                (2, 0.022),
                (3, 0.022),
                (4, 0.0218),
                (5, 0.0216),
                (9, 0.0215),
                (10, 0.0215),
                (11, 0.0217),
                (12, 0.0218),
                (15, 0.0221),
                (16, 0.0218),
                (17, 0.0218),
                (18, 0.0219),
                (19, 0.0219),
                (22, 0.0218),
                (23, 0.0217),
                (24, 0.0218),
                (25, 0.0219),
            ] {
                index
                    .add_fixing(Date::new(day, Month::October, 2018), rate)
                    .unwrap();
            }
        }
        let rows: Vec<Vec<&str>> = include_str!("fixtures/sofr_futures/curves.csv")
            .lines()
            .skip(1)
            .map(|line| line.split(',').collect::<Vec<_>>())
            .filter(|row| (row[0] == "juneteenth") == june)
            .collect();
        let helpers: Vec<Shared<dyn RateHelper>> = rows
            .iter()
            .map(|row| {
                let freq = if row[3] == "12" {
                    Frequency::Monthly
                } else {
                    Frequency::Quarterly
                };
                let helper = SofrFutureRateHelper::new(
                    quote(row[4].parse().unwrap()),
                    Month::from_ordinal(row[2].parse().unwrap()),
                    row[1].parse().unwrap(),
                    freq,
                    Handle::empty(),
                    Pillar::LastRelevantDate,
                    settings.clone(),
                )
                .unwrap();
                assert_eq!(
                    helper.earliest_date(),
                    Date::from_serial(row[5].parse().unwrap())
                );
                assert_eq!(
                    helper.maturity_date(),
                    Date::from_serial(row[6].parse().unwrap())
                );
                helper as Shared<dyn RateHelper>
            })
            .collect();
        let curve = PiecewiseYieldCurve::<Discount, Linear>::new(
            today,
            helpers.clone(),
            Actual365Fixed::new(),
            Linear,
        )
        .unwrap();
        curve
            .discount_date(helpers.last().unwrap().maturity_date(), false)
            .unwrap();
        for (row, helper) in rows.iter().zip(&helpers) {
            if june {
                near(
                    curve.discount_date(helper.maturity_date(), false).unwrap(),
                    row[7].parse().unwrap(),
                    1e-10,
                );
            }
            if june || row[3] == "4" {
                near(
                    helper.implied_quote().unwrap(),
                    row[4].parse().unwrap(),
                    1e-9,
                );
            }
        }
        let (start, end, price) = if june {
            (
                Date::new(19, Month::June, 2024),
                Date::new(18, Month::September, 2024),
                97.220,
            )
        } else {
            (
                Date::new(20, Month::March, 2019),
                Date::new(19, Month::June, 2019),
                97.440,
            )
        };
        let weak = Shared::downgrade(&curve);
        let index = shared(Sofr::new(
            Handle::new(curve.clone() as Shared<dyn YieldTermStructure>),
            settings,
        ));
        let convexity = shared(SimpleQuote::new(0.0));
        let mut future = OvernightIndexFuture::new(
            index,
            start,
            end,
            Handle::new(convexity.clone() as Shared<dyn Quote>),
            RateAveraging::Compound,
        )
        .unwrap();
        near(future.npv().unwrap(), price, 1e-9);
        convexity.set_value(0.1);
        near(future.npv().unwrap(), price - 10.0, 1e-9);
        drop(curve);
        assert!(weak.upgrade().is_some());
        convexity.set_value(0.2);
        near(future.npv().unwrap(), price - 20.0, 1e-9);
        drop(future);
        assert!(weak.upgrade().is_none());
        assert!(helpers[0].implied_quote().is_err());
        let dates = std::iter::once(today)
            .chain(
                rows.iter()
                    .map(|r| Date::from_serial(r[6].parse().unwrap())),
            )
            .collect();
        let discounts = std::iter::once(1.0)
            .chain(rows.iter().map(|r| r[7].parse().unwrap()))
            .collect();
        let frozen: Shared<dyn YieldTermStructure> = shared(
            libitofin::termstructures::yields::InterpolatedDiscountCurve::<Linear>::new(
                dates,
                discounts,
                Actual365Fixed::new(),
                None,
            )
            .unwrap(),
        );
        for (row, helper) in rows.iter().zip(&helpers) {
            helper.set_term_structure(&frozen);
            near(
                helper.implied_quote().unwrap(),
                row[8].parse().unwrap(),
                1e-9,
            );
        }
    }
}

#[test]
fn quantlib_holiday_clipping_and_today_fixing_oracle() {
    for row in include_str!("fixtures/sofr_futures/accrual.csv")
        .lines()
        .skip(1)
    {
        let row: Vec<_> = row.split(',').collect();
        let today = Date::from_serial(row[0].parse().unwrap());
        let settings = settings(today);
        let curve = shared(FlatForward::with_rate(
            Date::new(17, Month::June, 2024),
            0.035,
            Actual365Fixed::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        let index = shared(Sofr::new(
            Handle::new(curve as Shared<dyn YieldTermStructure>),
            settings,
        ));
        for (day, rate) in [(18, 0.02), (20, 0.025), (21, 0.03), (24, 0.04)] {
            let date = Date::new(day, Month::June, 2024);
            if date < today || (row[4] == "1" && date == today) {
                index.add_fixing(date, rate).unwrap();
            }
        }
        let averaging = if row[3] == "0" {
            RateAveraging::Simple
        } else {
            RateAveraging::Compound
        };
        let mut future = OvernightIndexFuture::new(
            index,
            Date::from_serial(row[1].parse().unwrap()),
            Date::from_serial(row[2].parse().unwrap()),
            Handle::empty(),
            averaging,
        )
        .unwrap();
        near(future.npv().unwrap(), row[5].parse().unwrap(), 1e-9);
    }
}

#[path = "sofr_futures/lifecycle.rs"]
mod lifecycle;
