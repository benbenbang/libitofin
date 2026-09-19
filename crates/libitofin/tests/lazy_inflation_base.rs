use std::cell::Cell;

use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::Index;
use libitofin::indexes::inflation::UkRpi;
use libitofin::indexes::inflationindex::{CpiInterpolationType, ZeroInflationIndex};
use libitofin::math::interpolations::linear::Linear;
use libitofin::patterns::observable::Observer;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::inflation::inflationhelpers::{
    ZeroCouponInflationSwapHelper, ZeroInflationHelper,
};
use libitofin::termstructures::inflation::inflationtermstructure::{
    InflationTermStructure, ZeroInflationTermStructure,
};
use libitofin::termstructures::inflation::piecewisezeroinflationcurve::PiecewiseZeroInflationCurve;
use libitofin::termstructures::yields::Pillar;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::unitedkingdom::{Market, UnitedKingdom};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::thirty360::{Convention, Thirty360};
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::timeunit::TimeUnit;

const RATES: [f64; 14] = [
    2.93, 2.95, 2.965, 2.98, 3.0, 3.06, 3.175, 3.243, 3.293, 3.338, 3.348, 3.348, 3.308, 3.228,
];
const FIXINGS: [f64; 31] = [
    189.9, 189.9, 189.6, 190.5, 191.6, 192.0, 192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1,
    193.4, 194.2, 195.0, 196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6,
    203.1, 204.4, 205.4, 206.2, 207.3,
];

fn reference() -> Date {
    Date::new(13, Month::August, 2007)
}

fn dc() -> libitofin::time::daycounter::DayCounter {
    Thirty360::with_convention(Convention::BondBasis)
}

struct MarketData {
    settings: Shared<Settings<Date>>,
    index: Shared<ZeroInflationIndex>,
    quotes: Vec<Shared<SimpleQuote>>,
    helpers: Vec<Shared<dyn ZeroInflationHelper>>,
    handle: RelinkableHandle<dyn ZeroInflationTermStructure>,
}

impl MarketData {
    fn empty() -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference());
        let handle = RelinkableHandle::empty();
        let index =
            shared(UkRpi::new(Shared::clone(&settings)).with_term_structure(handle.handle()));
        let quotes: Vec<_> = RATES
            .iter()
            .map(|_| shared(SimpleQuote::new(None)))
            .collect();
        let helpers = [
            (13, 2008),
            (13, 2009),
            (13, 2010),
            (15, 2011),
            (13, 2012),
            (13, 2014),
            (13, 2017),
            (13, 2019),
            (15, 2022),
            (14, 2027),
            (13, 2032),
            (15, 2037),
            (13, 2047),
            (13, 2057),
        ]
        .into_iter()
        .zip(&quotes)
        .map(|((day, year), quote)| {
            ZeroCouponInflationSwapHelper::new(
                Handle::new(Shared::clone(quote) as Shared<dyn Quote>),
                Period::new(3, TimeUnit::Months),
                Date::new(day, Month::August, year),
                UnitedKingdom::new(Market::Settlement),
                BusinessDayConvention::ModifiedFollowing,
                dc(),
                &index,
                CpiInterpolationType::Flat,
                Pillar::LastRelevantDate,
                Shared::clone(&settings),
            )
            .unwrap() as Shared<dyn ZeroInflationHelper>
        })
        .collect();
        Self {
            settings,
            index,
            quotes,
            helpers,
            handle,
        }
    }

    fn populate_fixings(&self, last: f64) {
        for (i, fixing) in FIXINGS.iter().enumerate() {
            self.index
                .add_fixing(
                    Date::new(1, Month::January, 2005) + Period::new(i as i32, TimeUnit::Months),
                    if i == FIXINGS.len() - 1 {
                        last
                    } else {
                        *fixing
                    },
                )
                .unwrap();
        }
    }

    fn populate(&self) {
        for (quote, rate) in self.quotes.iter().zip(RATES) {
            quote.set_value(rate / 100.0);
        }
        self.populate_fixings(207.3);
    }

    fn lazy_curve(&self) -> Shared<PiecewiseZeroInflationCurve<Linear>> {
        PiecewiseZeroInflationCurve::with_last_fixing_date(
            reference(),
            &self.index,
            Frequency::Monthly,
            dc(),
            self.helpers.clone(),
            None,
        )
        .unwrap()
    }
}

