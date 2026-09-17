//! Boundary tests use public QuantLib/core oracle conventions, without Python.
use crate::boundary::*;
use crate::stripper_api::*;
use crate::volcube_api::*;
use libitofin::currency::Currency;
use libitofin::handle::Handle;
use libitofin::indexes::{Euribor, SwapIndex};
use libitofin::interestrate::Compounding;
use libitofin::math::matrix::Matrix;
use libitofin::quotes::{SimpleQuote, make_quote_handle};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, SharedMut, shared, shared_mut};
use libitofin::termstructures::{
    volatility::*, yields::FlatForward, yieldtermstructure::YieldTermStructure,
};
use libitofin::time::{
    businessdayconvention::BusinessDayConvention as BDC,
    calendars::target::Target,
    date::{Date, Month},
    daycounters::{
        actual365fixed::Actual365Fixed,
        thirty360::{Convention, Thirty360},
    },
    frequency::Frequency,
    period::Period,
    timeunit::TimeUnit,
};

#[test]
fn optionlet_boundary_round_trips_caps_and_rejects_bad_buffers() {
    use libitofin::{
        instrument::Instrument,
        instruments::{CapFloorType, MakeCapFloor},
        pricingengine::PricingEngine,
        pricingengines::BlackCapFloorEngine,
    };
    let mut c = Context::new();
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(Date::new(28, Month::October, 2013));
    let curve = Handle::new(shared(FlatForward::moving_with_rate(
        0,
        Target::new(),
        0.04,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
        settings.clone(),
    )) as Shared<dyn YieldTermStructure>);
    let tenors: Vec<_> = (1..=10).map(|n| Period::new(n, TimeUnit::Years)).collect();
    let strikes: Vec<_> = (1..=10).map(|n| f64::from(n) / 100.).collect();
    let surface = shared(
        CapFloorTermVolSurface::moving_from_matrix(
            0,
            Target::new(),
            BDC::Following,
            tenors.clone(),
            strikes.clone(),
            &Matrix::filled(10, 10, 0.18),
            Actual365Fixed::new(),
            settings.clone(),
        )
        .unwrap(),
    );
    let index = shared(Euribor::six_months(curve.clone(), settings.clone()));
    let surface_id = c.insert(surface).unwrap();
    let index_id = c
        .insert(crate::indexes_api::NativeIbor::builtin(index.clone()))
        .unwrap();
    let settings_id = c.insert(settings.clone()).unwrap();
    let mut stripper = 0;
    let mut adapter = 0;
    unsafe {
        assert_eq!(
            itofin_optionlet_stripper_new(
                &mut c,
                surface_id,
                index_id,
                0,
                1e-6,
                100,
                0.,
                0,
                0,
                0,
                0,
                &mut stripper,
                std::ptr::null_mut()
            ),
            0
        );
        let mut n = 0;
        assert_eq!(
            itofin_optionlet_stripper_rates(
                &mut c,
                stripper,
                std::ptr::null_mut(),
                0,
                &mut n,
                std::ptr::null_mut()
            ),
            0
        );
        assert!(n > 0);
        let mut rates = vec![0.; n];
        assert_eq!(
            itofin_optionlet_stripper_rates(
                &mut c,
                stripper,
                rates.as_mut_ptr(),
                n - 1,
                &mut n,
                std::ptr::null_mut()
            ),
            INVALID_ARGUMENT
        );
        assert_eq!(
            itofin_optionlet_stripper_rates(
                &mut c,
                stripper,
                rates.as_mut_ptr(),
                n,
                &mut n,
                std::ptr::null_mut()
            ),
            0
        );
        let mut switch = 0.;
        assert_eq!(
            itofin_optionlet_stripper_switch_strike(
                &mut c,
                stripper,
                &mut switch,
                std::ptr::null_mut()
            ),
            0
        );
        assert!((switch - rates.iter().sum::<f64>() / n as f64).abs() < 1e-12);
        assert_eq!(
            itofin_stripped_optionlet_adapter_new(
                &mut c,
                stripper,
                settings_id,
                &mut adapter,
                std::ptr::null_mut()
            ),
            0
        );
        assert_eq!(
            itofin_handle_release(&mut c, stripper, std::ptr::null_mut()),
            0
        );
        assert_eq!(
            itofin_handle_release(&mut c, surface_id, std::ptr::null_mut()),
            0
        );
    }
    let vol = c
        .get::<Handle<dyn OptionletVolatilityStructure>>(adapter)
        .unwrap();
    vol.current_link().unwrap().enable_extrapolation();
    let stripped = shared_mut(BlackCapFloorEngine::new(curve.clone(), vol, None).unwrap())
        as SharedMut<dyn PricingEngine>;
    let flat = shared_mut(
        BlackCapFloorEngine::with_flat_vol(
            curve,
            make_quote_handle(0.18).handle(),
            Actual365Fixed::new(),
            0.,
            settings.clone(),
        )
        .unwrap(),
    ) as SharedMut<dyn PricingEngine>;
    for tenor in tenors {
        for &strike in &strikes {
            let mut a = MakeCapFloor::new(
                CapFloorType::Cap,
                tenor,
                index.clone(),
                strike,
                Period::new(0, TimeUnit::Days),
                settings.clone(),
            )
            .with_pricing_engine(stripped.clone())
            .build()
            .unwrap();
            let mut b = MakeCapFloor::new(
                CapFloorType::Cap,
                tenor,
                index.clone(),
                strike,
                Period::new(0, TimeUnit::Days),
                settings.clone(),
            )
            .with_pricing_engine(flat.clone())
            .build()
            .unwrap();
            assert!((a.npv().unwrap() - b.npv().unwrap()).abs() < 2.5e-8);
        }
    }
}

