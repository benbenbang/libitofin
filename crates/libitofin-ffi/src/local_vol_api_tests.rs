//! Local volatility through the C ABI (#1366), against the core #1365 MC
//! European local-vol oracle on the same market, grid and seed.
//!
//! The smile case is adapted: the bindings' `BlackVarianceSurface` defaults
//! to bilinear, and #1365's tanh grid `vol = 0.20 - 0.1 tanh(2 m)` under
//! bilinear fails in the core with "negative local vol^2 at strike 67.03...
//! and time 0.333...; the black vol surface is not smooth enough" (asserted
//! below). The convex skew `vol = 0.20 - 0.1 m + 0.05 m^2`, with
//! `m = ln(strike / spot)`, keeps the bilinear kinks positive and keeps
//! #1365's criterion `sign * (mc - flat) > 5 * se`. #1399 adds the Bicubic
//! choice: the original tanh grid under it matches the same chain built
//! through the core in-process bit for bit. The pinned macOS NPVs of the core
//! oracle (`mc_local_vol_oracle.rs`) hold to 1e-8 only, since the MC path calls
//! the platform `exp` and `log`.
use crate::boundary::*;
use crate::curves_api::itofin_flat_forward_new;
use crate::local_vol_api::*;
use crate::market_api::*;
use crate::mc_api::{McConfig, itofin_mc_engine_new};
use crate::options_api::*;
use crate::vol_api::{
    itofin_black_variance_curve_new, itofin_black_variance_surface_new,
    itofin_black_variance_surface_new_with_interpolation, itofin_black_vol_query,
};
use libitofin::exercise::EuropeanExercise;
use libitofin::handle::Handle;
use libitofin::instrument::Instrument;
use libitofin::instruments::{EuropeanOption, PlainVanillaPayoff};
use libitofin::math::randomnumbers::rngtraits::PseudoRandom;
use libitofin::math::{interpolations::bicubic::Bicubic, matrix::Matrix};
use libitofin::option::OptionType;
use libitofin::pricingengine::PricingEngine;
use libitofin::pricingengines::blackformula::black_formula;
use libitofin::pricingengines::vanilla::MakeMcEuropeanEngine;
use libitofin::processes::GeneralizedBlackScholesProcess;
use libitofin::quotes::{Quote, SimpleQuote};
use libitofin::settings::Settings;
use libitofin::shared::shared;
use libitofin::shared::{Shared, SharedMut, shared_mut};
use libitofin::termstructures::volatility::{
    BlackVarianceSurface, BlackVolTermStructure, LocalVolSurface, LocalVolTermStructure,
};
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual360::Actual360;
use std::os::raw::c_char;

const SPOT: f64 = 100.0;
const Q: f64 = 0.02;
const R: f64 = 0.05;

type Ctx = *mut Context;
type ErrPtr = *mut ItofinError;

struct Market {
    c: Context,
    today: i32,
    settings: u64,
    dc: u64,
    spot: u64,
    risk_free: u64,
    dividend: u64,
}

fn market() -> Market {
    let mut c = Context::new();
    let today = Date::new(15, Month::June, 2026);
    let settings = shared(Settings::new());
    settings.set_evaluation_date(today);
    let settings = c.insert(settings).unwrap();
    let dc = c.insert(Actual360::new()).unwrap();
    let today = today.serial_number();
    let mut m = Market {
        c,
        today,
        settings,
        dc,
        spot: 0,
        risk_free: 0,
        dividend: 0,
    };
    let (mut spot, mut risk_free, mut dividend) = (0, 0, 0);
    m.ok(|c, e| unsafe { itofin_quote_new(c, SPOT, &mut spot, e) });
    m.ok(|c, e| unsafe { itofin_flat_forward_new(c, today, R, dc, &mut risk_free, e) });
    m.ok(|c, e| unsafe { itofin_flat_forward_new(c, today, Q, dc, &mut dividend, e) });
    Market {
        spot,
        risk_free,
        dividend,
        ..m
    }
}

