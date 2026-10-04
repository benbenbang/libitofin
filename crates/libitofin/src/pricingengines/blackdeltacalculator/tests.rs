use super::*;

fn calculator(option: OptionType, delta_type: DeltaType) -> BlackDeltaCalculator {
    BlackDeltaCalculator::new(option, delta_type, 1.421, 0.997306, 0.992266, 0.1180654).unwrap()
}

#[test]
fn quantlib_published_delta_and_strike_cases() {
    let rows = [
        (
            OptionType::Call,
            DeltaType::Spot,
            1.421,
            0.997306,
            0.992266,
            0.1180654,
            1.608080,
            0.15,
        ),
        (
            OptionType::Call,
            DeltaType::PaSpot,
            1.421,
            0.997306,
            0.992266,
            0.1180654,
            1.600545,
            0.15,
        ),
        (
            OptionType::Call,
            DeltaType::Fwd,
            1.421,
            0.997306,
            0.992266,
            0.1180654,
            1.609029,
            0.15,
        ),
        (
            OptionType::Call,
            DeltaType::PaFwd,
            1.421,
            0.997306,
            0.992266,
            0.1180654,
            1.601550,
            0.15,
        ),
        (
            OptionType::Call,
            DeltaType::Spot,
            122.121,
            0.9695434,
            0.9872347,
            0.0887676,
            119.8031,
            0.67,
        ),
        (
            OptionType::Call,
            DeltaType::PaSpot,
            122.121,
            0.9695434,
            0.9872347,
            0.0887676,
            117.7096,
            0.67,
        ),
        (
            OptionType::Call,
            DeltaType::Fwd,
            122.121,
            0.9695434,
            0.9872347,
            0.0887676,
            120.0592,
            0.67,
        ),
        (
            OptionType::Call,
            DeltaType::PaFwd,
            122.121,
            0.9695434,
            0.9872347,
            0.0887676,
            118.0532,
            0.67,
        ),
        (
            OptionType::Put,
            DeltaType::Spot,
            3.4582,
            0.99979,
            0.9250616,
            0.3199034,
            4.964924,
            -0.821,
        ),
        (
            OptionType::Put,
            DeltaType::PaSpot,
            3.4582,
            0.99979,
            0.9250616,
            0.3199034,
            3.778327,
            -0.821,
        ),
        (
            OptionType::Put,
            DeltaType::Fwd,
            3.4582,
            0.99979,
            0.9250616,
            0.3199034,
            4.51896,
            -0.821,
        ),
        (
            OptionType::Put,
            DeltaType::PaFwd,
            3.4582,
            0.99979,
            0.9250616,
            0.3199034,
            3.65728,
            -0.821,
        ),
        (
            OptionType::Put,
            DeltaType::Spot,
            103.0,
            0.99482,
            0.98508,
            0.07247845,
            97.47,
            -0.25,
        ),
        (
            OptionType::Put,
            DeltaType::PaSpot,
            103.0,
            0.99482,
            0.98508,
            0.07247845,
            97.22,
            -0.25,
        ),
    ];
    for (option, convention, spot, domestic, foreign, deviation, strike, delta) in rows {
        let calc =
            BlackDeltaCalculator::new(option, convention, spot, domestic, foreign, deviation)
                .unwrap();
        assert!((calc.delta_from_strike(strike).unwrap() - delta).abs() < 1e-3);
        assert!((calc.strike_from_delta(delta).unwrap() - strike).abs() < 1e-2);
    }
}

#[test]
fn premium_adjusted_call_selects_high_strike_root() {
    for convention in [DeltaType::PaSpot, DeltaType::PaFwd] {
        let calc = calculator(OptionType::Call, convention);
        let low_strike = 0.5;
        let delta = calc.delta_from_strike(low_strike).unwrap();
        let high_strike = calc.strike_from_delta(delta).unwrap();
        assert!(high_strike > 1.4);
        assert!((calc.delta_from_strike(high_strike).unwrap() - delta).abs() < 1e-10);
        assert!(calc.strike_from_delta(0.9).is_err());
    }
}

#[test]
fn atm_convention_constraints_and_mutators_match_quantlib() {
    let mut calc = calculator(OptionType::Call, DeltaType::Spot);
    assert!(calc.atm_strike(AtmType::PutCall50).is_err());
    assert!(calc.atm_strike(AtmType::Null).is_err());
    let ordinary = calc.atm_strike(AtmType::DeltaNeutral).unwrap();
    calc.set_delta_type(DeltaType::PaSpot);
    assert!(calc.atm_strike(AtmType::PutCall50).is_err());
    assert!(calc.atm_strike(AtmType::DeltaNeutral).unwrap() < ordinary);
    calc.set_delta_type(DeltaType::PaFwd);
    assert!(calc.atm_strike(AtmType::PutCall50).is_err());
    calc.set_delta_type(DeltaType::Fwd);
    assert_eq!(calc.atm_strike(AtmType::PutCall50).unwrap(), ordinary);
    assert_eq!(calc.atm_strike(AtmType::GammaMax).unwrap(), ordinary);
    assert_eq!(calc.atm_strike(AtmType::VegaMax).unwrap(), ordinary);
    calc.set_option_type(OptionType::Put);
    assert!(calc.delta_from_strike(1.6).unwrap() < 0.0);
}