struct Notifications(Shared<Cell<usize>>);

impl Observer for Notifications {
    fn update(&mut self) {
        self.0.set(self.0.get() + 1);
    }
}

#[test]
fn lazy_base_matches_upstream_fixed_curve_and_independent_recalibration_nodes() {
    let market = MarketData::empty();
    let curve = market.lazy_curve();
    market.populate();
    let fixed = PiecewiseZeroInflationCurve::new(
        reference(),
        market.index.last_fixing_date().unwrap(),
        Frequency::Monthly,
        dc(),
        market.helpers.clone(),
        None,
    )
    .unwrap();
    assert_eq!(
        curve.try_base_date().unwrap(),
        fixed.try_base_date().unwrap()
    );
    assert_eq!(curve.nodes().unwrap(), fixed.nodes().unwrap());
    curve.update();
    market
        .handle
        .link_to(Shared::clone(&curve) as Shared<dyn ZeroInflationTermStructure>);
    let notifications = shared(Cell::new(0));
    let observer =
        shared_mut(Notifications(Shared::clone(&notifications))) as SharedMut<dyn Observer>;
    curve.register_observer(&observer);
    let oracle: Vec<Vec<&str>> = include_str!("fixtures/lazy_inflation_base/oracle.csv")
        .lines()
        .skip(1)
        .map(|line| line.split(',').collect())
        .collect();
    assert_eq!(oracle.len(), 75);
    let mut previous = Vec::new();
    for phase in 0..5 {
        let before = notifications.get();
        match phase {
            1 => {
                market.index.clear_fixings();
                assert!(curve.try_base_date().is_err());
                assert_eq!(curve.base_date(), Date::null());
                market.populate_fixings(208.1);
            }
            2 => {
                market
                    .settings
                    .set_evaluation_date(Date::new(13, Month::September, 2007));
                market
                    .index
                    .add_fixing(Date::new(1, Month::August, 2007), 208.4)
                    .unwrap();
            }
            3 => market
                .settings
                .set_evaluation_date(Date::new(13, Month::October, 2007)),
            4 => {
                market.quotes[0].set_value(RATES[0] / 100.0 + 0.0005);
            }
            _ => {}
        }
        if phase > 0 {
            assert!(
                notifications.get() > before,
                "phase {phase} failed to notify"
            );
        }
        let nodes = curve.nodes().unwrap();
        let rows = &oracle[phase * 15..(phase + 1) * 15];
        assert_eq!(nodes.len(), rows.len());
        for ((date, rate), row) in nodes.iter().zip(rows) {
            assert_eq!(*date, Date::from_serial(row[2].parse().unwrap()));
            let expected: f64 = row[3].parse().unwrap();
            assert!(
                (rate - expected).abs() < 1e-12,
                "phase {phase}, {date}: {rate} vs {expected}"
            );
        }
        let forecast = market
            .index
            .fixing(Date::new(1, Month::August, 2012), false)
            .unwrap();
        let expected: f64 = rows[0][4].parse().unwrap();
        assert!(
            (forecast - expected).abs() < 1e-7,
            "phase {phase}: {forecast} vs {expected}"
        );
        for helper in &market.helpers {
            assert!(helper.quote_error().unwrap().abs() < 1e-12);
        }
        if phase > 0 {
            assert_ne!(nodes, previous, "phase {phase} did not recalculate");
        }
        previous = nodes;
    }
}

