//! MC European under an external [`LocalVolSurface`] (#1365): end-to-end
//! oracle for the `MCEuropeanEngine` over
//! `GeneralizedBlackScholesProcess::with_local_vol`.
//!
//! Provenance: no QuantLib test-suite case prices a vanilla by Monte Carlo
//! under an external local vol; QL's local-vol pricing tests go through
//! `FdBlackScholesVanillaEngine(..., localVol=true)` (`hestonslvmodel.cpp`)
//! instead. The oracle is analytic equivalence: a `LocalVolSurface` over a
//! strike-independent Black curve gives a time-only Dupire vol, under which
//! the price is Black's formula.
//!
//! Bias budget. An external local vol keeps the process on its Euler branch
//! (`blackscholesprocess.cpp:44`, `isStrikeIndependent_=false`), and every
//! Euler step is an exact lognormal step at the frozen `sigma_loc(t_i)` with a
//! matching drift. The MC expectation is therefore exactly Black's price at
//! the Euler variance `V_euler = sum_i sigma_loc(t_i)^2 * dt`, read here from
//! the surface itself at the grid's step starts. The tests pin
//! `|mc - black(V_euler)| < 3 * se` (zero discretisation bias) and bound
//! `|black(V_euler) - analytic| <= budget`, the Euler bias of the steps that
//! straddle a variance-curve node; the acceptance band against
//! `AnalyticEuropeanEngine` is `3 * se + budget`. `steps_per_year` is pinned
//! and the curve nodes sit off the step starts, so the centred `dt = 1e-4`
//! time derivative of `localvolsurface.cpp:117-136` never averages the two
//! slopes at a node. Here the grid is 12 steps a year (30-day steps on
//! Actual360) with nodes at days 119 and 269, one day before a step end, so
//! the straddle bias is about 0.02 in price against a stated 0.03 budget.
//!
//! Divergence from the ticket's error criterion: a query "outside the
//! surface" cannot fail, because `LocalVolSurface` always reads the Black
//! surface with extrapolation on (`localvolsurface.rs:189-191`, as
//! `localvolsurface.cpp:97-99`). The engine error is driven instead through
//! the surface's own `decreasing variance` check
//! (`localvolsurface.rs:202,217-218`).

use crate::exercise::EuropeanExercise;
use crate::handle::Handle;
use crate::instrument::Instrument;
use crate::instruments::{EuropeanOption, PlainVanillaPayoff};
use crate::math::interpolations::bicubic::Bicubic;
use crate::math::matrix::Matrix;
use crate::math::randomnumbers::rngtraits::PseudoRandom;
use crate::option::OptionType;
use crate::pricingengine::PricingEngine;
use crate::pricingengines::blackformula::black_formula;
use crate::pricingengines::vanilla::test_market::{Market, market, today};
use crate::pricingengines::vanilla::{AnalyticEuropeanEngine, MakeMcEuropeanEngine};
use crate::processes::GeneralizedBlackScholesProcess;
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::termstructures::volatility::{
    BlackVarianceCurve, BlackVarianceSurface, BlackVolTermStructure, LocalVolSurface,
    LocalVolTermStructure,
};
use crate::time::daycounters::actual360::Actual360;
use crate::types::{Real, Size, Time};

const SPOT: Real = 100.0;
const Q: Real = 0.02;
const R: Real = 0.05;
const ATM_VOL: Real = 0.20;
const EXPIRY_DAYS: i32 = 360;
const STEPS_PER_YEAR: Size = 12;
const SAMPLES: Size = 20_000;
const SMILE_SAMPLES: Size = 4_000;
const SEED: u32 = 42;

fn set_market() -> Market {
    let market = market();
    market.set(SPOT, Q, R, ATM_VOL);
    market
}

fn local_vol_process(
    market: &Market,
    black: Shared<dyn BlackVolTermStructure>,
) -> (
    Shared<GeneralizedBlackScholesProcess>,
    Shared<dyn LocalVolTermStructure>,
) {
    let black = Handle::new(black);
    let local = shared(LocalVolSurface::new(
        black.clone(),
        market.process.risk_free_rate(),
        market.process.dividend_yield(),
        market.process.state_variable(),
    )) as Shared<dyn LocalVolTermStructure>;
    let process = shared(GeneralizedBlackScholesProcess::with_local_vol(
        market.process.state_variable(),
        market.process.dividend_yield(),
        market.process.risk_free_rate(),
        black,
        Handle::new(Shared::clone(&local)),
    ));
    (process, local)
}

