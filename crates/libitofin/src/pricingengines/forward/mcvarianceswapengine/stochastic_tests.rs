use super::*;
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::VarianceSwap;
use crate::pricingengines::forward::test_market::{Market, today};
use crate::shared::shared_mut;
use crate::termstructures::volatility::{LocalVolTermStructure, VolatilityTermStructure};
use crate::termstructures::{TermStructure, TermStructureBase};
use crate::time::businessdayconvention::BusinessDayConvention;
use crate::time::date::Date;
use crate::time::daycounters::actual365fixed::Actual365Fixed;

struct StateVol {
    base: TermStructureBase,
    invalid: bool,
}
impl AsObservable for StateVol {
    fn observable(&self) -> &Observable {
        self.base.observable()
    }
}
impl TermStructure for StateVol {
    fn base(&self) -> &TermStructureBase {
        &self.base
    }
    fn max_date(&self) -> Date {
        Date::max_date()
    }
}
impl VolatilityTermStructure for StateVol {
    fn business_day_convention(&self) -> BusinessDayConvention {
        BusinessDayConvention::Following
    }
    fn min_strike(&self) -> f64 {
        f64::MIN
    }
    fn max_strike(&self) -> f64 {
        f64::MAX
    }
}
impl LocalVolTermStructure for StateVol {
    fn local_vol_impl(&self, _time: f64, state: f64) -> QlResult<f64> {
        Ok(if self.invalid {
            -0.1
        } else {
            0.15 + 0.1 * state / (state + 100.0)
        })
    }
}

pub(super) fn process(market: &Market, invalid: bool) -> Shared<GeneralizedBlackScholesProcess> {
    let local = shared(StateVol {
        base: TermStructureBase::with_reference_date(today(), None, Some(Actual365Fixed::new())),
        invalid,
    });
    shared(GeneralizedBlackScholesProcess::with_local_vol(
        market.process.state_variable(),
        market.process.dividend_yield(),
        market.process.risk_free_rate(),
        market.process.black_volatility(),
        Handle::new(local as Shared<dyn LocalVolTermStructure>),
    ))
}

fn swap(
    market: &Market,
    process: Shared<GeneralizedBlackScholesProcess>,
    position: Position,
    samples: Option<usize>,
    tolerance: Option<f64>,
    maximum: Option<usize>,
    seed: u64,
) -> VarianceSwap {
    let mut swap = VarianceSwap::new(
        position,
        0.04,
        50_000.0,
        today(),
        today() + 90,
        market.settings.clone(),
    )
    .unwrap();
    swap.base_mut().set_pricing_engine(shared_mut(
        MCVarianceSwapEngine::new(process, Some(7), None, samples, tolerance, maximum, seed)
            .unwrap(),
    ));
    swap
}

#[test]
fn state_dependent_local_vol_has_seeded_error_and_signed_cash_error() {
    let market = Market::new();
    let process = process(&market, false);
    let mut long = swap(
        &market,
        process.clone(),
        Position::Long,
        Some(1023),
        None,
        None,
        42,
    );
    let mut short = swap(
        &market,
        process.clone(),
        Position::Short,
        Some(1023),
        None,
        None,
        42,
    );
    let mut other = swap(&market, process, Position::Long, Some(1023), None, None, 43);
    let variance = long.variance().unwrap();
    assert!(long.variance_error().unwrap() > 0.0);
    assert!(long.error_estimate().unwrap() > 0.0);
    assert_eq!(variance, short.variance().unwrap());
    assert_eq!(
        long.error_estimate().unwrap(),
        -short.error_estimate().unwrap()
    );
    assert_eq!(
        long.variance_error().unwrap(),
        short.variance_error().unwrap()
    );
    assert_ne!(variance, other.variance().unwrap());
    long.recalculate().unwrap();
    assert_eq!(variance, long.variance().unwrap());
    market.spot.set_value(110.0);
    assert_ne!(variance, long.variance().unwrap());
    market.rate.set_value(0.08);
    assert!(long.npv().unwrap().is_finite());
    market.dividend.set_value(0.03);
    assert!(long.npv().unwrap().is_finite());
}

#[test]
fn state_dependent_tolerance_batches_and_exhaustion() {
    let market = Market::new();
    let process = process(&market, false);
    let mut result = swap(
        &market,
        process.clone(),
        Position::Long,
        None,
        Some(1e-5),
        Some(16368),
        42,
    );
    assert!(result.variance().unwrap().is_finite());
    assert!(result.variance_error().unwrap() <= 1e-5);
    assert!(result.samples().unwrap() > 1023);
    let mut exhausted = swap(
        &market,
        process,
        Position::Long,
        None,
        Some(1e-15),
        Some(1023),
        42,
    );
    assert!(exhausted.variance().is_err());
    assert!(exhausted.samples().is_err());
    assert!(exhausted.variance_error().is_err());
}

#[test]
fn negative_external_diffusion_is_not_squared_into_a_valid_result() {
    let market = Market::new();
    let mut result = swap(
        &market,
        process(&market, true),
        Position::Long,
        Some(1023),
        None,
        None,
        42,
    );
    let error = result.variance().unwrap_err();
    assert!(error.message().contains("diffusion"));
    assert!(result.samples().is_err());
}