impl Market {
    fn call(&mut self, f: impl FnOnce(Ctx, ErrPtr) -> i32) -> Result<(), String> {
        let mut e = ItofinError {
            code: 0,
            message: [0 as c_char; 1024],
        };
        if f(&mut self.c, &mut e) == 0 {
            return Ok(());
        }
        let bytes: Vec<u8> = e
            .message
            .iter()
            .take_while(|v| **v != 0)
            .map(|v| *v as u8)
            .collect();
        Err(String::from_utf8_lossy(&bytes).into_owned())
    }

    fn ok(&mut self, f: impl FnOnce(Ctx, ErrPtr) -> i32) {
        self.call(f).unwrap()
    }

    fn term_curve(&mut self, vols: &[f64], days: &[i32], monotone: i32) -> u64 {
        let dates: Vec<i32> = days.iter().map(|d| self.today + d).collect();
        let (today, n, dc, mut id) = (self.today, vols.len(), self.dc, 0);
        let (d, v) = (dates.as_ptr(), vols.as_ptr());
        self.ok(|c, e| unsafe {
            itofin_black_variance_curve_new(c, today, d, v, n, dc, monotone, 0, &mut id, e)
        });
        id
    }

    fn smile_surface(&mut self) -> u64 {
        self.surface(|m| 0.2 - 0.1 * m + 0.05 * m * m)
    }

    fn surface(&mut self, smile: impl Fn(f64) -> f64) -> u64 {
        let dates: Vec<i32> = [60, 120, 240].iter().map(|d| self.today + d).collect();
        let strikes: Vec<f64> = (-7..=7)
            .map(|k| SPOT * (0.2 * f64::from(k)).exp())
            .collect();
        let vols: Vec<f64> = strikes
            .iter()
            .flat_map(|k| [smile((k / SPOT).ln()); 3])
            .collect();
        let (today, dc, mut id) = (self.today, self.dc, 0);
        let (d, k, v) = (dates.as_ptr(), strikes.as_ptr(), vols.as_ptr());
        self.ok(|c, e| unsafe {
            itofin_black_variance_surface_new(c, today, d, 3, k, 15, v, 45, dc, 0, &mut id, e)
        });
        id
    }

    fn local_process(&mut self, black: u64) -> (u64, u64) {
        let (spot, r, q, mut local, mut process) = (self.spot, self.risk_free, self.dividend, 0, 0);
        self.ok(|c, e| unsafe {
            itofin_local_vol_surface_new(c, black, r, q, spot, &mut local, e)
        });
        self.ok(|c, e| unsafe {
            itofin_black_scholes_with_local_vol(c, spot, r, q, black, local, &mut process, e)
        });
        (local, process)
    }

    fn option(
        &mut self,
        process: u64,
        kind: i32,
        strike: f64,
        days: i32,
        samples: usize,
    ) -> Result<u64, String> {
        let (expiry, settings, mut option, mut engine) = (self.today + days, self.settings, 0, 0);
        let cfg = McConfig {
            present: 2 | 4 | 32,
            steps_per_year: 12,
            samples,
            seed: 42,
            ..McConfig::default()
        };
        self.call(|c, e| unsafe {
            itofin_option_new(c, kind, strike, 0, expiry, 0, settings, &mut option, e)
        })?;
        if samples == 0 {
            self.call(|c, e| unsafe { itofin_option_set_engine(c, option, process, 0, 0, e) })?;
        } else {
            self.call(|c, e| unsafe { itofin_mc_engine_new(c, process, 0, cfg, &mut engine, e) })?;
            self.call(|c, e| unsafe { itofin_option_set_engine(c, option, engine, 2, 0, e) })?;
        }
        Ok(option)
    }

    fn value(&mut self, option: u64, field: i32) -> Result<f64, String> {
        let mut v = 0.0;
        self.call(|c, e| unsafe { itofin_option_value(c, option, field, &mut v, e) })?;
        Ok(v)
    }

    fn price(
        &mut self,
        process: u64,
        kind: i32,
        strike: f64,
        days: i32,
        samples: usize,
    ) -> Result<(f64, f64), String> {
        let option = self.option(process, kind, strike, days, samples)?;
        let se = if samples == 0 {
            0.0
        } else {
            self.value(option, 7)?
        };
        Ok((self.value(option, 0)?, se))
    }

