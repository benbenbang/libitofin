use super::test_market::close;
use super::varianceswapstrip::*;
use crate::option::OptionType;

#[test]
fn variance_strip_sorts_deduplicates_and_keeps_two_boundary_options() {
    let strip = VarianceSwapStrip::new(
        5.0,
        &[110.0, 100.0, 105.0, 105.0],
        &[95.0, 90.0, 100.0, 95.0],
    )
    .unwrap();
    let canonical =
        VarianceSwapStrip::new(5.0, &[100.0, 105.0, 110.0], &[100.0, 95.0, 90.0]).unwrap();
    assert_eq!(strip.weights(0.5).unwrap(), canonical.weights(0.5).unwrap());
    let weights = strip.weights(0.5).unwrap();
    assert_eq!(weights.len(), 6);
    assert_eq!(weights[0].option_type, OptionType::Call);
    assert_eq!(weights[0].strike, 100.0);
    assert_eq!(weights[2].strike, 110.0);
    assert_eq!(weights[3].option_type, OptionType::Put);
    assert_eq!(weights[3].strike, 100.0);
    assert_eq!(weights[5].strike, 90.0);
}

#[test]
fn variance_strip_weights_match_independent_log_secant_hand_arithmetic() {
    let strip = VarianceSwapStrip::new(10.0, &[100.0, 110.0], &[100.0, 90.0]).unwrap();
    let weights = strip.weights(1.0).unwrap();
    let call_first = 2.0 * (0.1 - 1.1_f64.ln()) / 10.0;
    let call_next = (2.0 * (0.2 - 1.2_f64.ln()) - 2.0 * (0.1 - 1.1_f64.ln())) / 10.0;
    let put_first = 2.0 * (-0.1 - 0.9_f64.ln()) / 10.0;
    let put_next = (2.0 * (-0.2 - 0.8_f64.ln()) - 2.0 * (-0.1 - 0.9_f64.ln())) / 10.0;
    for (actual, expected) in weights.iter().zip([
        call_first,
        call_next - call_first,
        put_first,
        put_next - put_first,
    ]) {
        close(actual.weight, expected, 1e-17);
    }
    for (long, short) in strip.weights(2.0).unwrap().iter().zip(weights) {
        close(long.weight, short.weight / 2.0, 0.0);
    }
}

#[test]
fn variance_strip_rejects_bad_sizes_strikes_boundaries_and_spacing() {
    let calls = [100.0, 110.0];
    let puts = [100.0, 90.0];
    for dk in [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::MIN_POSITIVE,
    ] {
        assert!(VarianceSwapStrip::new(dk, &calls, &puts).is_err());
    }
    for bad in [
        Vec::new(),
        vec![100.0],
        vec![100.0, 100.0],
        vec![100.0; MAX_VARIANCE_SWAP_STRIKES + 1],
        vec![100.0, 0.0],
        vec![100.0, -1.0],
        vec![100.0, f64::NAN],
        vec![100.0, f64::INFINITY],
        vec![100.0, f64::NEG_INFINITY],
    ] {
        assert!(VarianceSwapStrip::new(5.0, &bad, &puts).is_err());
        assert!(VarianceSwapStrip::new(5.0, &calls, &bad).is_err());
    }
    assert!(VarianceSwapStrip::new(5.0, &[101.0, 110.0], &puts).is_err());
    assert!(VarianceSwapStrip::new(90.0, &calls, &puts).is_err());
    assert!(VarianceSwapStrip::new(91.0, &calls, &puts).is_err());
    assert!(VarianceSwapStrip::new(f64::MAX / 2.0, &[1.0, f64::MAX], &[1.0, 0.5]).is_err());
    let strip = VarianceSwapStrip::new(5.0, &calls, &puts).unwrap();
    for time in [
        0.0,
        -1.0,
        f64::NAN,
        f64::INFINITY,
        f64::NEG_INFINITY,
        f64::from_bits(1),
    ] {
        assert!(strip.weights(time).is_err());
    }
}

#[test]
fn variance_strip_bound_accepts_4096_raw_entries_and_rejects_overflowed_payoff() {
    let mut calls = vec![100.0; MAX_VARIANCE_SWAP_STRIKES];
    calls[0] = 110.0;
    let strip = VarianceSwapStrip::new(5.0, &calls, &[100.0, 90.0]).unwrap();
    assert_eq!(strip.weights(1.0).unwrap().len(), 4);
    let huge = VarianceSwapStrip::new(1.0, &[100.0, 1e12], &[100.0, 90.0]).unwrap();
    assert!(huge.weights(1e-300).is_err());
}