#[test]
fn missing_inputs_fail_fallible_reads_and_repair_without_reconstruction() {
    let market = MarketData::empty();
    let curve = market.lazy_curve();
    let missing = curve.try_base_date().unwrap_err();
    assert!(missing.message().contains("no fixings stored"));
    assert_eq!(curve.base_date(), Date::null());
    assert!(
        curve
            .zero_rate(1.0, false)
            .unwrap_err()
            .message()
            .contains("no fixings stored")
    );
    assert!(
        curve
            .zero_rate_date(reference(), false)
            .unwrap_err()
            .message()
            .contains("no fixings stored")
    );
    market
        .handle
        .link_to(Shared::clone(&curve) as Shared<dyn ZeroInflationTermStructure>);
    assert!(
        market
            .index
            .fixing(Date::new(1, Month::August, 2012), false)
            .unwrap_err()
            .message()
            .contains("no fixings stored")
    );
    market.populate_fixings(207.3);
    assert!(curve.nodes().is_err());
    market.populate();
    assert_eq!(
        curve.try_base_date().unwrap(),
        Date::new(1, Month::July, 2007)
    );
    assert_eq!(curve.nodes().unwrap().len(), 15);
}

#[test]
fn owned_unlinked_index_copy_survives_original_drop_and_does_not_retain_curve() {
    let market = MarketData::empty();
    let curve = market.lazy_curve();
    market.populate();
    market
        .handle
        .link_to(Shared::clone(&curve) as Shared<dyn ZeroInflationTermStructure>);
    let before = curve.nodes().unwrap();
    let weak_curve = Shared::downgrade(&curve);
    let weak_original = Shared::downgrade(&market.index);
    let settings = Shared::clone(&market.settings);
    drop(market);
    assert!(weak_original.upgrade().is_none());
    assert_eq!(curve.nodes().unwrap(), before);
    settings.set_evaluation_date(Date::new(13, Month::September, 2007));
    assert_ne!(curve.nodes().unwrap(), before);
    drop(curve);
    assert!(weak_curve.upgrade().is_none());
}

#[test]
fn relinking_the_original_forecast_handle_does_not_notify_the_base_date_source() {
    use libitofin::termstructures::inflation::interpolatedzeroinflationcurve::ZeroInflationCurve;

    let market = MarketData::empty();
    market.populate();
    let curve = market.lazy_curve();
    let nodes = curve.nodes().unwrap();
    let notifications = shared(Cell::new(0));
    let observer =
        shared_mut(Notifications(Shared::clone(&notifications))) as SharedMut<dyn Observer>;
    curve.register_observer(&observer);
    let other = shared(
        ZeroInflationCurve::new(
            reference(),
            vec![
                Date::new(1, Month::July, 2007),
                Date::new(1, Month::July, 2060),
            ],
            vec![0.05, 0.05],
            Frequency::Monthly,
            dc(),
            Linear,
            None,
        )
        .unwrap(),
    );
    market
        .handle
        .link_to(other as Shared<dyn ZeroInflationTermStructure>);
    assert_eq!(notifications.get(), 0);
    assert_eq!(curve.nodes().unwrap(), nodes);
    market
        .handle
        .link_to(Shared::clone(&curve) as Shared<dyn ZeroInflationTermStructure>);
    assert_eq!(notifications.get(), 0);
    drop(market);
    let weak = Shared::downgrade(&curve);
    drop(curve);
    assert!(weak.upgrade().is_none());
}

#[test]
fn arbitrary_callback_is_lazy_cached_owned_and_explicitly_invalidated() {
    let market = MarketData::empty();
    market.populate();
    let date = shared(Cell::new(Date::null()));
    let captured = Shared::clone(&date);
    let calls = shared(Cell::new(0));
    let captured_calls = Shared::clone(&calls);
    let curve = PiecewiseZeroInflationCurve::with_base_date_func(
        reference(),
        move || {
            captured_calls.set(captured_calls.get() + 1);
            Ok(captured.get())
        },
        Frequency::Monthly,
        dc(),
        market.helpers.clone(),
        None,
    )
    .unwrap();
    assert_eq!(calls.get(), 0);
    assert!(
        curve
            .try_base_date()
            .unwrap_err()
            .message()
            .contains("null lazy base date")
    );
    date.set(Date::new(1, Month::July, 2007));
    assert_eq!(curve.try_base_date().unwrap(), date.get());
    assert_eq!(calls.get(), 2);
    curve.nodes().unwrap();
    assert_eq!(calls.get(), 2);
    date.set(Date::new(1, Month::June, 2007));
    assert_eq!(
        curve.try_base_date().unwrap(),
        Date::new(1, Month::July, 2007)
    );
    curve.update();
    assert_eq!(curve.try_base_date().unwrap(), date.get());
    assert_eq!(calls.get(), 3);
    let weak_date = Shared::downgrade(&date);
    drop(date);
    assert!(weak_date.upgrade().is_some());
    drop(curve);
    assert!(weak_date.upgrade().is_none());
    assert!(
        PiecewiseZeroInflationCurve::with_last_fixing_date(
            reference(),
            &market.index,
            Frequency::Monthly,
            dc(),
            Vec::new(),
            None,
        )
        .is_err()
    );
}