    fn local_vol(&mut self, id: u64, t: f64, extrapolate: i32) -> Result<f64, String> {
        let mut v = 0.0;
        self.call(|c, e| unsafe {
            itofin_local_vol_query(c, id, t, SPOT, extrapolate, &mut v, e)
        })?;
        Ok(v)
    }
}

#[test]
fn time_only_local_vol_matches_analytic_european() {
    let mut m = market();
    let black = m.term_curve(&[0.15, 0.20, 0.25], &[119, 269, 360], 1);
    let (local, process) = m.local_process(black);
    let v_euler: f64 = (0..12)
        .map(|i| m.local_vol(local, f64::from(i) / 12.0, 1).unwrap().powi(2) / 12.0)
        .sum();
    for (kind, option_type) in [(0, OptionType::Call), (1, OptionType::Put)] {
        for strike in [90.0, 100.0, 110.0] {
            let (analytic, _) = m.price(process, kind, strike, 360, 0).unwrap();
            let (mc, se) = m.price(process, kind, strike, 360, 20_000).unwrap();
            let forward = SPOT * (R - Q).exp();
            let euler = black_formula(
                option_type,
                strike,
                forward,
                v_euler.sqrt(),
                (-R).exp(),
                0.0,
            )
            .unwrap();
            let budget = (euler - analytic).abs();
            assert!(
                se > 0.0 && (mc - euler).abs() < 3.0 * se,
                "K={strike}: mc {mc} euler {euler} se {se}"
            );
            assert!(budget < 0.03 && (mc - analytic).abs() < 3.0 * se + budget);
        }
    }
}

#[test]
fn smile_moves_the_local_vol_price() {
    let mut m = market();
    let surface = m.smile_surface();
    let (_, process) = m.local_process(surface);
    let (today, dc, mut flat) = (m.today, m.dc, 0);
    m.ok(|c, e| unsafe { itofin_black_scholes_new(c, SPOT, R, Q, 0.20, today, dc, &mut flat, e) });
    for (kind, strike, sign) in [(0, 115.0, -1.0), (1, 85.0, 1.0)] {
        let (flat, _) = m.price(flat, kind, strike, 180, 0).unwrap();
        let (mc, se) = m.price(process, kind, strike, 180, 20_000).unwrap();
        let (implied, _) = m.price(process, kind, strike, 180, 0).unwrap();
        assert!(
            sign * (mc - flat) > 5.0 * se,
            "K={strike}: mc {mc} flat {flat} se {se}"
        );
        assert!(
            (mc - implied).abs() < 3.0 * se + 0.01,
            "K={strike}: mc {mc} black implied {implied} se {se}"
        );
    }
}

#[test]
fn smile_surface_observes_the_shared_spot() {
    let mut m = market();
    let surface = m.smile_surface();
    let (_, process) = m.local_process(surface);
    let option = m.option(process, 0, 115.0, 180, 20_000).unwrap();
    let before = m.value(option, 0).unwrap();
    let spot = m.spot;
    m.ok(|c, e| unsafe { itofin_quote_set(c, spot, 105.0, e) });
    let after = m.value(option, 0).unwrap();

    let mut fresh = market();
    let spot = fresh.spot;
    fresh.ok(|c, e| unsafe { itofin_quote_set(c, spot, 105.0, e) });
    let surface = fresh.smile_surface();
    let (_, process) = fresh.local_process(surface);
    let (rebuilt, _) = fresh.price(process, 0, 115.0, 180, 20_000).unwrap();
    assert_ne!(after, before);
    assert_eq!(after.to_bits(), rebuilt.to_bits(), "{after} vs {rebuilt}");
}