fn term_curve() -> Shared<dyn BlackVolTermStructure> {
    let dates = [today() + 119, today() + 269, today() + EXPIRY_DAYS];
    shared(
        BlackVarianceCurve::new(today(), &dates, &[0.15, 0.20, 0.25], Actual360::new(), true)
            .unwrap(),
    ) as Shared<dyn BlackVolTermStructure>
}

fn option(
    market: &Market,
    engine: SharedMut<dyn PricingEngine>,
    option_type: OptionType,
    strike: Real,
    expiry_days: i32,
) -> EuropeanOption {
    let payoff = shared(PlainVanillaPayoff::new(option_type, strike));
    let exercise = shared(EuropeanExercise::new(today() + expiry_days));
    let mut option = EuropeanOption::new(payoff, exercise, Shared::clone(&market.settings));
    option.base_mut().set_pricing_engine(engine);
    option
}

fn mc_engine(
    process: &Shared<GeneralizedBlackScholesProcess>,
    samples: Size,
) -> SharedMut<dyn PricingEngine> {
    shared_mut(
        MakeMcEuropeanEngine::<PseudoRandom>::new(Shared::clone(process))
            .with_steps_per_year(STEPS_PER_YEAR)
            .with_samples(samples)
            .with_seed(SEED)
            .build()
            .unwrap(),
    ) as SharedMut<dyn PricingEngine>
}

fn analytic_engine(
    process: &Shared<GeneralizedBlackScholesProcess>,
) -> SharedMut<dyn PricingEngine> {
    shared_mut(AnalyticEuropeanEngine::new(Shared::clone(process))) as SharedMut<dyn PricingEngine>
}

fn euler_variance(local: &Shared<dyn LocalVolTermStructure>, expiry: Time) -> Real {
    let steps = (STEPS_PER_YEAR as Real * expiry) as Size;
    let dt = expiry / steps as Real;
    (0..steps)
        .map(|i| {
            let sigma = local.local_vol(i as Real * dt, SPOT, true).unwrap();
            sigma * sigma * dt
        })
        .sum()
}

fn black_price(option_type: OptionType, strike: Real, expiry: Time, variance: Real) -> Real {
    let forward = SPOT * ((R - Q) * expiry).exp();
    black_formula(
        option_type,
        strike,
        forward,
        variance.sqrt(),
        (-R * expiry).exp(),
        0.0,
    )
    .unwrap()
}

#[test]
fn time_only_local_vol_matches_analytic_european() {
    let market = set_market();
    let (process, local) = local_vol_process(&market, term_curve());
    let expiry = 1.0;
    let v_euler = euler_variance(&local, expiry);

    for option_type in [OptionType::Call, OptionType::Put] {
        for strike in [90.0, 100.0, 110.0] {
            let analytic = option(
                &market,
                analytic_engine(&process),
                option_type,
                strike,
                EXPIRY_DAYS,
            )
            .npv()
            .unwrap();
            let mut mc = option(
                &market,
                mc_engine(&process, SAMPLES),
                option_type,
                strike,
                EXPIRY_DAYS,
            );
            let mc_npv = mc.npv().unwrap();
            let se = mc.error_estimate().unwrap();
            let euler = black_price(option_type, strike, expiry, v_euler);
            let budget = (euler - analytic).abs();

            assert!(se > 0.0);
            assert!(
                (mc_npv - euler).abs() < 3.0 * se,
                "{option_type:?} K={strike}: |mc {mc_npv} - black(V_euler) {euler}| not within 3*se={}",
                3.0 * se
            );
            assert!(
                budget < 0.03,
                "{option_type:?} K={strike}: Euler bias {budget} over the 0.03 budget"
            );
            assert!(
                (mc_npv - analytic).abs() < 3.0 * se + budget,
                "{option_type:?} K={strike}: |mc {mc_npv} - analytic {analytic}| not within 3*se + budget"
            );
        }
    }
}

