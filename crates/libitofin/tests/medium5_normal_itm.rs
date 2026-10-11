use libitofin::option::OptionType;
use libitofin::pricingengines::blackformula::bachelier_black_formula_asset_itm_probability as itm;

fn close(actual: f64, expected: f64) {
    let tolerance = 3e-13 * expected.abs().max(f64::MIN_POSITIVE);
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17e}, expected {expected:.17e}"
    );
}

#[test]
fn native_and_independent_extreme_references() {
    let mut rows = 0;
    for line in include_str!("fixtures/medium5_normal_itm/reference.tsv")
        .lines()
        .skip(1)
    {
        let row: Vec<_> = line.split('\t').collect();
        let option = match row[1] {
            "Call" => OptionType::Call,
            "Put" => OptionType::Put,
            other => panic!("unexpected option {other}"),
        };
        let numbers: Vec<f64> = row[2..6].iter().map(|s| s.parse().unwrap()).collect();
        close(
            itm(option, numbers[0], numbers[1], numbers[2]).unwrap(),
            numbers[3],
        );
        rows += 1;
    }
    assert_eq!(rows, 32);
}

#[test]
fn positive_volatility_call_put_parity_and_atm() {
    for strike in [-5.0, -0.02, 0.0, 0.02, 5.0] {
        for forward in [-5.0, -0.02, 0.0, 0.02, 5.0] {
            for std_dev in [1e-300, 0.001, 0.2, 10.0, f64::MAX] {
                let call = itm(OptionType::Call, strike, forward, std_dev).unwrap();
                let put = itm(OptionType::Put, strike, forward, std_dev).unwrap();
                assert!((0.0..=1.0).contains(&call));
                assert!((0.0..=1.0).contains(&put));
                close(call + put, 1.0);
                if strike == forward {
                    assert_eq!(call, 0.5);
                    assert_eq!(put, 0.5);
                }
            }
        }
    }
}

#[test]
fn zero_volatility_is_intrinsic_not_probability_or_continuous_limit() {
    assert_eq!(itm(OptionType::Call, -2.0, 3.0, 0.0).unwrap(), 5.0);
    assert_eq!(itm(OptionType::Put, -2.0, 3.0, 0.0).unwrap(), 0.0);
    assert_eq!(itm(OptionType::Put, 3.0, -2.0, -0.0).unwrap(), 5.0);
    assert_eq!(itm(OptionType::Call, 3.0, -2.0, -0.0).unwrap(), 0.0);
    for option in [OptionType::Call, OptionType::Put] {
        assert_eq!(itm(option, -0.01, -0.01, 0.0).unwrap(), 0.0);
        assert_eq!(itm(option, -0.01, -0.01, 1e-300).unwrap(), 0.5);
        assert_eq!(itm(option, -0.0, 0.0, -0.0).unwrap(), 0.0);
    }
    close(itm(OptionType::Call, -2.0, 3.0, 1e-300).unwrap(), 1.0);
}

#[test]
fn non_finite_inputs_and_negative_standard_deviation_fail() {
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        for option in [OptionType::Call, OptionType::Put] {
            for std_dev in [0.0, 0.01] {
                assert!(itm(option, bad, -0.01, std_dev).is_err());
                assert!(itm(option, -0.01, bad, std_dev).is_err());
            }
            assert!(itm(option, -0.01, -0.02, bad).is_err());
        }
    }
    for negative in [-0.01, -f64::MIN_POSITIVE, -f64::from_bits(1)] {
        assert!(itm(OptionType::Call, -0.01, -0.02, negative).is_err());
    }
}

#[test]
fn extreme_finite_distances_have_cdf_endpoints_but_intrinsic_overflow_fails() {
    let maximum = f64::MAX;
    assert!(itm(OptionType::Call, -maximum, maximum, 0.0).is_err());
    assert!(itm(OptionType::Put, maximum, -maximum, 0.0).is_err());
    assert_eq!(itm(OptionType::Put, -maximum, maximum, 0.0).unwrap(), 0.0);
    assert_eq!(itm(OptionType::Call, maximum, -maximum, 0.0).unwrap(), 0.0);
    for std_dev in [1.0, f64::from_bits(1)] {
        assert_eq!(
            itm(OptionType::Call, -maximum, maximum, std_dev).unwrap(),
            1.0
        );
        assert_eq!(
            itm(OptionType::Put, -maximum, maximum, std_dev).unwrap(),
            0.0
        );
        assert_eq!(
            itm(OptionType::Call, maximum, -maximum, std_dev).unwrap(),
            0.0
        );
        assert_eq!(
            itm(OptionType::Put, maximum, -maximum, std_dev).unwrap(),
            1.0
        );
    }
    assert_eq!(itm(OptionType::Call, 0.0, maximum, 0.0).unwrap(), maximum);
}

#[test]
fn adjacent_values_and_translation_do_not_cancel_before_subtraction() {
    let adjacent = f64::from_bits(1.0f64.to_bits() + 1);
    let step = adjacent - 1.0;
    close(
        itm(OptionType::Call, 1.0, adjacent, step).unwrap(),
        itm(OptionType::Call, 0.0, step, step).unwrap(),
    );
    for option in [OptionType::Call, OptionType::Put] {
        close(
            itm(option, -8.0, -7.0, 0.25).unwrap(),
            itm(option, 8.0, 9.0, 0.25).unwrap(),
        );
    }
}