#[test]
fn shared_quote_reprices_and_handles_outlive_callers() {
    let mut m = market();
    let black = m.term_curve(&[0.15, 0.20, 0.25], &[119, 269, 360], 1);
    let (local, process) = m.local_process(black);
    let option = m.option(process, 0, 100.0, 360, 20_000).unwrap();
    let before = m.value(option, 0).unwrap();
    let (spot, mut flag) = (m.spot, 1);
    m.ok(|c, e| unsafe { itofin_quote_set(c, spot, 105.0, e) });
    m.ok(|c, e| unsafe { itofin_option_calculate(c, option, 0, &mut flag, e) });
    assert_eq!(flag, 0);
    let after = m.value(option, 0).unwrap();
    assert!(after > before);
    for id in [black, local, m.spot, m.risk_free, m.dividend] {
        m.ok(|c, e| unsafe { itofin_handle_release(c, id, e) });
    }
    assert_eq!(m.price(process, 0, 100.0, 360, 20_000).unwrap().0, after);
}

#[test]
fn local_vol_core_errors() {
    let mut m = market();
    let black = m.term_curve(&[0.15, 0.20, 0.25], &[119, 269, 360], 1);
    let (local, _) = m.local_process(black);
    assert!(
        m.local_vol(local, f64::NAN, 1)
            .unwrap_err()
            .contains("finite")
    );
    assert!(m.local_vol(local, 0.5, 2).unwrap_err().contains("boolean"));
    let past = m.local_vol(local, 1.5, 0).unwrap_err();
    assert!(past.contains("past max curve time"), "{past}");

    let decreasing = m.term_curve(&[0.30, 0.10], &[102, 252], 0);
    let (_, process) = m.local_process(decreasing);
    let err = m.price(process, 0, 100.0, 180, 20_000).unwrap_err();
    assert!(err.contains("decreasing variance"), "{err}");

    let surface = m.smile_surface();
    let (spot, r, q, mut plain) = (m.spot, m.risk_free, m.dividend, 0);
    m.ok(|c, e| unsafe { itofin_black_scholes_from_quote(c, spot, r, q, surface, &mut plain, e) });
    let err = m.price(plain, 0, 100.0, 180, 20_000).unwrap_err();
    assert!(err.contains("only constant Black volatilities"), "{err}");
}

#[test]
fn linear_local_vol_curve_prices_and_queries() {
    let mut m = market();
    let black = m.term_curve(&[0.15, 0.20, 0.25], &[119, 269, 360], 1);
    let dates: Vec<i32> = [119, 269, 360].iter().map(|d| m.today + d).collect();
    let (today, dc, d, v) = (m.today, m.dc, dates.as_ptr(), [0.15, 0.20, 0.25].as_ptr());
    let (spot, r, q, mut curve, mut process) = (m.spot, m.risk_free, m.dividend, 0, 0);
    m.ok(|c, e| unsafe { itofin_local_vol_curve_new(c, today, d, v, 3, dc, 1, 0, &mut curve, e) });
    assert!((m.local_vol(curve, 0.1, 0).unwrap() - 0.15).abs() < 1e-12);
    let past = m.local_vol(curve, 1.5, 0).unwrap_err();
    assert!(past.contains("past max curve time"), "{past}");
    assert!(m.local_vol(curve, 1.5, 1).is_ok());
    let err = m
        .call(|c, e| unsafe { itofin_local_vol_curve_new(c, today, d, v, 3, dc, 1, 3, &mut 0, e) });
    assert!(err.unwrap_err().contains("unknown time extrapolation"));

    m.ok(|c, e| unsafe {
        itofin_black_scholes_with_local_vol(c, spot, r, q, black, curve, &mut process, e)
    });
    let v_euler: f64 = (0..12)
        .map(|i| m.local_vol(curve, f64::from(i) / 12.0, 1).unwrap().powi(2) / 12.0)
        .sum();
    for id in [black, curve] {
        m.ok(|c, e| unsafe { itofin_handle_release(c, id, e) });
    }
    let (analytic, _) = m.price(process, 0, 100.0, 360, 0).unwrap();
    let (mc, se) = m.price(process, 0, 100.0, 360, 20_000).unwrap();
    let euler = black_formula(
        OptionType::Call,
        100.0,
        SPOT * (R - Q).exp(),
        v_euler.sqrt(),
        (-R).exp(),
        0.0,
    )
    .unwrap();
    let budget = (euler - analytic).abs();
    assert!(
        (mc - euler).abs() < 3.0 * se,
        "mc {mc} euler {euler} se {se}"
    );
    assert!(budget < 0.03 && (mc - analytic).abs() < 3.0 * se + budget);
}

