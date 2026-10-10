//! Whole-class compiled QuantLib 1.43 fixtures; reproduction metadata is tracked.
use libitofin::currency::Currency;
use libitofin::errors::QlResult;
use libitofin::exchangerate::ExchangeRate;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::{IborIndex, Index};
use libitofin::instruments::XccyBasisSwap;
use libitofin::interestrate::Compounding;
use libitofin::quotes::Quote;
use libitofin::quotes::SimpleQuote;
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::{nullcalendar::NullCalendar, weekendsonly::WeekendsOnly};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::frequency::Frequency;
use libitofin::time::period::Period;
use libitofin::time::schedule::Schedule;
use libitofin::time::timeunit::TimeUnit;

fn rows() -> Vec<(&'static str, Vec<f64>)> {
    include_str!("fixtures/medium3_xccy_basis/native.csv")
        .lines()
        .skip(1)
        .map(|line| {
            let mut cells = line.split(',');
            (
                cells.next().unwrap(),
                cells.map(|x| x.parse().unwrap()).collect(),
            )
        })
        .collect()
}
fn date(serial: f64) -> Date {
    Date::from_serial(serial as i32)
}
fn dates(asymmetric: bool) -> Vec<Date> {
    let months = if asymmetric {
        vec![
            (2, 2026),
            (5, 2026),
            (8, 2026),
            (11, 2026),
            (2, 2027),
            (5, 2027),
        ]
    } else {
        vec![(1, 2026), (4, 2026), (7, 2026), (10, 2026), (1, 2027)]
    };
    months
        .into_iter()
        .map(|(m, y)| Date::new(8, Month::from_ordinal(m), y))
        .collect()
}
fn quote_schedule(row: &[f64]) -> Schedule {
    if row[0] != 2.0 {
        return Schedule::from_dates(dates(row[0] != 0.0));
    }
    let dates = dates(false).into_iter().map(|date| date + 9).collect();
    Schedule::with_metadata(
        dates,
        WeekendsOnly::new(),
        BusinessDayConvention::Unadjusted,
        None,
        None,
        None,
        None,
        vec![],
    )
}
fn flat(reference: Date, rate: f64) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        reference,
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}
struct Market {
    settings: Shared<Settings<Date>>,
    base: Shared<IborIndex>,
    quote: Shared<IborIndex>,
    rates: [Shared<SimpleQuote>; 4],
    curves: [RelinkableHandle<dyn YieldTermStructure>; 4],
}
impl Market {
    fn new(row: &[f64]) -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(date(row[1]));
        settings.set_include_todays_cash_flows(if row[12] < 0.0 {
            None
        } else {
            Some(row[12] != 0.0)
        });
        let rates = [8, 9, 10, 11].map(|i| shared(SimpleQuote::new(row[i])));
        let curves = std::array::from_fn(|i| {
            RelinkableHandle::new(shared(FlatForward::new(
                date(46027.0),
                Handle::new(rates[i].clone() as Shared<dyn Quote>),
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        });
        let index = |name: &str, ccy, forwarding| {
            shared(IborIndex::new(
                name.into(),
                Period::new(3, TimeUnit::Months),
                0,
                ccy,
                NullCalendar::new(),
                BusinessDayConvention::Unadjusted,
                false,
                Actual360::new(),
                forwarding,
                settings.clone(),
            ))
        };
        let base = index("M3Base", Currency::eur(), curves[0].handle());
        let quote = index("M3Quote", Currency::usd(), curves[1].handle());
        for (index, schedule, fixing) in [
            (&base, dates(false), 0.022),
            (&quote, dates(row[0] != 0.0), 0.034),
        ] {
            for fixing_date in schedule.iter().take(schedule.len() - 1) {
                if *fixing_date < date(row[1]) {
                    index.add_fixing(*fixing_date, fixing).unwrap();
                }
            }
        }
        Self {
            settings,
            base,
            quote,
            rates,
            curves,
        }
    }
    fn swap(
        &self,
        row: &[f64],
        spread: f64,
        base_schedule: Schedule,
        spot: ExchangeRate,
    ) -> QlResult<XccyBasisSwap> {
        XccyBasisSwap::new(
            base_schedule,
            self.base.clone(),
            self.curves[2].handle(),
            1_000_000.0,
            Actual360::new(),
            quote_schedule(row),
            self.quote.clone(),
            self.curves[3].handle(),
            row[6],
            Actual360::new(),
            spread,
            spot,
            None,
            self.settings.clone(),
        )?
        .with_valuation_dates(date(row[2]), date(row[3]), row[5] != 0.0)?
        .with_spot_settlement_date(date(row[4]))
    }
    fn standard(&self, row: &[f64], spread: f64) -> XccyBasisSwap {
        self.swap(
            row,
            spread,
            Schedule::from_dates(dates(false)),
            ExchangeRate::new(Currency::eur(), Currency::usd(), 1.27),
        )
        .unwrap()
    }
}
fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}"
    );
}
#[test]
fn compiled_whole_class_oracle_in_both_currencies_and_fair_spread() {
    for (name, row) in rows() {
        let market = Market::new(&row);
        let swap = market.standard(&row, row[7]);
        for (actual, expected) in [
            (swap.base_leg_pv().unwrap(), row[13]),
            (swap.quote_leg_pv().unwrap(), row[14]),
            (swap.quote_leg_bps().unwrap(), row[15]),
            (swap.npv().unwrap().value(), row[16]),
            (swap.npv_in_quote().unwrap().value(), row[17]),
        ] {
            close(actual, expected, 2e-9);
        }
        assert_eq!(swap.npv().unwrap().currency(), &Currency::eur());
        assert_eq!(swap.npv_in_quote().unwrap().currency(), &Currency::usd());
        if name == "expired" {
            assert!(swap.fair_quote_basis_spread().is_err());
        } else {
            close(swap.fair_quote_basis_spread().unwrap(), row[18], 2e-13);
            let fair = market.standard(&row, swap.fair_quote_basis_spread().unwrap());
            close(fair.npv_in_quote().unwrap().value(), 0.0, 2e-9);
        }
    }
}
#[test]
fn curve_quotes_and_relinks_are_live_without_cache() {
    let fixtures = rows();
    let row = &fixtures[0].1;
    let updated = &fixtures[7].1;
    let market = Market::new(row);
    let swap = market.standard(row, row[7]);
    for (quote, i) in market.rates.iter().zip(8..12) {
        quote.set_value(updated[i]);
    }
    close(swap.npv_in_quote().unwrap().value(), updated[17], 2e-9);
    for (curve, i) in market.curves.iter().zip(8..12) {
        curve.link_to(flat(date(46027.0), row[i]));
    }
    close(swap.npv_in_quote().unwrap().value(), row[17], 2e-9);
    market.curves[3].link_to(flat(date(46028.0), row[11]));
    assert!(swap.npv().is_err());
    market.curves[3].reset();
    assert!(swap.quote_leg_pv().is_err());
}
