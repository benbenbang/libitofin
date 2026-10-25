use libitofin::handle::Handle;
use libitofin::indexes::ibor::GbpLibor;
use libitofin::indexes::inflation::UkRpi;
use libitofin::indexes::inflationindex::CpiInterpolationType;
use libitofin::indexes::{Index, ZeroInflationIndex};
use libitofin::instrument::Instrument;
use libitofin::instruments::{CpiSwap, SwapType};
use libitofin::interestrate::Compounding;
use libitofin::math::interpolations::linear::Linear;
use libitofin::pricingengines::DiscountingSwapEngine;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared, shared_mut};
use libitofin::termstructures::inflation::{
    inflationtermstructure::ZeroInflationTermStructure,
    interpolatedzeroinflationcurve::InterpolatedZeroInflationCurve,
};
use libitofin::termstructures::yields::FlatForward;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::{
    businessdayconvention::BusinessDayConvention as Bdc,
    calendars::nullcalendar::NullCalendar,
    date::{Date, Month},
    daycounters::actual365fixed::Actual365Fixed,
    frequency::Frequency,
    period::Period,
    schedule::Schedule,
    timeunit::TimeUnit,
};

pub fn date(day: i32, month: i32, year: i32) -> Date {
    Date::new(day, Month::from_ordinal(month), year)
}
pub fn schedule() -> Schedule {
    Schedule::with_metadata(
        (0..=6)
            .map(|i| date(15, if i % 2 == 0 { 1 } else { 7 }, 2025 + i / 2))
            .collect(),
        NullCalendar::new(),
        Bdc::Unadjusted,
        None,
        None,
        None,
        None,
        vec![],
    )
}
pub struct Market {
    pub settings: Shared<Settings<Date>>,
    pub index: Shared<ZeroInflationIndex>,
    pub floating: Shared<libitofin::indexes::IborIndex>,
    pub discount: Handle<dyn YieldTermStructure>,
    pub quote: Shared<SimpleQuote>,
}
impl Market {
    pub fn new() -> Self {
        let today = date(15, 6, 2025);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        let dc = Actual365Fixed::new();
        let z = InterpolatedZeroInflationCurve::new(
            today,
            vec![date(1, 1, 2025), date(1, 1, 2035)],
            vec![0.025; 2],
            Frequency::Monthly,
            dc.clone(),
            Linear,
            None,
        )
        .unwrap();
        let index = shared(
            UkRpi::new(settings.clone()).with_term_structure(Handle::new(
                shared(z) as Shared<dyn ZeroInflationTermStructure>
            )),
        );
        for (year, month, value) in [
            (2024, 10, 99.5),
            (2024, 11, 99.8),
            (2024, 12, 100.0),
            (2025, 1, 100.2),
            (2025, 2, 100.4),
            (2025, 3, 100.6),
            (2025, 4, 100.8),
            (2025, 5, 101.0),
        ] {
            index.add_fixing(date(1, month, year), value).unwrap();
        }
        let quote = shared(SimpleQuote::new(0.04));
        let discount = Handle::new(shared(FlatForward::new(
            today,
            Handle::new(quote.clone() as Shared<dyn Quote>),
            dc,
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let floating = shared(
            GbpLibor::new(
                Period::new(6, TimeUnit::Months),
                discount.clone(),
                settings.clone(),
            )
            .unwrap(),
        );
        floating.add_fixing(date(15, 1, 2025), 0.042).unwrap();
        Self {
            settings,
            index,
            floating,
            discount,
            quote,
        }
    }
    pub fn build(
        &self,
        side: SwapType,
        growth: bool,
        inflation_nominal: f64,
        interpolation: CpiInterpolationType,
        spread: f64,
    ) -> CpiSwap {
        let mut swap = CpiSwap::new(
            side,
            100000.0,
            inflation_nominal,
            growth,
            spread,
            Actual365Fixed::new(),
            schedule(),
            Bdc::Unadjusted,
            self.floating.clone(),
            0.02,
            100.0,
            Actual365Fixed::new(),
            schedule(),
            Bdc::Unadjusted,
            Period::new(3, TimeUnit::Months),
            self.index.clone(),
            interpolation,
            self.settings.clone(),
        )
        .unwrap();
        swap.base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                self.discount.clone(),
                None,
                None,
                None,
                self.settings.clone(),
            )));
        swap
    }
}