#[test]
fn tanh_smile_grid_under_bilinear_returns_the_core_error() {
    let mut m = market();
    let black = m.surface(|m| 0.2 - 0.1 * (2.0 * m).tanh());
    let (local, _) = m.local_process(black);
    let mut e = ItofinError {
        code: 0,
        message: [0 as c_char; 1024],
    };
    let mut v = 0.0;
    let status = unsafe {
        itofin_local_vol_query(
            &mut m.c,
            local,
            1.0 / 3.0,
            67.03185451850742,
            0,
            &mut v,
            &mut e,
        )
    };
    let bytes: Vec<u8> = e
        .message
        .iter()
        .take_while(|b| **b != 0)
        .map(|b| *b as u8)
        .collect();
    let message = String::from_utf8_lossy(&bytes);
    assert_ne!(status, 0);
    assert_eq!(e.code, CORE_ERROR, "{message}");
    assert!(
        message.contains("negative local vol^2") && message.contains("not smooth enough"),
        "{message}"
    );
}

#[test]
fn surface_query_matches_the_core_argument_order() {
    use libitofin::handle::Handle;
    use libitofin::quotes::{Quote, SimpleQuote};
    use libitofin::shared::Shared;
    use libitofin::termstructures::volatility::{
        BlackVolTermStructure, LocalVolSurface, LocalVolTermStructure,
    };
    use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
    let mut m = market();
    let black = m.smile_surface();
    let (local, _) = m.local_process(black);
    let curve = |id| m.c.get::<Handle<dyn YieldTermStructure>>(id).unwrap();
    let core = LocalVolSurface::new(
        m.c.get::<Handle<dyn BlackVolTermStructure>>(black).unwrap(),
        curve(m.risk_free),
        curve(m.dividend),
        Handle::new(m.c.get::<Shared<SimpleQuote>>(m.spot).unwrap() as Shared<dyn Quote>),
    );
    for strike in [90.0, 110.0] {
        let expected = core.local_vol(0.25, strike, false).unwrap();
        let mut v = 0.0;
        m.ok(|c, e| unsafe { itofin_local_vol_query(c, local, 0.25, strike, 0, &mut v, e) });
        assert_eq!(
            v.to_bits(),
            expected.to_bits(),
            "K={strike}: {v} vs {expected}"
        );
    }
}

/// The core #1365 fixture vols `0.20 - 0.1 tanh(2m)` at `m = 0.2k`,
/// `k = -7..=7`, as computed by Apple libm. glibc `tanh` is one ulp away at
/// `k = -1` and `k = 3`, so every binding feeds these exact bits instead.
const CORE_TANH_VOLS: [f64; 15] = [
    0.29926315202011283,
    0.29836748576936806,
    0.2964027580075817,
    0.29216685544064713,
    0.28336546070121554,
    0.2664036770267849,
    0.2379948962255225,
    0.2,
    0.16200510377447752,
    0.1335963229732151,
    0.11663453929878446,
    0.10783314455935288,
    0.10359724199241832,
    0.10163251423063198,
    0.1007368479798872,
];

fn tanh_grid(ks: &[i32]) -> (Vec<f64>, Vec<f64>) {
    ks.iter()
        .map(|&k| {
            (
                SPOT * (0.2 * f64::from(k)).exp(),
                CORE_TANH_VOLS[(k + 7) as usize],
            )
        })
        .unzip()
}

type CoreLocal = Shared<dyn LocalVolTermStructure>;

const SMILE_GRID: [i32; 15] = [-7, -6, -5, -4, -3, -2, -1, 0, 1, 2, 3, 4, 5, 6, 7];