#[test]
fn interpolated_cube_boundary_preserves_node_order_and_quote_updates() {
    let mut c = Context::new();
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(Date::new(15, Month::June, 2026));
    let curve = Handle::new(shared(FlatForward::moving_with_rate(
        0,
        Target::new(),
        0.05,
        Actual365Fixed::new(),
        Compounding::Continuous,
        Frequency::Annual,
        settings.clone(),
    )) as Shared<dyn YieldTermStructure>);
    let ibor = shared(Euribor::six_months(curve, settings.clone()));
    let mk_index = |n| {
        shared(SwapIndex::new(
            "test".into(),
            Period::new(n, TimeUnit::Years),
            2,
            Currency::eur(),
            Target::new(),
            Period::new(1, TimeUnit::Years),
            BDC::ModifiedFollowing,
            Thirty360::with_convention(Convention::BondBasis),
            ibor.clone(),
            settings.clone(),
        ))
    };
    let index = c.insert(mk_index(2)).unwrap();
    let short_index = c.insert(mk_index(1)).unwrap();
    let atm = Handle::new(shared(ConstantSwaptionVolatility::moving(
        0,
        Target::new(),
        BDC::ModifiedFollowing,
        0.2,
        Actual365Fixed::new(),
        VolatilityType::ShiftedLognormal,
        0.,
        settings.clone(),
    )) as Shared<dyn SwaptionVolatilityStructure>);
    let atm_id = c.insert(atm).unwrap();
    let settings_id = c.insert(settings).unwrap();
    let options = [1, 5, 10];
    let swaps = [2, 5, 10];
    let units = [3, 3, 3];
    let strikes = [-0.01, 0., 0.01];
    let mut live = vec![];
    let mut ids = vec![];
    for node in 0..9 {
        for k in 0..3 {
            let q = shared(SimpleQuote::new(if k == 1 {
                0.
            } else {
                0.001 * (node + 1) as f64 * (k + 1) as f64
            }));
            ids.push(c.insert(q.clone()).unwrap());
            live.push(q);
        }
    }
    let cfg = ItofinVolCubeConfig {
        atm: atm_id,
        index,
        short_index,
        settings: settings_id,
        option_lengths: options.as_ptr(),
        option_units: units.as_ptr(),
        options: 3,
        swap_lengths: swaps.as_ptr(),
        swap_units: units.as_ptr(),
        swaps: 3,
        strike_spreads: strikes.as_ptr(),
        strikes: 3,
        vol_spreads: ids.as_ptr(),
        vol_count: ids.len(),
        guesses: std::ptr::null(),
        guess_count: 0,
        fixed: std::ptr::null(),
        atm_calibrated: 0,
        vega_weighted: 0,
        use_max_error: 0,
        max_guesses: 50,
        cutoff_strike: 0.0001,
    };
    let mut result = ItofinVolCubeHandles {
        surface: 0,
        cube: 0,
    };
    unsafe {
        assert_eq!(
            itofin_swaption_vol_cube_new(&mut c, 0, &cfg, &mut result, std::ptr::null_mut()),
            0
        );
    }
    let base = c
        .get::<Handle<dyn SwaptionVolatilityStructure>>(result.surface)
        .unwrap()
        .current_link()
        .unwrap();
    for (i, &o) in options.iter().enumerate() {
        for (j, &s) in swaps.iter().enumerate() {
            let mut atm = 0.;
            unsafe {
                assert_eq!(
                    itofin_swaption_vol_cube_atm(
                        &mut c,
                        result.cube,
                        o,
                        3,
                        s,
                        3,
                        &mut atm,
                        std::ptr::null_mut()
                    ),
                    0
                );
            }
            for (k, &spread) in strikes.iter().enumerate() {
                let expected = if k == 1 {
                    0.2
                } else {
                    0.2 + 0.001 * (i * 3 + j + 1) as f64 * (k + 1) as f64
                };
                let actual = base
                    .volatility_tenors(
                        Period::new(o, TimeUnit::Years),
                        Period::new(s, TimeUnit::Years),
                        atm + spread,
                        true,
                    )
                    .unwrap();
                assert!(
                    (actual - expected).abs() < 1e-12,
                    "node {i}/{j}/{k}: {actual} vs {expected}"
                );
            }
        }
    }
    live[14].set_value(0.07);
    let mut atm = 0.;
    unsafe {
        assert_eq!(
            itofin_swaption_vol_cube_atm(
                &mut c,
                result.cube,
                5,
                3,
                5,
                3,
                &mut atm,
                std::ptr::null_mut()
            ),
            0
        );
    }
    let actual = base
        .volatility_tenors(
            Period::new(5, TimeUnit::Years),
            Period::new(5, TimeUnit::Years),
            atm + 0.01,
            true,
        )
        .unwrap();
    assert!((actual - 0.27).abs() < 1e-12);

    // SABR's beta=1, nu=0 limit is a constant lognormal smile. This also
    // discriminates the four-column parameter order and fixed-flag transport.
    let zero = shared(SimpleQuote::new(0.0));
    let zero_id = c.insert(zero).unwrap();
    let zeros = vec![zero_id; 27];
    let mut guesses = vec![];
    for _ in 0..9 {
        for value in [0.2, 1.0, 0.0, 0.0] {
            guesses.push(c.insert(shared(SimpleQuote::new(value))).unwrap());
        }
    }
    let fixed = [1, 1, 1, 1];
    let cfg = ItofinVolCubeConfig {
        vol_spreads: zeros.as_ptr(),
        guesses: guesses.as_ptr(),
        guess_count: guesses.len(),
        fixed: fixed.as_ptr(),
        ..cfg
    };
    unsafe {
        assert_eq!(
            itofin_swaption_vol_cube_new(&mut c, 1, &cfg, &mut result, std::ptr::null_mut()),
            0
        );
    }
    let sabr = c
        .get::<Handle<dyn SwaptionVolatilityStructure>>(result.surface)
        .unwrap()
        .current_link()
        .unwrap();
    unsafe {
        assert_eq!(
            itofin_swaption_vol_cube_atm(
                &mut c,
                result.cube,
                5,
                3,
                5,
                3,
                &mut atm,
                std::ptr::null_mut()
            ),
            0
        );
    }
    for spread in strikes {
        let vol = sabr
            .volatility_tenors(
                Period::new(5, TimeUnit::Years),
                Period::new(5, TimeUnit::Years),
                atm + spread,
                true,
            )
            .unwrap();
        assert!((vol - 0.2).abs() < 1e-10);
    }
}
