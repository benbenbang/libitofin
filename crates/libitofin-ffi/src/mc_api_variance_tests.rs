//! Brownian bridge and control variate through the C ABI (#1364), against
//! the core #1362/#1363 oracles on the same fixtures and seeds.
use super::*;
use crate::boundary::itofin_handle_release;
use crate::market_api::itofin_black_scholes_new;
use crate::options_api::{itofin_option_new, itofin_option_set_engine, itofin_option_value};
use libitofin::settings::Settings;
use libitofin::shared::shared;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use std::ptr::null_mut;

const BRIDGE_NPV: f64 = 9.271732701042982;
const CV_NPV: f64 = 0.7231366070567805;

struct Market {
    c: Context,
    settings: u64,
    process: u64,
    today: i32,
}

fn market(american: bool) -> Market {
    let mut c = Context::new();
    let (evaluation, reference, spot, rate, dividend) = if american {
        let reference = Date::new(17, Month::May, 1998);
        (Date::new(15, Month::May, 1998), reference, 36.0, 0.06, 0.0)
    } else {
        let today = Date::new(15, Month::June, 2026);
        (today, today, 100.0, 0.05, 0.02)
    };
    let settings = shared(Settings::new());
    settings.set_evaluation_date(evaluation);
    let settings = c.insert(settings).unwrap();
    let dc = if american {
        c.insert(Actual365Fixed::new())
    } else {
        c.insert(Actual360::new())
    };
    let (dc, today, mut process) = (dc.unwrap(), reference.serial_number(), 0);
    let status = unsafe {
        itofin_black_scholes_new(
            &mut c,
            spot,
            rate,
            dividend,
            0.2,
            today,
            dc,
            &mut process,
            null_mut(),
        )
    };
    assert_eq!(status, 0);
    Market {
        c,
        settings,
        process,
        today,
    }
}

impl Market {
    fn engine(&mut self, kind: i32, cfg: McConfig) -> Result<u64, i32> {
        let mut out = 0;
        match unsafe {
            itofin_mc_engine_new(&mut self.c, self.process, kind, cfg, &mut out, null_mut())
        } {
            0 => Ok(out),
            status => Err(status),
        }
    }

    fn price(&mut self, engine: u64, american: bool, fields: usize) -> [f64; 3] {
        let (kind, strike, earliest, expiry) = if american {
            (
                1,
                32.0,
                self.today,
                Date::new(17, Month::May, 1999).serial_number(),
            )
        } else {
            (0, 100.0, 0, self.today + 360)
        };
        let mut option = 0;
        let mut values = [f64::NAN; 3];
        unsafe {
            let status = itofin_option_new(
                &mut self.c,
                kind,
                strike,
                earliest,
                expiry,
                american as i32,
                self.settings,
                &mut option,
                null_mut(),
            );
            assert_eq!(status, 0);
            assert_eq!(
                itofin_option_set_engine(&mut self.c, option, engine, 2, 0, null_mut()),
                0
            );
            for (field, value) in [0, 7, 8].into_iter().zip(values.iter_mut()).take(fields) {
                assert_eq!(
                    itofin_option_value(&mut self.c, option, field, value, null_mut()),
                    0
                );
            }
        }
        values
    }
}

fn bridge_cfg(steps: usize, bridge: i32) -> McConfig {
    McConfig {
        present: 1 | 4 | 32 | 512,
        steps,
        samples: 40_000,
        seed: 42,
        brownian_bridge: bridge,
        ..McConfig::default()
    }
}

fn cv_cfg(cv: i32) -> McConfig {
    McConfig {
        present: 1 | 4 | 32 | 64 | 128 | 1024,
        steps: 75,
        samples: 4096,
        seed: 42,
        antithetic: 1,
        polynomial_order: 3,
        control_variate: cv,
        ..McConfig::default()
    }
}