impl Market {
    fn interpolated(&mut self, ks: &[i32], interpolation: i32) -> (u64, i32, String) {
        let dates: Vec<i32> = [60, 120, 240].iter().map(|d| self.today + d).collect();
        let (strikes, smile) = tanh_grid(ks);
        let vols: Vec<f64> = smile.iter().flat_map(|v| [*v; 3]).collect();
        let (today, dc, n, mut id) = (self.today, self.dc, strikes.len(), 0);
        let (d, k, v) = (dates.as_ptr(), strikes.as_ptr(), vols.as_ptr());
        let mut e = ItofinError {
            code: 0,
            message: [0 as c_char; 1024],
        };
        unsafe {
            itofin_black_variance_surface_new_with_interpolation(
                &mut self.c,
                today,
                d,
                3,
                k,
                n,
                v,
                3 * n,
                dc,
                0,
                interpolation,
                &mut id,
                &mut e,
            )
        };
        let bytes: Vec<u8> = e
            .message
            .iter()
            .take_while(|b| **b != 0)
            .map(|b| *b as u8)
            .collect();
        (id, e.code, String::from_utf8_lossy(&bytes).into_owned())
    }

    fn vol(&mut self, id: u64, t: f64, strike: f64) -> f64 {
        let mut v = 0.0;
        self.ok(|c, e| unsafe { itofin_black_vol_query(c, id, 0, t, 0.0, strike, 0, &mut v, e) });
        v
    }

    fn core_local(&self, ks: &[i32]) -> (Handle<dyn BlackVolTermStructure>, CoreLocal) {
        let today = Date::from_serial(self.today);
        let dates = [today + 60, today + 120, today + 240];
        let (strikes, smile) = tanh_grid(ks);
        let mut vols = Matrix::with_size(ks.len(), 3);
        for (i, v) in smile.iter().enumerate() {
            for j in 0..3 {
                vols[(i, j)] = *v;
            }
        }
        let mut surface =
            BlackVarianceSurface::new(today, None, &dates, strikes, &vols, Actual360::new())
                .unwrap();
        surface.set_interpolation(&Bicubic).unwrap();
        let black = Handle::new(shared(surface) as Shared<dyn BlackVolTermStructure>);
        let local = shared(LocalVolSurface::new(
            black.clone(),
            self.curve(self.risk_free),
            self.curve(self.dividend),
            self.spot_handle(),
        )) as CoreLocal;
        (black, local)
    }

    fn curve(&self, id: u64) -> Handle<dyn YieldTermStructure> {
        self.c.get::<Handle<dyn YieldTermStructure>>(id).unwrap()
    }

    fn spot_handle(&self) -> Handle<dyn Quote> {
        Handle::new(self.c.get::<Shared<SimpleQuote>>(self.spot).unwrap() as Shared<dyn Quote>)
    }

    fn core_bicubic_mc(&self, option_type: OptionType, strike: f64) -> (f64, f64) {
        let today = Date::from_serial(self.today);
        let (black, local) = self.core_local(&SMILE_GRID);
        let process = shared(GeneralizedBlackScholesProcess::with_local_vol(
            self.spot_handle(),
            self.curve(self.dividend),
            self.curve(self.risk_free),
            black,
            Handle::new(local),
        ));
        let engine = shared_mut(
            MakeMcEuropeanEngine::<PseudoRandom>::new(process)
                .with_steps_per_year(12)
                .with_samples(4_000)
                .with_seed(42)
                .build()
                .unwrap(),
        ) as SharedMut<dyn PricingEngine>;
        let mut option = EuropeanOption::new(
            shared(PlainVanillaPayoff::new(option_type, strike)),
            shared(EuropeanExercise::new(today + 180)),
            self.c.get::<Shared<Settings<Date>>>(self.settings).unwrap(),
        );
        option.base_mut().set_pricing_engine(engine);
        (option.npv().unwrap(), option.error_estimate().unwrap())
    }
}

