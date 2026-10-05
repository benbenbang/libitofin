use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::VarianceSwap;
use crate::interestrate::Compounding;
use crate::position::Position;
use crate::pricingengines::ReplicatingVarianceSwapEngine;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::quotes::{Quote, SimpleQuote};
use crate::settings::Settings;
use crate::shared::{Shared, shared, shared_mut};
use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::{Date, Month};
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::frequency::Frequency;

pub(crate) fn today() -> Date {
    Date::new(5, Month::October, 2026)
}
pub(crate) const CALLS: &[f64] = &[100.0, 105.0, 110.0, 115.0, 120.0, 125.0, 130.0, 135.0];
pub(crate) const PUTS: &[f64] = &[
    50.0, 55.0, 60.0, 65.0, 70.0, 75.0, 80.0, 85.0, 90.0, 95.0, 100.0,
];

pub(crate) fn quote_handle(quote: &Shared<SimpleQuote>) -> Handle<dyn Quote> {
    Handle::new(quote.clone() as Shared<dyn Quote>)
}

pub(crate) fn curve(
    reference: Date,
    quote: &Shared<SimpleQuote>,
    day_counter: DayCounter,
) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::new(
        reference,
        quote_handle(quote),
        day_counter,
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

pub(crate) struct Market {
    pub(crate) settings: Shared<Settings<Date>>,
    pub(crate) spot: Shared<SimpleQuote>,
    pub(crate) rate: Shared<SimpleQuote>,
    pub(crate) dividend: Shared<SimpleQuote>,
    pub(crate) vol: Shared<SimpleQuote>,
    pub(crate) process: Shared<GeneralizedBlackScholesProcess>,
}

impl Market {
    pub(crate) fn new() -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        let spot = shared(SimpleQuote::new(100.0));
        let rate = shared(SimpleQuote::new(0.05));
        let dividend = shared(SimpleQuote::new(0.0));
        let vol = shared(SimpleQuote::new(0.2));
        let process = shared(GeneralizedBlackScholesProcess::new(
            quote_handle(&spot),
            Handle::new(curve(today(), &dividend, Actual365Fixed::new())),
            Handle::new(curve(today(), &rate, Actual365Fixed::new())),
            Handle::new(shared(BlackConstantVol::with_quote(
                today(),
                None,
                quote_handle(&vol),
                Actual365Fixed::new(),
            )) as Shared<dyn BlackVolTermStructure>),
        ));
        Self {
            settings,
            spot,
            rate,
            dividend,
            vol,
            process,
        }
    }

    pub(crate) fn swap(
        &self,
        position: Position,
        strike: f64,
        calls: &[f64],
        puts: &[f64],
        dk: f64,
    ) -> VarianceSwap {
        let mut swap = VarianceSwap::new(
            position,
            strike,
            50000.0,
            today(),
            today() + 90,
            self.settings.clone(),
        )
        .unwrap();
        swap.base_mut().set_pricing_engine(shared_mut(
            ReplicatingVarianceSwapEngine::new(self.process.clone(), dk, calls, puts).unwrap(),
        ));
        swap
    }
}

pub(crate) fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        actual.is_finite() && expected.is_finite() && (actual - expected).abs() <= tolerance,
        "{actual} != {expected}, tolerance {tolerance}"
    );
}
