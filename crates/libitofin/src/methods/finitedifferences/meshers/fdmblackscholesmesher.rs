//! 1-D mesher for the Black-Scholes process, in `ln(S)`.
//!
//! Port of `ql/methods/finitedifferences/meshers/fdmblackscholesmesher.hpp:39`
//! and its `.cpp:36-148`.

use crate::cashflows::Dividend;
use crate::errors::QlResult;
use crate::handle::Handle;
use crate::math::distributions::normal::InverseCumulativeNormal;
use crate::processes::GeneralizedBlackScholesProcess;
use crate::quotes::Quote;
use crate::shared::{Shared, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::termstructures::volatility::{BlackConstantVol, BlackVolTermStructure};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::types::{Real, Size, Time, Volatility};
use crate::{fail, require};

use super::concentrating1dmesher::concentrating_1d_mesher;
use super::fdm1dmesher::Fdm1dMesher;
use super::uniform1dmesher::uniform_1d_mesher;

/// The `ln(S)` grid an equity finite-difference engine prices on.
///
/// The bounds come from the range the forward travels over `[0, maturity]`,
/// widened either way by `normInvEps * scaleFactor * sigma * sqrt(maturity)`
/// (`cpp:96-102`). The forward is rolled through the discount-factor ratios of
/// the risk-free and dividend curves at a set of intermediate times - the
/// dividend dates that fall in `(0, maturity]` plus `max(2, 24 * maturity)`
/// equally spaced steps (`cpp:51-93`) - and each discrete dividend is
/// subtracted as it is passed, so both the pre-dividend high and the
/// post-dividend low enter the range. `x_min_constraint` / `x_max_constraint`
/// override the computed bounds outright.
///
/// With `c_point` set to a spot the grid can bracket, the points concentrate
/// around its logarithm; otherwise they are equally spaced (`cpp:111-124`).
///
/// Divergences from C++, all at the API boundary:
///
/// - the C++ constructor of an `Fdm1dMesher` subclass that adds no members
///   becomes a constructor function, as for
///   [`uniform_1d_mesher`](super::uniform_1d_mesher);
/// - `x_min_constraint`, `x_max_constraint` and `c_point` are [`Option`]
///   rather than `Null<Real>` sentinels, following
///   [`concentrating_1d_mesher`](super::concentrating_1d_mesher);
/// - the process is borrowed rather than shared: the mesher only reads it
///   while building the grid and keeps nothing.
///
/// Deferred to #636: the quanto branch (`cpp:66-73`), which replaces the
/// dividend curve with a `QuantoTermStructure` when an `FdmQuantoHelper` is
/// given. Neither that helper nor that term structure is ported yet, so the
/// parameter is omitted rather than accepted and ignored, and the dividend
/// curve here is always the process's own.
///
/// # Errors
///
/// Returns an error unless the spot is strictly positive, and propagates the
/// failures of the curves it reads and of the grid it delegates to.
#[allow(clippy::too_many_arguments, clippy::neg_cmp_op_on_partial_ord)]
pub fn fdm_black_scholes_mesher(
    size: Size,
    process: &GeneralizedBlackScholesProcess,
    maturity: Time,
    strike: Real,
    x_min_constraint: Option<Real>,
    x_max_constraint: Option<Real>,
    eps: Real,
    scale_factor: Real,
    c_point: Option<(Real, Real)>,
    dividend_schedule: &[Shared<dyn Dividend>],
    spot_adjustment: Real,
) -> QlResult<Fdm1dMesher> {
    let spot = process.x0()?;
    require!(spot > 0.0, "negative or null underlying given");

    let mut intermediate_steps: Vec<(Time, Real)> = Vec::new();
    for dividend in dividend_schedule {
        let t = process.time(&dividend.date())?;
        if t <= maturity && t >= 0.0 {
            intermediate_steps.push((t, dividend.amount()?));
        }
    }

    let intermediate_time_steps = ((24.0 * maturity) as Size).max(2);
    for i in 0..intermediate_time_steps {
        let t = (i + 1) as Real * (maturity / intermediate_time_steps as Real);
        intermediate_steps.push((t, 0.0));
    }

    intermediate_steps.sort_by(|a, b| a.0.total_cmp(&b.0).then(a.1.total_cmp(&b.1)));

    let r_ts = process.risk_free_rate().current_link()?;
    let q_ts = process.dividend_yield().current_link()?;

    let mut last_div_time: Time = 0.0;
    let mut fwd = spot + spot_adjustment;
    let mut mi = fwd;
    let mut ma = fwd;

    for &(div_time, div_amount) in &intermediate_steps {
        fwd = fwd / r_ts.discount(div_time, false)?
            * r_ts.discount(last_div_time, false)?
            * q_ts.discount(div_time, false)?
            / q_ts.discount(last_div_time, false)?;

        mi = mi.min(fwd);
        ma = ma.max(fwd);

        fwd -= div_amount;

        mi = mi.min(fwd);
        ma = ma.max(fwd);

        last_div_time = div_time;
    }

    let norm_inv_eps = InverseCumulativeNormal::standard_value(1.0 - eps)?;
    let sigma_sqrt_t = process
        .black_volatility()
        .current_link()?
        .black_vol(maturity, strike, false)?
        * maturity.sqrt();

    let mut x_min = mi.ln() - sigma_sqrt_t * norm_inv_eps * scale_factor;
    let mut x_max = ma.ln() + sigma_sqrt_t * norm_inv_eps * scale_factor;

    if let Some(constraint) = x_min_constraint {
        x_min = constraint;
    }
    if let Some(constraint) = x_max_constraint {
        x_max = constraint;
    }

    match c_point {
        Some((point, density)) if point.ln() >= x_min && point.ln() <= x_max => {
            concentrating_1d_mesher(x_min, x_max, size, Some((point.ln(), density)), false)
        }
        _ => uniform_1d_mesher(x_min, x_max, size),
    }
}

/// A Black-Scholes process on flat curves and a constant volatility, the
/// market the mesher's own tests and the FD engines' shortcuts are built on.
///
/// Port of the static `FdmBlackScholesMesher::processHelper`
/// (`cpp:133-148`). The volatility curve takes its reference date and
/// day-count convention from `r_ts`, so an `r_ts` without a day counter is an
/// error here where C++ would carry an empty one into `BlackConstantVol`.
///
/// # Errors
///
/// Returns an error if `r_ts` is an empty handle or carries no day counter.
pub fn process_helper(
    s0: Handle<dyn Quote>,
    r_ts: Handle<dyn YieldTermStructure>,
    q_ts: Handle<dyn YieldTermStructure>,
    vol: Volatility,
) -> QlResult<GeneralizedBlackScholesProcess> {
    let curve = r_ts.current_link()?;
    let reference_date = curve.reference_date()?;
    let Some(day_counter) = curve.day_counter() else {
        fail!("no day counter provided for the risk-free curve");
    };

    let black_vol = Handle::new(shared(BlackConstantVol::new(
        reference_date,
        None,
        vol,
        day_counter,
    )) as Shared<dyn BlackVolTermStructure>);

    Ok(GeneralizedBlackScholesProcess::new(
        s0, q_ts, r_ts, black_vol,
    ))
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::cashflows::FixedDividend;
    use crate::interestrate::Compounding;
    use crate::quotes::make_quote_handle;
    use crate::termstructures::yields::FlatForward;
    use crate::time::date::{Date, Month};
    use crate::time::daycounter::DayCounter;
    use crate::time::daycounters::actual365fixed::Actual365Fixed;
    use crate::time::frequency::Frequency;
    use crate::time::period::Period;
    use crate::time::timeunit::TimeUnit;
    use crate::types::Rate;

    fn flat_rate(reference: Date, rate: Rate, dc: DayCounter) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference,
            rate,
            dc,
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn flat_vol(
        reference: Date,
        vol: Volatility,
        dc: DayCounter,
    ) -> Handle<dyn BlackVolTermStructure> {
        Handle::new(shared(BlackConstantVol::new(reference, None, vol, dc))
            as Shared<dyn BlackVolTermStructure>)
    }

    /// `testHighInterestRateBlackScholesMesher`, `fdmlinearop.cpp:1495-1556`.
    ///
    /// With `r` well above `q` the forward only rises, so the lower bound is
    /// the spot pushed down by the volatility term and the upper bound is the
    /// forward at maturity pushed up by it.
    #[test]
    fn grid_bounds_under_a_high_interest_rate() {
        let dc = Actual365Fixed::new();
        let today = Date::new(11, Month::February, 2018);

        let spot = 100.0;
        let r = 0.21;
        let q = 0.02;
        let v = 0.25;

        let process = GeneralizedBlackScholesProcess::new(
            make_quote_handle(spot).handle(),
            flat_rate(today, q, dc.clone()),
            flat_rate(today, r, dc.clone()),
            flat_vol(today, v, dc),
        );

        let size = 10;
        let maturity = 2.0;
        let strike = 100.0;
        let eps = 0.05;
        let norm_inv_eps = 1.64485363;
        let scale_factor = 2.5;

        let mesher = fdm_black_scholes_mesher(
            size,
            &process,
            maturity,
            strike,
            None,
            None,
            eps,
            scale_factor,
            None,
            &[],
            0.0,
        )
        .unwrap();
        let locations = mesher.locations();

        let calculated_min = locations[0].exp();
        let calculated_max = locations[size - 1].exp();

        let minimum = spot * (-norm_inv_eps * scale_factor * v * maturity.sqrt()).exp();
        let maximum = spot
            / process
                .risk_free_rate()
                .current_link()
                .unwrap()
                .discount(maturity, false)
                .unwrap()
            * process
                .dividend_yield()
                .current_link()
                .unwrap()
                .discount(maturity, false)
                .unwrap()
            * (norm_inv_eps * scale_factor * v * maturity.sqrt()).exp();

        let rel_tol = 1e-7;
        assert!((calculated_max - maximum).abs() <= rel_tol * maximum);
        assert!((calculated_min - minimum).abs() <= rel_tol * minimum);
    }

    /// `testLowVolatilityHighDiscreteDividendBlackScholesMesher`,
    /// `fdmlinearop.cpp:1558-1620`.
    ///
    /// A zero volatility collapses the widening term, so the bounds are the
    /// forward's own extremes: the peak just before the first dividend, and
    /// the trough just after the second.
    #[test]
    fn grid_bounds_under_discrete_dividends_and_no_volatility() {
        let dc = Actual365Fixed::new();
        let today = Date::new(28, Month::January, 2018);

        let spot = make_quote_handle(100.0);
        let q_ts = flat_rate(today, 0.07, dc.clone());
        let r_ts = flat_rate(today, 0.16, dc.clone());

        let process = GeneralizedBlackScholesProcess::new(
            spot.handle(),
            q_ts.clone(),
            r_ts.clone(),
            flat_vol(today, 0.0, dc),
        );

        let first_div_date = today + Period::new(7, TimeUnit::Months);
        let first_div_amount = 10.0;
        let second_div_date = today + Period::new(11, TimeUnit::Months);
        let second_div_amount = 5.0;

        let dividends: Vec<Shared<dyn Dividend>> = vec![
            shared(FixedDividend::new(first_div_amount, first_div_date)),
            shared(FixedDividend::new(second_div_amount, second_div_date)),
        ];

        let size = 5;
        let mesher = fdm_black_scholes_mesher(
            size, &process, 1.0, 100.0, None, None, 0.0001, 1.5, None, &dividends, 0.0,
        )
        .unwrap();
        let locations = mesher.locations();

        let spot_value = spot.handle().current_link().unwrap().value().unwrap();
        let discount_ratio = |date: Date| {
            q_ts.current_link()
                .unwrap()
                .discount_date(date, false)
                .unwrap()
                / r_ts
                    .current_link()
                    .unwrap()
                    .discount_date(date, false)
                    .unwrap()
        };

        let maximum = spot_value * discount_ratio(first_div_date);
        let minimum = (1.0 - first_div_amount / (spot_value * discount_ratio(first_div_date)))
            * spot_value
            * discount_ratio(second_div_date)
            - second_div_amount;

        let calculated_max = locations[size - 1].exp();
        let calculated_min = locations[0].exp();

        let rel_tol = 1e5 * Real::EPSILON;
        assert!((calculated_max - maximum).abs() <= rel_tol * maximum);
        assert!((calculated_min - minimum).abs() <= rel_tol * minimum);
    }

    #[test]
    fn spot_must_be_positive() {
        let dc = Actual365Fixed::new();
        let today = Date::new(11, Month::February, 2018);
        let process = GeneralizedBlackScholesProcess::new(
            make_quote_handle(0.0).handle(),
            flat_rate(today, 0.02, dc.clone()),
            flat_rate(today, 0.05, dc.clone()),
            flat_vol(today, 0.25, dc),
        );

        let err = fdm_black_scholes_mesher(
            5,
            &process,
            1.0,
            100.0,
            None,
            None,
            0.0001,
            1.5,
            None,
            &[],
            0.0,
        )
        .unwrap_err();
        assert_eq!(err.message(), "negative or null underlying given");
    }
}
