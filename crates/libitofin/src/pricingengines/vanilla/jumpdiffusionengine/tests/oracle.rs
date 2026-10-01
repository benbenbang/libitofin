use super::*;

#[test]
fn independent_quantlib_price_and_greek_fixtures() {
    let cases = [
        (
            OptionType::Call,
            1.0,
            -0.1,
            0.3,
            360,
            [
                14.586956184779012,
                0.6175760317781934,
                0.012393297925041149,
                -8.239240639178727,
                24.78659585008232,
                47.17064699304032,
                -61.75760317781934,
            ],
        ),
        (
            OptionType::Put,
            1.0,
            -0.1,
            0.3,
            360,
            [
                11.690031304174994,
                -0.3626226415285577,
                0.012393297925041173,
                -5.443490863290569,
                24.78659585008237,
                -47.95229545703078,
                36.26226415285577,
            ],
        ),
        (
            OptionType::Call,
            2.5,
            0.2,
            0.3,
            360,
            [
                26.799237829294686,
                0.5881096362288443,
                0.005316638728967413,
                -12.993719078787757,
                10.633277457934824,
                32.01172579358976,
                -58.810963622884444,
            ],
        ),
        (
            OptionType::Put,
            5.0,
            -0.2,
            0.01,
            1800,
            [
                27.894319309407567,
                -0.22001528452903665,
                0.0027058771720590165,
                -0.969877184352032,
                27.05877172059017,
                -249.47923881155612,
                110.0076422645183,
            ],
        ),
        (
            OptionType::Call,
            0.0,
            -0.1,
            0.3,
            360,
            [
                9.227005508154061,
                0.5868511461347649,
                0.01895057875500871,
                -5.089318913998339,
                37.90115751001742,
                49.45810910532239,
                -58.68511461347645,
            ],
        ),
        (
            OptionType::Put,
            0.0,
            -0.1,
            0.3,
            360,
            [
                6.330080627549918,
                -0.39334752717199073,
                0.01895057875500871,
                -2.2935691381082712,
                37.90115751001742,
                -45.664833344749,
                39.33475271719908,
            ],
        ),
        (
            OptionType::Call,
            2.0,
            0.08,
            0.0,
            360,
            [
                10.375222262501174,
                0.5797189785696198,
                0.01656025464287714,
                -5.613586080223574,
                33.12050928575426,
                47.59667559446084,
                -57.97189785696202,
            ],
        ),
        (
            OptionType::Call,
            8.0,
            0.0,
            0.0,
            360,
            [
                9.227005508153308,
                0.586851146134717,
                0.01895057875500715,
                -5.089318913976321,
                37.90115751001434,
                49.458109105318364,
                -58.68511461347167,
            ],
        ),
        (
            OptionType::Call,
            800.0,
            0.0,
            0.02,
            360,
            [
                24.248655648833182,
                0.6241657093580795,
                0.006132118812473648,
                -11.696711504971956,
                12.264237624947306,
                38.167915286974676,
                -62.41657093580786,
            ],
        ),
        (
            OptionType::Put,
            800.0,
            0.0,
            0.02,
            360,
            [
                21.351730768751178,
                -0.35603296377505017,
                0.006132118812473648,
                -8.900961731202003,
                12.264237624947306,
                -56.955027146256214,
                35.60329637750505,
            ],
        ),
    ];
    for (kind, intensity, mean, jump_vol, days, expected) in cases {
        let m = JumpMarket::new(intensity, mean, jump_vol);
        let mut option = m.market.option(kind, 100.0, today() + days);
        option
            .base_mut()
            .set_pricing_engine(shared_mut(
                JumpDiffusionEngine::new(Shared::clone(&m.process), 1e-12, 4096).unwrap(),
            ) as crate::shared::SharedMut<dyn PricingEngine>);
        for (actual, expected) in values(&mut option).into_iter().zip(expected) {
            close(actual, expected, 1e-8);
        }
    }
}