#[test]
fn brownian_bridge_matches_the_core_engine_and_the_core_oracle() {
    let mut m = market(false);
    let bridged = m.engine(0, bridge_cfg(12, 1)).unwrap();
    let [npv, se, _] = m.price(bridged, false, 2);
    let core = MakeMcEuropeanEngine::<PseudoRandom>::new(m.c.get(m.process).unwrap())
        .with_steps(12)
        .with_samples(40_000)
        .with_seed(42)
        .with_brownian_bridge(true)
        .build()
        .unwrap();
    let core =
        m.c.insert(shared_mut(core) as SharedMut<dyn PricingEngine>)
            .unwrap();
    assert_eq!(m.price(core, false, 1)[0], npv);
    assert_eq!(npv, BRIDGE_NPV);

    let mut option = 0;
    let mut reference = f64::NAN;
    unsafe {
        itofin_option_new(
            &mut m.c,
            0,
            100.0,
            0,
            m.today + 360,
            0,
            m.settings,
            &mut option,
            null_mut(),
        );
        assert_eq!(
            itofin_option_set_engine(&mut m.c, option, m.process, 0, 0, null_mut()),
            0
        );
        assert_eq!(
            itofin_option_value(&mut m.c, option, 0, &mut reference, null_mut()),
            0
        );
    }
    assert!(
        se > 0.0 && (npv - reference).abs() < 3.0 * se,
        "{npv} +/- {se} vs {reference}"
    );
    let plain = m.engine(0, bridge_cfg(12, 0)).unwrap();
    assert_ne!(m.price(plain, false, 1)[0], npv);
    let one = m.engine(0, bridge_cfg(1, 1)).unwrap();
    let one_plain = m.engine(0, bridge_cfg(1, 0)).unwrap();
    assert!((m.price(one, false, 1)[0] - m.price(one_plain, false, 1)[0]).abs() <= 1e-12 * npv);
}

#[test]
fn control_variate_matches_the_core_engine_and_reduces_the_error() {
    let mut m = market(true);
    let controlled = m.engine(2, cv_cfg(1)).unwrap();
    let [npv, se, probability] = m.price(controlled, true, 3);
    let plain = m.engine(2, cv_cfg(0)).unwrap();
    let [plain_npv, plain_se, plain_probability] = m.price(plain, true, 3);
    assert_eq!(probability, plain_probability);
    assert!(se < plain_se, "cv {se} vs plain {plain_se}");
    assert_ne!(npv, plain_npv);
    assert_eq!(npv, CV_NPV);

    let core = MakeMcAmericanEngine::<PseudoRandom>::new(m.c.get(m.process).unwrap())
        .with_steps(75)
        .with_samples(4096)
        .with_seed(42)
        .with_antithetic_variate(true)
        .with_polynomial_order(3)
        .with_control_variate(true)
        .build()
        .unwrap();
    let core =
        m.c.insert(shared_mut(core) as SharedMut<dyn PricingEngine>)
            .unwrap();
    assert_eq!(m.price(core, true, 1)[0], npv);
    let mut basis = 0;
    let status = unsafe {
        itofin_mc_american_engine_new(&mut m.c, m.process, cv_cfg(1), 0, &mut basis, null_mut())
    };
    assert_eq!(status, 0);
    assert_eq!(m.price(basis, true, 1)[0], npv);
}

#[test]
fn engines_outlive_a_released_process() {
    let mut m = market(false);
    let bridged = m.engine(0, bridge_cfg(12, 1)).unwrap();
    assert_eq!(
        unsafe { itofin_handle_release(&mut m.c, m.process, null_mut()) },
        0
    );
    assert_eq!(m.price(bridged, false, 1)[0], BRIDGE_NPV);

    let mut m = market(true);
    let controlled = m.engine(2, cv_cfg(1)).unwrap();
    assert_eq!(
        unsafe { itofin_handle_release(&mut m.c, m.process, null_mut()) },
        0
    );
    assert_eq!(m.price(controlled, true, 1)[0], CV_NPV);
}

#[test]
fn unsupported_combinations_and_bad_booleans_are_rejected() {
    let mut m = market(false);
    let cv = McConfig {
        present: 1 | 4 | 1024,
        steps: 1,
        samples: 4095,
        control_variate: 1,
        ..McConfig::default()
    };
    let bridge = McConfig {
        present: 1 | 4 | 512,
        brownian_bridge: 0,
        ..cv
    };
    for (kind, cfg) in [(0, cv), (1, cv), (3, cv), (1, bridge), (2, bridge)] {
        assert_eq!(m.engine(kind, cfg), Err(INVALID_ARGUMENT), "kind {kind}");
    }
    for cfg in [
        McConfig {
            brownian_bridge: 2,
            ..bridge
        },
        McConfig {
            present: 1 | 4 | 1024,
            control_variate: -1,
            ..cv
        },
    ] {
        assert_eq!(m.engine(0, cfg), Err(INVALID_ARGUMENT));
        assert_eq!(m.engine(2, cfg), Err(INVALID_ARGUMENT));
    }
    let mut out = 77;
    let status = unsafe {
        itofin_mc_american_engine_new(&mut m.c, m.process, bridge, 0, &mut out, null_mut())
    };
    assert_eq!((status, out), (INVALID_ARGUMENT, 77));
}
