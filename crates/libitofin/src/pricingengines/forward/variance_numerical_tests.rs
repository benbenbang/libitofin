use super::ReplicatingVarianceSwapEngine;
use super::test_market::{CALLS, Market, PUTS, close, curve, quote_handle, today};
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::VarianceSwap;
use crate::math::matrix::Matrix;
use crate::position::Position;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, shared, shared_mut};
use crate::termstructures::volatility::{
    BlackConstantVol, BlackVarianceSurface, BlackVolTermStructure,
};
use crate::time::daycounters::actual360::Actual360;
use crate::time::daycounters::actual365fixed::Actual365Fixed;

#[test]
fn variance_replication_flat_strip_tends_to_volatility_squared_without_dividends() {
    let market = Market::new();
    market.rate.set_value(0.0);
    let calls: Vec<_> = (4000..=8000).map(|i| f64::from(i) * 0.025).collect();
    let puts: Vec<_> = (1..=4000).map(|i| f64::from(i) * 0.025).collect();
    let mut swap = market.swap(Position::Long, 0.04, &calls, &puts, 0.0125);
    close(swap.variance().unwrap(), 0.04, 1e-7);
    market.vol.set_value(0.0);
    close(swap.variance().unwrap(), 0.0, 1e-15);
}

#[test]
fn variance_replication_nonzero_dividend_limit_includes_boundary_term_and_signed_output() {
    let market = Market::new();
    market.dividend.set_value(0.03);
    let calls: Vec<_> = (4000..=8000).map(|i| f64::from(i) * 0.025).collect();
    let puts: Vec<_> = (1..=4000).map(|i| f64::from(i) * 0.025).collect();
    let mut swap = market.swap(Position::Long, 0.04, &calls, &puts, 0.0125);
    let time: f64 = 90.0 / 365.0;
    let expected =
        0.04 + 2.0 * 0.03 + 2.0 / time * (((0.05 - 0.03) * time).exp() - (0.05 * time).exp());
    close(swap.variance().unwrap(), expected, 1e-7);
    assert!((expected - 0.04).abs() > 1e-4);
    let calls: Vec<_> = (500..=4095).map(|i| f64::from(i) * 0.1).collect();
    let puts: Vec<_> = (1..=500).map(|i| f64::from(i) * 0.1).collect();
    market.dividend.set_value(0.2);
    let mut signed = market.swap(Position::Long, 0.04, &calls, &puts, 0.05);
    assert!(signed.variance().unwrap() < 0.0);
    assert!(signed.npv().unwrap() < 0.0);
}

#[test]
fn variance_replication_literature_smile_keeps_original_native_tolerance() {
    let market = Market::new();
    let strikes: Vec<_> = (50..=135).step_by(5).map(f64::from).collect();
    let mut vols = Matrix::with_size(strikes.len(), 1);
    for i in 0..strikes.len() {
        vols[(i, 0)] = 0.30 - 0.01 * i as f64;
    }
    let volatility = shared(
        BlackVarianceSurface::new(
            today(),
            None,
            &[today() + 90],
            strikes,
            &vols,
            Actual365Fixed::new(),
        )
        .unwrap(),
    ) as Shared<dyn BlackVolTermStructure>;
    let process = shared(GeneralizedBlackScholesProcess::new(
        quote_handle(&market.spot),
        Handle::new(curve(today(), &market.dividend, Actual365Fixed::new())),
        Handle::new(curve(today(), &market.rate, Actual365Fixed::new())),
        Handle::new(volatility),
    ));
    let mut swap = VarianceSwap::new(
        Position::Long,
        0.04,
        50000.0,
        today(),
        today() + 90,
        market.settings.clone(),
    )
    .unwrap();
    swap.base_mut().set_pricing_engine(shared_mut(
        ReplicatingVarianceSwapEngine::new(process, 5.0, CALLS, PUTS).unwrap(),
    ));
    close(swap.variance().unwrap(), 0.04189, 1e-4);
    assert_eq!(swap.option_weights().unwrap().len(), 19);
}

#[test]
fn variance_replication_rejects_forward_seasoned_and_mismatched_references() {
    let market = Market::new();
    for start in [today() - 1, today() + 1] {
        let mut swap = VarianceSwap::new(
            Position::Long,
            0.04,
            50000.0,
            start,
            today() + 90,
            market.settings.clone(),
        )
        .unwrap();
        swap.base_mut().set_pricing_engine(shared_mut(
            ReplicatingVarianceSwapEngine::new(market.process.clone(), 5.0, CALLS, PUTS).unwrap(),
        ));
        assert!(swap.npv().unwrap_err().message().contains("spot start"));
    }
    for side in 0..3 {
        let reference = |selected| {
            if side == selected {
                today() - 1
            } else {
                today()
            }
        };
        let process = shared(GeneralizedBlackScholesProcess::new(
            quote_handle(&market.spot),
            Handle::new(curve(reference(0), &market.dividend, Actual365Fixed::new())),
            Handle::new(curve(reference(1), &market.rate, Actual365Fixed::new())),
            Handle::new(shared(BlackConstantVol::new(
                reference(2),
                None,
                0.2,
                Actual365Fixed::new(),
            )) as Shared<dyn BlackVolTermStructure>),
        ));
        let mut swap = VarianceSwap::new(
            Position::Long,
            0.04,
            50000.0,
            today(),
            today() + 90,
            market.settings.clone(),
        )
        .unwrap();
        swap.base_mut().set_pricing_engine(shared_mut(
            ReplicatingVarianceSwapEngine::new(process, 5.0, CALLS, PUTS).unwrap(),
        ));
        assert!(
            swap.npv()
                .unwrap_err()
                .message()
                .contains("reference dates")
        );
    }
}

#[test]
fn variance_replication_accepts_distinct_daycounters_and_existing_negative_raw_vol() {
    let market = Market::new();
    let process = shared(GeneralizedBlackScholesProcess::new(
        quote_handle(&market.spot),
        Handle::new(curve(today(), &market.dividend, Actual360::new())),
        Handle::new(curve(today(), &market.rate, Actual365Fixed::new())),
        Handle::new(
            shared(BlackConstantVol::new(today(), None, 0.2, Actual360::new()))
                as Shared<dyn BlackVolTermStructure>,
        ),
    ));
    let mut swap = VarianceSwap::new(
        Position::Long,
        0.04,
        50000.0,
        today(),
        today() + 90,
        market.settings.clone(),
    )
    .unwrap();
    swap.base_mut().set_pricing_engine(shared_mut(
        ReplicatingVarianceSwapEngine::new(process, 5.0, CALLS, PUTS).unwrap(),
    ));
    assert!(swap.variance().unwrap().is_finite());
    let mut swap = market.swap(Position::Long, 0.04, CALLS, PUTS, 5.0);
    let original = swap.variance().unwrap();
    market.vol.set_value(-0.2);
    close(swap.variance().unwrap(), original, 0.0);
}