#[test]
fn different_reference_dates_and_day_counters_match_quantlib() {
    use crate::interestrate::Compounding;
    use crate::processes::GeneralizedBlackScholesProcess;
    use crate::termstructures::volatility::BlackConstantVol;
    use crate::termstructures::yields::FlatForward;
    use crate::time::daycounters::{actual360::Actual360, actual365fixed::Actual365Fixed};
    use crate::time::frequency::Frequency;
    let m = JumpMarket::new(1.0, -0.1, 0.3);
    let rate = shared(FlatForward::new(
        today(),
        quote_handle(&m.market.r_rate),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let dividend = shared(FlatForward::new(
        today(),
        quote_handle(&m.market.q_rate),
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ));
    let vol = shared(BlackConstantVol::with_quote(
        today() - 30,
        None,
        quote_handle(&m.market.vol),
        Actual360::new(),
    ));
    let black = shared(GeneralizedBlackScholesProcess::new(
        quote_handle(&m.market.spot),
        Handle::new(dividend),
        Handle::new(rate),
        Handle::new(vol),
    ));
    let process = shared(
        Merton76Process::from_black_scholes(
            black,
            quote_handle(&m.intensity),
            quote_handle(&m.mean),
            quote_handle(&m.jump_vol),
        )
        .unwrap(),
    );
    let mut option = m.market.option(OptionType::Call, 100.0, today() + 360);
    option.base_mut().set_pricing_engine(shared_mut(
        JumpDiffusionEngine::new(process, 1e-12, 4096).unwrap(),
    ) as crate::shared::SharedMut<dyn PricingEngine>);
    let expected = [
        14.430782838565015,
        0.6108593616328124,
        0.012360216923478787,
        -8.011973418209974,
        24.72043384695758,
        46.65515332471622,
        -60.2491425172089,
    ];
    for (actual, expected) in values(&mut option).into_iter().zip(expected) {
        close(actual, expected, 1e-8);
    }
}

#[test]
fn zero_diffusion_greeks_match_independent_price_derivatives() {
    for (intensity, expected) in [
        (
            1.0,
            [
                11.711311771578648,
                0.727923385342851,
                0.006742450870215559,
                -7.938350512148121,
                0.0,
                61.0810267626638,
                -72.79233853430877,
            ],
        ),
        (
            0.0,
            [
                2.896924880604118,
                0.9801986733075789,
                0.0,
                -2.7957497758990253,
                0.0,
                95.12294245003734,
                -98.01986733063946,
            ],
        ),
    ] {
        let m = JumpMarket::new(intensity, -0.1, 0.3);
        m.market.vol.set_value(0.0);
        for (actual, expected) in values(&mut m.option(OptionType::Call, 100.0, 1e-12, 4096))
            .into_iter()
            .zip(expected)
        {
            close(actual, expected, 1e-6);
        }
    }
}

#[test]
fn engine_retains_market_inputs_after_source_owners_drop() {
    let m = JumpMarket::new(1.0, -0.1, 0.3);
    let mut option = m.option(OptionType::Call, 100.0, 1e-12, 4096);
    let quote = Shared::clone(&m.intensity);
    let initial = option.npv().unwrap();
    drop(m);
    close(option.npv().unwrap(), initial, 0.0);
    quote.set_value(2.0);
    assert!((option.npv().unwrap() - initial).abs() > 1e-3);
}

#[test]
fn rare_large_jumps_do_not_stop_at_zero_early_payoffs() {
    for diffusion in [0.0, 0.001] {
        let m = JumpMarket::new(0.01, 1.0, 0.0);
        m.market.vol.set_value(diffusion);
        let mut option = m.option(OptionType::Call, 1000.0, 1e-12, 1000);
        let expected = [
            0.00016415889568634478,
            0.000003215128392864,
            0.0,
            -0.0004885479016182441,
            0.0,
            0.0001573539436001241,
            -0.0003215128392863766,
        ];
        for (actual, expected) in values(&mut option).into_iter().zip(expected) {
            close(actual, expected, 1e-10);
        }
        assert!(option.npv().unwrap() > 0.0);
    }
}
