//! `testCrankNicolsonWithDamping`, `test-suite/fdmlinearop.cpp:1291-1377`.
//!
//! A cash-or-nothing put priced on a 400-point concentrated `ln(S)` grid,
//! rolled back over `FdmBlackScholesOp` by three implicit-Euler damping steps
//! and twenty-five Douglas steps, and read off a monotone natural cubic
//! spline. Both the value and the gamma are checked against the analytic
//! European engine at a relative 2e-3, so the whole stack - mesher, operator,
//! cell-averaging seed, damped rollback, spline read-off - is pinned by
//! numbers rather than by structure.
//!
//! The C++ fixture evaluates at `Date::todaysDate()` and dates the expiry
//! `today + timeToDays(0.75)`; the port pins the fixed evaluation date of
//! [`test_market`](crate::pricingengines::vanilla::test_market) and the same
//! 270-day offset, which is exactly 0.75 under `Actual/360`.

use crate::instrument::Instrument;
use crate::instruments::CashOrNothingPayoff;
use crate::math::array::Array;
use crate::math::interpolations::Interpolation;
use crate::math::interpolations::cubic::MonotonicCubicNaturalSpline;
use crate::methods::finitedifferences::meshers::{
    FdmMesher, FdmMesherComposite, fdm_black_scholes_mesher,
};
use crate::methods::finitedifferences::operators::{
    FdmBlackScholesOp, FdmLinearOpComposite, FdmLinearOpLayout,
};
use crate::methods::finitedifferences::solvers::{FdmBackwardSolver, FdmSchemeDesc};
use crate::methods::finitedifferences::utilities::{FdmInnerValueCalculator, fdm_log_inner_value};
use crate::option::OptionType::Put;
use crate::pricingengines::vanilla::test_market::{Market, market, time_to_days, today};
use crate::shared::{Shared, SharedMut, shared, shared_mut};
use crate::stochasticprocess::StochasticProcess1D;
use crate::time::date::Date;
use crate::types::{Rate, Real, Size, Time, Volatility};

const SPOT: Real = 100.0;
const STRIKE: Real = 100.0;
const CASH: Real = 10.0;
const RATE: Rate = 0.06;
const VOL: Volatility = 0.35;
const MATURITY: Time = 0.75;
const X_GRID: Size = 400;
const CS_STEPS: Size = 25;
const DAMPING_STEPS: Size = 3;
const REL_TOL: Real = 2e-3;

/// The `cpp:1296-1315` fixture, on quote-backed curves rather than C++'s
/// hard-coded flat ones. The quotes are set before anything reads them: the
/// mesher and the operator both take their curves eagerly.
fn fixture() -> Market {
    let market = market();
    market.set(SPOT, RATE, RATE, VOL);
    market
}

fn expiry() -> Date {
    today() + time_to_days(MATURITY)
}

fn payoff() -> Shared<CashOrNothingPayoff> {
    shared(CashOrNothingPayoff::new(Put, STRIKE, CASH))
}

/// `cpp:1316-1322`: the analytic reference the finite-difference roll is
/// measured against, as `(NPV, gamma)`.
fn analytic(market: &Market) -> (Real, Real) {
    let mut option = market.option_with_payoff(payoff(), expiry());
    (option.npv().unwrap(), option.gamma().unwrap())
}

/// `cpp:1329-1336`: one concentrating `ln(S)` mesher, wrapped in a composite.
fn mesher(market: &Market) -> Shared<FdmMesherComposite> {
    let equity = fdm_black_scholes_mesher(
        X_GRID,
        &market.process,
        MATURITY,
        STRIKE,
        None,
        None,
        0.0001,
        1.5,
        Some((STRIKE, 0.01)),
        &[],
        0.0,
    )
    .unwrap();

    shared(FdmMesherComposite::new(vec![equity]))
}

