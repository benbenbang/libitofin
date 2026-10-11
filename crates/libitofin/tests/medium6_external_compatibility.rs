//! Compatibility probes for downstream argument literals and borrowing engines.

use libitofin::errors::QlResult;
use libitofin::instruments::{BondResults, CallabilityType, CallableBondArguments};
use libitofin::patterns::observable::{AsObservable, Observable};
use libitofin::pricingengine::{Arguments, PricingEngine, Results};
use libitofin::time::date::{Date, Month};

struct BorrowingEngine<'a> {
    observable: &'a Observable,
    calls: &'a mut usize,
    args: CallableBondArguments,
    results: BondResults,
}

impl AsObservable for BorrowingEngine<'_> {
    fn observable(&self) -> &Observable {
        self.observable
    }
}

impl PricingEngine for BorrowingEngine<'_> {
    fn arguments_mut(&mut self) -> &mut dyn Arguments {
        &mut self.args
    }

    fn results(&self) -> &dyn Results {
        &self.results
    }

    fn reset(&mut self) {
        self.results.reset();
    }

    fn calculate(&mut self) -> QlResult<()> {
        self.args.validate()?;
        *self.calls += 1;
        self.results.instrument.value = Some(self.args.redemption);
        Ok(())
    }
}

#[test]
fn exhaustive_old_argument_literal_and_borrowing_engine_remain_supported() {
    let observable = Observable::new();
    let mut calls = 0;
    let settlement = Date::new(12, Month::October, 2026);
    let redemption = Date::new(12, Month::October, 2031);
    let args = CallableBondArguments {
        settlement_date: Some(settlement),
        coupon_dates: vec![],
        coupon_amounts: vec![],
        face_amount: 100.0,
        redemption: 107.5,
        redemption_date: Some(redemption),
        callability_prices: vec![99.0],
        callability_types: vec![CallabilityType::Call],
        callability_dates: vec![Date::new(12, Month::October, 2029)],
    };
    {
        let mut engine = BorrowingEngine {
            observable: &observable,
            calls: &mut calls,
            args,
            results: BondResults::default(),
        };
        let protocol: &mut dyn PricingEngine = &mut engine;
        protocol.arguments_mut().validate().unwrap();
        protocol.calculate().unwrap();
        assert_eq!(
            protocol.results().as_instrument_results().unwrap().value,
            Some(107.5)
        );
        protocol.reset();
        assert_eq!(
            protocol.results().as_instrument_results().unwrap().value,
            None
        );
    }
    assert_eq!(calls, 1);
}