#[test]
fn tanh_smile_under_bicubic_matches_the_core_oracle() {
    let mut m = market();
    let (surface, code, message) = m.interpolated(&SMILE_GRID, 1);
    assert_eq!(code, 0, "{message}");
    let (_, process) = m.local_process(surface);
    let (today, dc, mut flat) = (m.today, m.dc, 0);
    m.ok(|c, e| unsafe { itofin_black_scholes_new(c, SPOT, R, Q, 0.20, today, dc, &mut flat, e) });
    for (kind, option_type, strike, sign, npv, error) in [
        (
            0,
            OptionType::Call,
            115.0,
            -1.0,
            1.0751944785024798,
            0.05164019195806817,
        ),
        (
            1,
            OptionType::Put,
            85.0,
            1.0,
            1.0122245200291142,
            0.05721888187042385,
        ),
    ] {
        let (flat, _) = m.price(flat, kind, strike, 180, 0).unwrap();
        let (mc, se) = m.price(process, kind, strike, 180, 4_000).unwrap();
        let (core_mc, core_se) = m.core_bicubic_mc(option_type, strike);
        assert_eq!(
            mc.to_bits(),
            core_mc.to_bits(),
            "K={strike}: {mc} vs core {core_mc}"
        );
        assert_eq!(
            se.to_bits(),
            core_se.to_bits(),
            "K={strike}: {se} vs core {core_se}"
        );
        assert!(
            (mc - npv).abs() < 1e-8 && (se - error).abs() < 1e-8,
            "K={strike}: {mc} {se} vs pinned {npv} {error}"
        );
        assert!(
            sign * (mc - flat) > 5.0 * se,
            "K={strike}: mc {mc} flat {flat} se {se}"
        );
    }
}

#[test]
fn default_surface_is_bilinear_and_bicubic_differs_off_node() {
    let mut m = market();
    let default = m.surface(|x| CORE_TANH_VOLS[((x / 0.2).round() as i32 + 7) as usize]);
    let (bilinear, _, _) = m.interpolated(&SMILE_GRID, 0);
    let (bicubic, _, _) = m.interpolated(&SMILE_GRID, 1);
    for (t, strike) in [(0.25, 95.0), (0.5, 130.0), (1.0 / 6.0, 100.0)] {
        let (a, b) = (m.vol(default, t, strike), m.vol(bilinear, t, strike));
        assert_eq!(a.to_bits(), b.to_bits(), "t={t} K={strike}: {a} vs {b}");
    }
    let (b, c) = (m.vol(bilinear, 0.25, 95.0), m.vol(bicubic, 0.25, 95.0));
    assert!((b - c).abs() > 1e-6, "bilinear {b} bicubic {c}");
}

#[test]
fn interpolation_choice_errors() {
    let mut m = market();
    let (_, code, message) = m.interpolated(&SMILE_GRID, 2);
    assert_eq!(code, INVALID_ARGUMENT, "{message}");
    assert!(message.contains("unknown interpolation"), "{message}");
    let (_, code, message) = m.interpolated(&[0], 1);
    assert_eq!(code, CORE_ERROR, "{message}");
    assert!(
        message.contains("bilinear interpolation needs at least 2"),
        "{message}"
    );
}

#[test]
fn two_strike_bicubic_grid_prices_and_extrapolates_like_the_core() {
    let mut m = market();
    let (surface, code, message) = m.interpolated(&[-1, 1], 1);
    assert_eq!(code, 0, "{message}");
    let (local, process) = m.local_process(surface);
    let (npv, _) = m.price(process, 0, 100.0, 180, 0).unwrap();
    assert!(npv.is_finite() && npv > 0.0, "{npv}");
    let (_, core) = m.core_local(&[-1, 1]);
    for strike in [40.0, 250.0] {
        let mut v = 0.0;
        let binding = m
            .call(|c, e| unsafe { itofin_local_vol_query(c, local, 0.4, strike, 1, &mut v, e) })
            .map(|()| v);
        match core.local_vol(0.4, strike, true) {
            Ok(v) => assert_eq!(binding.unwrap().to_bits(), v.to_bits(), "K={strike}"),
            Err(e) => assert!(binding.unwrap_err().contains(e.message()), "K={strike}"),
        }
    }
}
