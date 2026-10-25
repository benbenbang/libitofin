#[path = "fixtures/cpi_swap/support.rs"]
mod support;
use libitofin::indexes::Index;
use libitofin::indexes::inflationindex::CpiInterpolationType;
use libitofin::instrument::Instrument;
use libitofin::instruments::SwapType;

fn close(value: f64, expected: f64, tolerance: f64) {
    assert!(
        (value - expected).abs() <= tolerance,
        "{value:.17} != {expected:.17}"
    );
}
#[test]
fn signed_leg_totals_spread_bps_and_true_par_spread() {
    for interpolation in [CpiInterpolationType::Flat, CpiInterpolationType::Linear] {
        for growth in [false, true] {
            for nominal in [100000.0, 90000.0] {
                let m = support::Market::new();
                let mut swap = m.build(SwapType::Payer, growth, nominal, interpolation, 0.001);
                let npv = swap.npv().unwrap();
                close(
                    swap.fixed_leg_npv().unwrap() + swap.float_leg_npv().unwrap(),
                    npv,
                    2e-9,
                );
                assert!(swap.float_leg_bps().unwrap() < 0.0);
                let fair = swap.fair_spread().unwrap();
                let mut par = m.build(SwapType::Payer, growth, nominal, interpolation, fair);
                close(par.npv().unwrap(), 0.0, 2e-9);
                let mut receiver =
                    m.build(SwapType::Receiver, growth, nominal, interpolation, 0.001);
                close(receiver.npv().unwrap(), -npv, 2e-9);
                close(receiver.fair_spread().unwrap(), fair, 2e-13);
                let mut bumped = m.build(SwapType::Payer, growth, nominal, interpolation, 0.0011);
                close(
                    bumped.npv().unwrap() - npv,
                    swap.float_leg_bps().unwrap(),
                    2e-9,
                );
                assert_eq!(swap.cpi_leg().len(), 7);
                assert_eq!(
                    swap.float_leg().len(),
                    if growth && nominal == 100000.0 { 6 } else { 7 }
                );
            }
        }
    }
}
#[test]
fn live_quote_freeze_fixing_and_expiry() {
    let m = support::Market::new();
    let mut swap = m.build(
        SwapType::Payer,
        true,
        100000.0,
        CpiInterpolationType::Linear,
        0.001,
    );
    let initial = swap.npv().unwrap();
    swap.base_mut().freeze();
    m.quote.set_value(0.05);
    close(swap.npv().unwrap(), initial, 0.0);
    swap.recalculate().unwrap();
    assert!((swap.npv().unwrap() - initial).abs() > 1.0);
    swap.base_mut().unfreeze();
    m.settings.set_evaluation_date(support::date(16, 1, 2026));
    assert!(swap.npv().is_err());
    m.floating
        .add_fixing(support::date(15, 7, 2025), 0.04)
        .unwrap();
    m.floating
        .add_fixing(support::date(15, 1, 2026), 0.04)
        .unwrap();
    for month in 6..=12 {
        m.index
            .add_fixing(support::date(1, month, 2025), 101.0 + f64::from(month))
            .unwrap();
    }
    assert!(swap.npv().unwrap().is_finite());
    m.settings.set_evaluation_date(support::date(16, 1, 2028));
    close(swap.npv().unwrap(), 0.0, 0.0);
    assert!(swap.fair_spread().is_err());
    assert!(swap.is_expired().unwrap());
}

#[test]
fn terminal_float_nominal_threshold_and_indexed_principal_dependency() {
    let m = support::Market::new();
    for (nominal, flows) in [
        (100000.0 + 1e-6, 6),
        (100000.0 - 1e-6, 6),
        (100000.0 + 1e-4, 7),
        (100000.0 - 1e-4, 7),
    ] {
        let swap = m.build(
            SwapType::Payer,
            true,
            nominal,
            CpiInterpolationType::Flat,
            0.001,
        );
        assert_eq!(swap.float_leg().len(), flows);
    }
    m.settings.set_evaluation_date(support::date(16, 1, 2028));
    let swap = m.build(
        SwapType::Payer,
        true,
        100000.0,
        CpiInterpolationType::Flat,
        0.001,
    );
    let principal = swap.cpi_leg().last().unwrap();
    assert!(principal.as_coupon().is_none());
    assert!(principal.amount().is_err());
}