#[test]
fn zero_volatility_and_zero_strike_have_defined_deltas() {
    for convention in [
        DeltaType::Spot,
        DeltaType::Fwd,
        DeltaType::PaSpot,
        DeltaType::PaFwd,
    ] {
        for option in [OptionType::Call, OptionType::Put] {
            let calc = BlackDeltaCalculator::new(option, convention, 1.0, 1.0, 1.0, 0.0).unwrap();
            let sign = option as i32 as Real;
            assert_eq!(calc.delta_from_strike(1.0).unwrap(), sign * 0.5);
            assert!(calc.strike_from_delta(sign * 0.25).is_err());
            let zero_delta = calc.delta_from_strike(0.0).unwrap();
            assert_eq!(
                zero_delta,
                if option == OptionType::Call
                    && matches!(convention, DeltaType::Spot | DeltaType::Fwd)
                {
                    1.0
                } else {
                    0.0
                }
            );
            let large_delta = calc.delta_from_strike(2.0).unwrap();
            assert_eq!(
                large_delta,
                if option == OptionType::Call {
                    0.0
                } else if matches!(convention, DeltaType::Spot | DeltaType::Fwd) {
                    -1.0
                } else {
                    -2.0
                }
            );
        }
    }
}

#[test]
fn invalid_inputs_and_inversion_boundaries_error() {
    for bad in [Real::NAN, Real::INFINITY, -1.0] {
        assert!(
            BlackDeltaCalculator::new(OptionType::Call, DeltaType::Fwd, bad, 1.0, 1.0, 0.1)
                .is_err()
        );
        assert!(
            BlackDeltaCalculator::new(OptionType::Call, DeltaType::Fwd, 1.0, bad, 1.0, 0.1)
                .is_err()
        );
        assert!(
            BlackDeltaCalculator::new(OptionType::Call, DeltaType::Fwd, 1.0, 1.0, bad, 0.1)
                .is_err()
        );
        assert!(
            BlackDeltaCalculator::new(OptionType::Call, DeltaType::Fwd, 1.0, 1.0, 1.0, bad)
                .is_err()
        );
        assert!(
            calculator(OptionType::Call, DeltaType::Fwd)
                .delta_from_strike(bad)
                .is_err()
        );
    }
    let calc = calculator(OptionType::Call, DeltaType::Fwd);
    for bad in [Real::NAN, Real::INFINITY, -0.25, 0.0, 1.0, 1.1] {
        assert!(calc.strike_from_delta(bad).is_err());
    }
}

#[test]
fn compiled_quantlib_1_43_delta_and_strike_oracle() {
    let rows = [
        (
            OptionType::Call,
            DeltaType::Spot,
            0.16013974647836293,
            1.6080794396498383,
        ),
        (
            OptionType::Call,
            DeltaType::Fwd,
            0.16138792065672203,
            1.6090292778757842,
        ),
        (
            OptionType::Call,
            DeltaType::PaSpot,
            0.15067826139118903,
            1.6005703561512619,
        ),
        (
            OptionType::Call,
            DeltaType::PaFwd,
            0.15185269009639454,
            1.6015501875673384,
        ),
        (
            OptionType::Put,
            DeltaType::Spot,
            -0.832126253521637,
            1.260473835686862,
        ),
        (
            OptionType::Put,
            DeltaType::Fwd,
            -0.838612079343278,
            1.2597297558566178,
        ),
        (
            OptionType::Put,
            DeltaType::PaSpot,
            -0.9722560102485015,
            1.2546219991778351,
        ),
        (
            OptionType::Put,
            DeltaType::PaFwd,
            -0.9798340467661911,
            1.2539067336573,
        ),
    ];
    for (option, convention, expected_delta, expected_strike) in rows {
        let calc = calculator(option, convention);
        let delta = calc.delta_from_strike(1.6).unwrap();
        let strike = calc
            .strike_from_delta(option as i32 as Real * 0.15)
            .unwrap();
        assert!(
            (delta - expected_delta).abs() < 1e-14,
            "{option:?} {convention:?}: {delta}"
        );
        assert!(
            (strike - expected_strike).abs() < 1e-10,
            "{option:?} {convention:?}: {strike}"
        );
        assert!((calc.atm_strike(AtmType::Spot).unwrap() - 1.421).abs() < 1e-14);
        assert!((calc.atm_strike(AtmType::Fwd).unwrap() - 1.413818813884605).abs() < 1e-14);
        let neutral = if matches!(convention, DeltaType::Spot | DeltaType::Fwd) {
            1.4237071536606858
        } else {
            1.4039991534456169
        };
        assert!((calc.atm_strike(AtmType::DeltaNeutral).unwrap() - neutral).abs() < 1e-14);
    }
}