#[test]
fn flat_black_vol_local_surface_matches_analytic_european() {
    let market = set_market();
    let (process, _) = local_vol_process(
        &market,
        market.process.black_volatility().current_link().unwrap(),
    );

    for (option_type, strike) in [(OptionType::Call, 100.0), (OptionType::Put, 90.0)] {
        let analytic = option(
            &market,
            analytic_engine(&process),
            option_type,
            strike,
            EXPIRY_DAYS,
        )
        .npv()
        .unwrap();
        let mut mc = option(
            &market,
            mc_engine(&process, SAMPLES),
            option_type,
            strike,
            EXPIRY_DAYS,
        );
        let mc_npv = mc.npv().unwrap();
        let se = mc.error_estimate().unwrap();
        assert!(
            (mc_npv - analytic).abs() < 3.0 * se,
            "{option_type:?} K={strike}: |mc {mc_npv} - analytic {analytic}| not within 3*se={}",
            3.0 * se
        );
    }
}

fn smile_surface() -> BlackVarianceSurface {
    let dates = [today() + 60, today() + 120, today() + 240];
    let strikes: Vec<Real> = (-7..=7).map(|k| SPOT * (0.2 * k as Real).exp()).collect();
    let mut vols = Matrix::with_size(strikes.len(), dates.len());
    for (i, &strike) in strikes.iter().enumerate() {
        let m = (strike / SPOT).ln();
        for j in 0..dates.len() {
            vols[(i, j)] = ATM_VOL - 0.1 * (2.0 * m).tanh();
        }
    }
    let mut surface =
        BlackVarianceSurface::new(today(), None, &dates, strikes, &vols, Actual360::new()).unwrap();
    surface.set_interpolation(&Bicubic).unwrap();
    surface
}

#[test]
fn smile_moves_the_local_vol_price() {
    let market = set_market();
    let (process, _) = local_vol_process(&market, shared(smile_surface()));
    let expiry_days = 180;

    for (option_type, strike, sign) in [
        (OptionType::Call, 115.0, -1.0),
        (OptionType::Put, 85.0, 1.0),
    ] {
        let flat = market
            .option(option_type, strike, today() + expiry_days)
            .npv()
            .unwrap();
        let mut mc = option(
            &market,
            mc_engine(&process, SMILE_SAMPLES),
            option_type,
            strike,
            expiry_days,
        );
        let mc_npv = mc.npv().unwrap();
        let se = mc.error_estimate().unwrap();
        assert!(
            sign * (mc_npv - flat) > 5.0 * se,
            "{option_type:?} K={strike}: skew mc {mc_npv} not moved from flat-ATM {flat} by 5*se={}",
            5.0 * se
        );
    }
}

#[test]
fn spot_bump_reprices_through_the_surface() {
    let market = set_market();
    let (process, _) = local_vol_process(&market, term_curve());
    let mut mc = option(
        &market,
        mc_engine(&process, SAMPLES),
        OptionType::Call,
        100.0,
        EXPIRY_DAYS,
    );
    let before = mc.npv().unwrap();
    assert!(mc.base().is_calculated());

    market.spot.set_value(105.0);
    assert!(!mc.base().is_calculated());
    let after = mc.npv().unwrap();
    assert_ne!(before, after);
    assert!(after > before);
}

#[test]
fn decreasing_variance_surfaces_an_engine_err() {
    let market = set_market();
    let dates = [today() + 102, today() + 252];
    let curve = shared(
        BlackVarianceCurve::new(today(), &dates, &[0.30, 0.10], Actual360::new(), false).unwrap(),
    ) as Shared<dyn BlackVolTermStructure>;
    let (process, _) = local_vol_process(&market, curve);
    let mut mc = option(
        &market,
        mc_engine(&process, SAMPLES),
        OptionType::Call,
        100.0,
        180,
    );
    let err = mc.npv().unwrap_err();
    assert!(
        err.message().contains("decreasing variance"),
        "{}",
        err.message()
    );
}

#[test]
fn smile_without_external_local_vol_stays_an_err() {
    let market = set_market();
    let process = shared(GeneralizedBlackScholesProcess::new(
        market.process.state_variable(),
        market.process.dividend_yield(),
        market.process.risk_free_rate(),
        Handle::new(shared(smile_surface()) as Shared<dyn BlackVolTermStructure>),
    ));
    let mut mc = option(
        &market,
        mc_engine(&process, SAMPLES),
        OptionType::Call,
        100.0,
        180,
    );
    let err = mc.npv().unwrap_err();
    assert!(
        err.message().contains("only constant Black volatilities"),
        "{}",
        err.message()
    );
}
