//! Shared fixture construction, not a valuation oracle.
use libitofin::currency::Currency;
use libitofin::errors::QlResult;
use libitofin::handle::{Handle, RelinkableHandle};
use libitofin::indexes::{IborIndex, Index};
use libitofin::instrument::Instrument;
use libitofin::instruments::FloatFloatSwap;
use libitofin::interestrate::Compounding;
use libitofin::pricingengines::swap::DiscountingSwapEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::calendars::{nullcalendar::NullCalendar, weekendsonly::WeekendsOnly};
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
use libitofin::time::{
    frequency::Frequency, period::Period, schedule::Schedule, timeunit::TimeUnit,
};

pub(super) fn reference() -> Date {
    Date::new(2, Month::January, 2026)
}
pub(super) fn dates(second: bool) -> Vec<Date> {
    let months = if second {
        vec![(1, 2026), (7, 2026), (1, 2027)]
    } else {
        vec![(1, 2026), (4, 2026), (7, 2026), (10, 2026), (1, 2027)]
    };
    months
        .into_iter()
        .map(|(month, year)| Date::new(17, Month::from_ordinal(month), year))
        .collect()
}
pub(super) fn schedule(second: bool) -> Schedule {
    Schedule::with_metadata(
        dates(second),
        WeekendsOnly::new(),
        BusinessDayConvention::Unadjusted,
        None,
        None,
        None,
        None,
        vec![],
    )
}
pub(super) fn rows() -> Vec<(&'static str, Vec<f64>)> {
    include_str!("native.csv")
        .lines()
        .skip(1)
        .map(|line| {
            let mut cells = line.split(',');
            (
                cells.next().unwrap(),
                cells.map(|cell| cell.parse().unwrap()).collect(),
            )
        })
        .collect()
}
pub(super) fn date(serial: f64) -> Date {
    Date::from_serial(serial as i32)
}
pub(super) fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}"
    );
}
pub(super) struct Market {
    pub(super) settings: Shared<Settings<Date>>,
    pub(super) indexes: [Shared<IborIndex>; 2],
    pub(super) quotes: [Shared<SimpleQuote>; 3],
    pub(super) curves: [RelinkableHandle<dyn YieldTermStructure>; 3],
}
impl Market {
    pub(super) fn new(row: &[f64]) -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(date(row[0]));
        settings.set_include_todays_cash_flows(if row[4] < 0.0 {
            None
        } else {
            Some(row[4] != 0.0)
        });
        let quotes = [5, 6, 7].map(|i| shared(SimpleQuote::new(row[i])));
        let curves = std::array::from_fn(|i| {
            RelinkableHandle::new(shared(FlatForward::new(
                reference(),
                Handle::new(quotes[i].clone() as Shared<dyn Quote>),
                Actual360::new(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        });
        let indexes = std::array::from_fn(|i| {
            shared(IborIndex::new(
                format!("M4Float{i}"),
                Period::new(if i == 0 { 3 } else { 6 }, TimeUnit::Months),
                0,
                Currency::eur(),
                NullCalendar::new(),
                BusinessDayConvention::Unadjusted,
                false,
                Actual360::new(),
                curves[i].handle(),
                settings.clone(),
            ))
        });
        for (i, index) in indexes.iter().enumerate() {
            for fixing_date in dates(i == 1).iter().take(dates(i == 1).len() - 1) {
                if *fixing_date < date(row[0]) {
                    index
                        .add_fixing(*fixing_date, if i == 0 { 0.021 } else { 0.031 })
                        .unwrap();
                }
            }
        }
        Self {
            settings,
            indexes,
            quotes,
            curves,
        }
    }
    pub(super) fn build(
        &self,
        row: &[f64],
        nominal: f64,
        spread1: f64,
        spread2: f64,
        first: Schedule,
    ) -> QlResult<FloatFloatSwap> {
        let mut swap = FloatFloatSwap::new(
            nominal,
            first,
            self.indexes[0].clone(),
            spread1,
            Actual360::new(),
            schedule(true),
            self.indexes[1].clone(),
            spread2,
            Actual365Fixed::new(),
            if row[10] != 0.0 {
                Some(BusinessDayConvention::Following)
            } else {
                None
            },
            self.settings.clone(),
        )?;
        swap.base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                self.curves[2].handle(),
                Some(row[3] != 0.0),
                Some(date(row[1])),
                Some(date(row[2])),
                self.settings.clone(),
            )));
        Ok(swap)
    }
    pub(super) fn standard(&self, row: &[f64]) -> FloatFloatSwap {
        self.build(row, 1_000_000.0, row[8], row[9], schedule(false))
            .unwrap()
    }
}