#[test]
fn fixed_base_inspection_does_not_require_quotes_or_fixings() {
    let market = MarketData::empty();
    let base = Date::new(1, Month::July, 2007);
    let curve = PiecewiseZeroInflationCurve::new(
        reference(),
        base,
        Frequency::Monthly,
        dc(),
        market.helpers,
        None,
    )
    .unwrap();
    assert_eq!(curve.base_date(), base);
    assert_eq!(curve.try_base_date().unwrap(), base);
    assert!(curve.nodes().is_err());
    assert_eq!(curve.try_base_date().unwrap(), base);
}

#[test]
fn direct_seasonality_queries_and_installation_propagate_lazy_errors_and_repair() {
    use libitofin::termstructures::inflation::seasonality::{
        KerkhofSeasonality, MultiplicativePriceSeasonality, Seasonality,
    };

    for callback_failure in [false, true] {
        let market = MarketData::empty();
        let fail_callback = shared(Cell::new(callback_failure));
        let captured = Shared::clone(&fail_callback);
        let curve = if callback_failure {
            PiecewiseZeroInflationCurve::with_base_date_func(
                reference(),
                move || {
                    if captured.get() {
                        libitofin::fail!("base-date callback unavailable");
                    }
                    Ok(Date::new(1, Month::July, 2007))
                },
                Frequency::Monthly,
                dc(),
                market.helpers.clone(),
                None,
            )
            .unwrap()
        } else {
            market.lazy_curve()
        };
        let expected = if callback_failure {
            "base-date callback unavailable"
        } else {
            "no fixings stored"
        };
        let seasonality = shared(
            MultiplicativePriceSeasonality::new(
                Date::new(31, Month::January, 2007),
                Frequency::Monthly,
                vec![1.0; 24],
            )
            .unwrap(),
        );
        let kerkhof =
            KerkhofSeasonality::new(Date::new(31, Month::January, 2007), vec![1.0; 12]).unwrap();
        let query = Date::new(1, Month::August, 2012);
        for result in [
            seasonality.correct_zero_rate(query, 0.03, curve.as_ref()),
            seasonality.correct_yoy_rate(query, 0.03, curve.as_ref()),
            kerkhof.correct_zero_rate(query, 0.03, curve.as_ref()),
        ] {
            assert!(result.unwrap_err().message().contains(expected));
        }
        assert!(
            seasonality
                .is_consistent(curve.as_ref())
                .unwrap_err()
                .message()
                .contains(expected)
        );
        assert!(
            curve
                .set_seasonality(Some(Shared::clone(&seasonality) as Shared<dyn Seasonality>))
                .unwrap_err()
                .message()
                .contains(expected)
        );
        assert!(curve.has_seasonality());
        fail_callback.set(false);
        market.populate();
        assert_eq!(
            curve.try_base_date().unwrap(),
            Date::new(1, Month::July, 2007)
        );
        assert_eq!(curve.nodes().unwrap().len(), 15);
        assert!(seasonality.is_consistent(curve.as_ref()).unwrap());
        for corrected in [
            kerkhof
                .correct_zero_rate(query, 0.03, curve.as_ref())
                .unwrap(),
            seasonality
                .correct_zero_rate(query, 0.03, curve.as_ref())
                .unwrap(),
            seasonality
                .correct_yoy_rate(query, 0.03, curve.as_ref())
                .unwrap(),
        ] {
            assert!((corrected - 0.03).abs() < 1e-14);
        }
        curve
            .set_seasonality(Some(seasonality as Shared<dyn Seasonality>))
            .unwrap();
        assert!(curve.zero_rate_date(query, false).unwrap().is_finite());
    }
}