/// `cpp:1341-1349`: the terminal payoff cell-averaged onto the grid,
/// alongside the grid's own coordinates.
fn seed(mesher: &Shared<FdmMesherComposite>) -> (Vec<Real>, Array) {
    let calculator =
        fdm_log_inner_value(payoff(), Shared::clone(mesher) as Shared<dyn FdmMesher>, 0);
    let layout: &Shared<FdmLinearOpLayout> = mesher.layout();

    let mut rhs = Array::with_size(layout.size());
    let mut x = vec![0.0; layout.size()];
    for iter in layout.iter() {
        rhs[iter.index()] = calculator.avg_inner_value(&iter, MATURITY);
        x[iter.index()] = mesher.location(&iter, 0);
    }

    (x, rhs)
}

/// `cpp:1338-1354`: the damped Douglas roll from maturity to today, over an
/// empty boundary-condition set and no step condition.
fn rollback(
    market: &Market,
    mesher: &Shared<FdmMesherComposite>,
    rhs: &mut Array,
    damping_steps: Size,
) {
    let map = shared_mut(
        FdmBlackScholesOp::new(
            Shared::clone(mesher) as Shared<dyn FdmMesher>,
            &market.process,
            STRIKE,
            0,
        )
        .unwrap(),
    );

    let mut solver = FdmBackwardSolver::new(
        map as SharedMut<dyn FdmLinearOpComposite>,
        Vec::new(),
        None,
        FdmSchemeDesc::douglas(),
    );
    solver
        .rollback(rhs, MATURITY, 0.0, CS_STEPS, damping_steps)
        .unwrap();
}

/// `cpp:1356-1361`: value and gamma at the spot, off a monotone natural cubic
/// spline through the rolled-back grid. The gamma carries the `ln(S)` chain
/// rule, `(f'' - f') / S^2`.
///
/// The type matches C++'s by name, but on this fixture the Hyman filter never
/// fires: the rolled-back grid is already monotone in `ln(S)`, so the plain
/// `CubicNaturalSpline` reads off the same gamma to the last bit. The
/// monotonicity is fidelity, not the reason the tolerance is met.
fn read_off(x: Vec<Real>, rhs: &Array) -> (Real, Real) {
    let spline = MonotonicCubicNaturalSpline::new(x, rhs.to_vec()).unwrap();
    let log_spot = SPOT.ln();

    let pv = spline.value(log_spot).unwrap();
    let gamma = (spline.second_derivative(log_spot).unwrap()
        - spline.derivative(log_spot).unwrap())
        / (SPOT * SPOT);

    (pv, gamma)
}

/// The finite-difference `(PV, gamma)` over `damping_steps` damping steps.
fn finite_difference(market: &Market, damping_steps: Size) -> (Real, Real) {
    let mesher = mesher(market);
    let (x, mut rhs) = seed(&mesher);
    rollback(market, &mesher, &mut rhs, damping_steps);
    read_off(x, &rhs)
}

fn relative_error(calculated: Real, expected: Real) -> Real {
    (calculated - expected).abs() / expected.abs()
}

/// `cpp:1363-1376`, the oracle: PV and gamma both within a relative 2e-3 of
/// the analytic engine.
///
/// The gamma arm is what makes this more than a value check. The digital's
/// analytic gamma here is around 6e-4, so 2e-3 relative is an absolute band
/// near 1e-6 on a spline second derivative - a coefficient the value alone
/// cannot see, since a drift or diffusion term scaled wrongly can still
/// reprice the option to within a tenth of a percent.
#[test]
fn crank_nicolson_with_damping_prices_the_digital_put() {
    let market = fixture();
    let (expected_pv, expected_gamma) = analytic(&market);
    let (calculated_pv, calculated_gamma) = finite_difference(&market, DAMPING_STEPS);

    assert!(
        expected_gamma > 0.0,
        "the analytic gamma must be positive for a relative band to mean anything: \
         {expected_gamma}"
    );
    assert!(
        relative_error(calculated_pv, expected_pv) <= REL_TOL,
        "PV of the digital option: expected {expected_pv}, calculated {calculated_pv}, \
         rel. error {} against {REL_TOL}",
        relative_error(calculated_pv, expected_pv)
    );
    assert!(
        relative_error(calculated_gamma, expected_gamma) <= REL_TOL,
        "gamma of the digital option: expected {expected_gamma}, calculated \
         {calculated_gamma}, rel. error {} against {REL_TOL}",
        relative_error(calculated_gamma, expected_gamma)
    );
}

/// C++ dates the expiry through `timeToDays(0.75)` and hands the mesher the
/// bare 0.75 (`cpp:1307-1308`, `cpp:1331`), so the two only agree because the
/// day counter is `Actual/360`. Rust has no `timeToDays`, so the 270-day
/// offset is pinned rather than trusted: an expiry off the grid's maturity
/// would compare an FD price at one horizon against an analytic price at
/// another.
#[test]
fn the_expiry_is_exactly_the_maturity_the_grid_is_built_for() {
    let market = fixture();

    assert_eq!(expiry() - today(), 270);
    assert_eq!(market.process.time(&expiry()).unwrap(), MATURITY);
}

/// `cpp:1333`: the critical point is the first of the epic's oracles to reach
/// the mesher's concentrating branch, so the branch is pinned directly.
///
/// The spacing at the concentration point must be well below the spacing at
/// the edge; the uniform branch would make every gap equal. The final `dplus`
/// and the first `dminus` are null sentinels, so only interior gaps are read.
#[test]
fn the_critical_point_makes_the_grid_non_uniform() {
    let market = fixture();
    let mesher = mesher(&market);
    let locations = mesher.fdm_1d_meshers()[0].locations();

    let gaps: Vec<Real> = locations.windows(2).map(|pair| pair[1] - pair[0]).collect();
    let at_strike = gaps
        .iter()
        .enumerate()
        .min_by(|a, b| a.1.total_cmp(b.1))
        .map(|(i, _)| i)
        .unwrap();

    let narrowest = gaps[at_strike];
    let widest = gaps.iter().copied().fold(Real::MIN, Real::max);
    assert!(
        widest > 10.0 * narrowest,
        "the grid is near-uniform: gaps run {narrowest} to {widest}"
    );
    assert!(
        (locations[at_strike] - STRIKE.ln()).abs() < 0.05,
        "the narrowest gap is at {} rather than at ln(strike) {}",
        locations[at_strike],
        STRIKE.ln()
    );
}

/// The cell straddling `ln(strike)` is the only one whose integrand is
/// discontinuous, and it is the cell the gamma rides on. The cell-averaging
/// integral there can fail to converge and fall back to the grid-point value
/// (`fdminnervaluecalculator.rs:128-131`) without surfacing an error, which
/// would silently remove the regularization at exactly that point. A strictly
/// interior seed value proves the average, not the fallback, produced it.
#[test]
fn the_seed_averages_across_the_strike_cell() {
    let market = fixture();
    let mesher = mesher(&market);
    let (x, rhs) = seed(&mesher);

    let partial: Vec<Size> = (0..rhs.size())
        .filter(|&i| rhs[i] > 0.0 && rhs[i] < CASH)
        .collect();
    assert_eq!(
        partial.len(),
        1,
        "expected exactly one averaged cell, found {partial:?}"
    );

    let straddling = partial[0];
    assert!(
        x[straddling] < STRIKE.ln() && STRIKE.ln() < x[straddling + 1],
        "the averaged cell at {} does not straddle ln(strike) {}",
        x[straddling],
        STRIKE.ln()
    );
    assert_eq!(rhs[straddling - 1], CASH);
    assert_eq!(rhs[straddling + 1], 0.0);
}

/// Why the damping steps are there at all. Douglas is second order and
/// oscillates on a payoff this steep; dropping the three implicit-Euler steps
/// leaves those oscillations in the grid and the spline's second derivative
/// reads them, so the gamma misses the oracle. The test name of the C++ case
/// is about the damping, and this is the arm that makes it mean something.
#[test]
fn dropping_the_damping_steps_breaks_the_gamma() {
    let market = fixture();
    let (_, expected_gamma) = analytic(&market);

    let (_, damped) = finite_difference(&market, DAMPING_STEPS);
    let (_, undamped) = finite_difference(&market, 0);

    assert!(
        relative_error(undamped, expected_gamma) > REL_TOL,
        "the undamped roll still meets the oracle: {undamped} against {expected_gamma}"
    );
    assert!(
        relative_error(undamped, expected_gamma) > relative_error(damped, expected_gamma),
        "the undamped roll is no worse than the damped one: {undamped} vs {damped}"
    );
}
