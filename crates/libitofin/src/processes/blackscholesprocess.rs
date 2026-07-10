//! Black-Scholes processes.
//!
//! Port of `ql/processes/blackscholesprocess.{hpp,cpp}`:
//! [`GeneralizedBlackScholesProcess`] describes the stochastic process
//! `d ln S(t) = (r(t) - q(t) - sigma(t, S)^2 / 2) dt + sigma dW_t`, built
//! from a spot quote handle plus risk-free, dividend and Black-volatility
//! term-structure handles. While the interface is expressed in terms of `S`,
//! the internal calculations work on `ln S`, so
//! [`apply`](crate::stochasticprocess::StochasticProcess1D::apply) composes
//! multiplicatively, exactly as in C++.
//!
//! The local volatility is derived lazily from the Black volatility and
//! cached until an input notification invalidates it (the C++ `update()`
//! resets `updated_` before notifying, mirrored here by the invalidating
//! observer). Only the `BlackConstantVol` shortcut of the C++ dispatch is
//! ported: it marks the process strike-independent and enables the exact
//! curve formulas for `expectation` / `variance` / `evolve`. The
//! `BlackVarianceCurve` shortcut and the strike-dependent `LocalVolSurface`
//! fallback follow with EPIC-4, so a non-constant Black volatility without an
//! external local volatility is an `Err` for now instead of silently wrong.
//!
//! Not ported, noted as follow-up: the pluggable `discretization` strategy
//! and `forceDiscretization` flag (the Euler scheme is the trait's provided
//! default, see [`crate::stochasticprocess`]), and the sibling conveniences
//! `BlackScholesProcess` (its dividend-free curve needs a D5 settings
//! decision), `BlackProcess` and `GarmanKohlagenProcess`.

use std::any::Any;
use std::cell::Cell;

use crate::errors::QlResult;
use crate::fail;
use crate::handle::{Handle, RelinkableHandle};
use crate::interestrate::Compounding;
use crate::patterns::observable::{AsObservable, Observable, Observer, ResetThenNotify};
use crate::quotes::Quote;
use crate::require;
use crate::shared::{Shared, SharedMut, shared};
use crate::stochasticprocess::StochasticProcess1D;
use crate::termstructures::TermStructure;
use crate::termstructures::volatility::{
    BlackConstantVol, BlackVolTermStructure, LocalConstantVol, LocalVolTermStructure,
};
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::time::date::Date;
use crate::time::frequency::Frequency;
use crate::types::{Rate, Real, Time};

/// Generalized Black-Scholes stochastic process.
pub struct GeneralizedBlackScholesProcess {
    x0: Handle<dyn Quote>,
    risk_free_rate: Handle<dyn YieldTermStructure>,
    dividend_yield: Handle<dyn YieldTermStructure>,
    black_volatility: Handle<dyn BlackVolTermStructure>,
    external_local_vol: Option<Handle<dyn LocalVolTermStructure>>,
    local_volatility: RelinkableHandle<dyn LocalVolTermStructure>,
    updated: Shared<Cell<bool>>,
    is_strike_independent: Cell<bool>,
    observable: Shared<Observable>,
    _listener: SharedMut<ResetThenNotify>,
}

/// Merton (1973) extension to the Black-Scholes stochastic process: a stock
/// or stock index paying a continuous dividend yield. Identical to the
/// generalized process, as in C++ where the subclass adds nothing.
pub type BlackScholesMertonProcess = GeneralizedBlackScholesProcess;

impl GeneralizedBlackScholesProcess {
    fn assemble(
        x0: Handle<dyn Quote>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        black_volatility: Handle<dyn BlackVolTermStructure>,
        external_local_vol: Option<Handle<dyn LocalVolTermStructure>>,
    ) -> GeneralizedBlackScholesProcess {
        let updated = shared(Cell::new(false));
        let observable = shared(Observable::new());
        let listener = ResetThenNotify::broadcasting(Shared::clone(&observable), {
            let updated = Shared::clone(&updated);
            move || updated.set(false)
        });
        let observer = listener.clone() as SharedMut<dyn Observer>;
        x0.register_observer(&observer);
        risk_free_rate.register_observer(&observer);
        dividend_yield.register_observer(&observer);
        black_volatility.register_observer(&observer);
        if let Some(local_vol) = &external_local_vol {
            local_vol.register_observer(&observer);
        }
        GeneralizedBlackScholesProcess {
            x0,
            risk_free_rate,
            dividend_yield,
            black_volatility,
            external_local_vol,
            local_volatility: RelinkableHandle::empty(),
            updated,
            is_strike_independent: Cell::new(false),
            observable,
            _listener: listener,
        }
    }

    /// Process deriving its local volatility from the Black volatility.
    pub fn new(
        x0: Handle<dyn Quote>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        black_volatility: Handle<dyn BlackVolTermStructure>,
    ) -> GeneralizedBlackScholesProcess {
        Self::assemble(x0, dividend_yield, risk_free_rate, black_volatility, None)
    }

    /// Process with an externally supplied local volatility.
    pub fn with_local_vol(
        x0: Handle<dyn Quote>,
        dividend_yield: Handle<dyn YieldTermStructure>,
        risk_free_rate: Handle<dyn YieldTermStructure>,
        black_volatility: Handle<dyn BlackVolTermStructure>,
        local_vol: Handle<dyn LocalVolTermStructure>,
    ) -> GeneralizedBlackScholesProcess {
        Self::assemble(
            x0,
            dividend_yield,
            risk_free_rate,
            black_volatility,
            Some(local_vol),
        )
    }

    /// The spot quote handle.
    pub fn state_variable(&self) -> Handle<dyn Quote> {
        self.x0.clone()
    }

    /// The dividend-yield curve handle.
    pub fn dividend_yield(&self) -> Handle<dyn YieldTermStructure> {
        self.dividend_yield.clone()
    }

    /// The risk-free-rate curve handle.
    pub fn risk_free_rate(&self) -> Handle<dyn YieldTermStructure> {
        self.risk_free_rate.clone()
    }

    /// The Black-volatility curve handle.
    pub fn black_volatility(&self) -> Handle<dyn BlackVolTermStructure> {
        self.black_volatility.clone()
    }

    /// The local-volatility curve, derived from the Black volatility and
    /// cached until an input changes (or the external one, when supplied).
    pub fn local_volatility(&self) -> QlResult<Handle<dyn LocalVolTermStructure>> {
        if let Some(external) = &self.external_local_vol {
            return Ok(external.clone());
        }
        if !self.updated.get() {
            self.is_strike_independent.set(true);
            let black_vol = self.black_volatility.current_link()?;
            let Some(constant_vol) = (&*black_vol as &dyn Any).downcast_ref::<BlackConstantVol>()
            else {
                self.is_strike_independent.set(false);
                fail!(
                    "only constant Black volatilities are supported; the local-volatility \
                     surface follows with EPIC-4"
                );
            };
            let reference_date = constant_vol.reference_date()?;
            let vol = constant_vol.black_vol(0.0, self.x0()?, false)?;
            let Some(day_counter) = constant_vol.day_counter() else {
                fail!("no day counter provided for the constant Black volatility");
            };
            self.local_volatility.link_to(shared(LocalConstantVol::new(
                reference_date,
                vol,
                day_counter,
            )) as Shared<dyn LocalVolTermStructure>);
            self.updated.set(true);
        }
        Ok(self.local_volatility.handle())
    }

    fn forward(
        &self,
        curve: &Handle<dyn YieldTermStructure>,
        t1: Time,
        t2: Time,
    ) -> QlResult<Rate> {
        Ok(curve
            .current_link()?
            .forward_rate(
                t1,
                t2,
                Compounding::Continuous,
                Frequency::NoFrequency,
                true,
            )?
            .rate())
    }

    fn exact_growth_rate(&self, t0: Time, dt: Time) -> QlResult<Rate> {
        let r = self.forward(&self.risk_free_rate, t0, t0 + dt)?;
        let q = self.forward(&self.dividend_yield, t0, t0 + dt)?;
        Ok(r - q)
    }
}

/// Reject a non-finite process argument.
///
/// Divergence: `blackscholesprocess.cpp` contains no `QL_REQUIRE` at all. A
/// non-finite `t` or `x` reaches the volatility curve, whose own range checks
/// pass it through, and the caller receives a NaN drift or diffusion. Under D10
/// the port fails at the boundary instead.
fn check_finite(name: &str, value: Real) -> QlResult<()> {
    if !value.is_finite() {
        fail!("{name} ({value}) must be finite");
    }
    Ok(())
}

/// Reject a negative or non-finite evolution step.
///
/// Divergence: as for [`check_finite`]. A negative `dt` makes `variance`
/// negative and `std_deviation` returns NaN from the square root.
fn check_time_step(dt: Time) -> QlResult<()> {
    if !dt.is_finite() || dt < 0.0 {
        fail!("dt ({dt}) must be non-negative");
    }
    Ok(())
}

impl AsObservable for GeneralizedBlackScholesProcess {
    fn observable(&self) -> &Observable {
        &self.observable
    }
}

impl StochasticProcess1D for GeneralizedBlackScholesProcess {
    fn x0(&self) -> QlResult<Real> {
        self.x0.current_link()?.value()
    }

    fn drift(&self, t: Time, x: Real) -> QlResult<Real> {
        check_finite("t", t)?;
        check_finite("x", x)?;
        let sigma = self.diffusion(t, x)?;
        let t1 = t + 0.0001;
        let r = self.forward(&self.risk_free_rate, t, t1)?;
        let q = self.forward(&self.dividend_yield, t, t1)?;
        Ok(r - q - 0.5 * sigma * sigma)
    }

    fn diffusion(&self, t: Time, x: Real) -> QlResult<Real> {
        check_finite("t", t)?;
        check_finite("x", x)?;
        self.local_volatility()?
            .current_link()?
            .local_vol(t, x, true)
    }

    fn expectation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        check_finite("t0", t0)?;
        check_finite("x0", x0)?;
        check_time_step(dt)?;
        self.local_volatility()?;
        require!(self.is_strike_independent.get(), "not implemented");
        Ok(x0 * (dt * self.exact_growth_rate(t0, dt)?).exp())
    }

    fn std_deviation(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        check_finite("t0", t0)?;
        check_finite("x0", x0)?;
        check_time_step(dt)?;
        self.local_volatility()?;
        if self.is_strike_independent.get() {
            Ok(self.variance(t0, x0, dt)?.sqrt())
        } else {
            Ok(self.diffusion(t0, x0)? * dt.sqrt())
        }
    }

    fn variance(&self, t0: Time, x0: Real, dt: Time) -> QlResult<Real> {
        check_finite("t0", t0)?;
        check_finite("x0", x0)?;
        check_time_step(dt)?;
        self.local_volatility()?;
        if self.is_strike_independent.get() {
            let vol = self.black_volatility.current_link()?;
            Ok(vol.black_variance(t0 + dt, 0.01, false)? - vol.black_variance(t0, 0.01, false)?)
        } else {
            let sigma = self.diffusion(t0, x0)?;
            Ok(sigma * sigma * dt)
        }
    }

    fn evolve(&self, t0: Time, x0: Real, dt: Time, dw: Real) -> QlResult<Real> {
        check_finite("t0", t0)?;
        check_finite("x0", x0)?;
        check_time_step(dt)?;
        check_finite("dw", dw)?;
        self.local_volatility()?;
        if self.is_strike_independent.get() {
            let var = self.variance(t0, x0, dt)?;
            let drift = self.exact_growth_rate(t0, dt)? * dt - 0.5 * var;
            Ok(self.apply(x0, var.sqrt() * dw + drift))
        } else {
            let drift = self.drift(t0, x0)? * dt;
            Ok(self.apply(x0, drift + self.std_deviation(t0, x0, dt)? * dw))
        }
    }

    fn apply(&self, x0: Real, dx: Real) -> Real {
        x0 * dx.exp()
    }

    fn time(&self, date: &Date) -> QlResult<Time> {
        self.risk_free_rate
            .current_link()?
            .time_from_reference(*date)
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    use crate::quotes::make_quote_handle;
    use crate::termstructures::TermStructureBase;
    use crate::termstructures::volatility::VolatilityTermStructure;
    use crate::termstructures::yields::FlatForward;
    use crate::test_support::{Flag, as_observer};
    use crate::time::businessdayconvention::BusinessDayConvention;
    use crate::time::calendars::target::Target;
    use crate::time::date::Month;
    use crate::time::daycounters::actual360::Actual360;
    use crate::types::Volatility;

    const SPOT: Real = 100.0;
    const Q: Rate = 0.04;
    const R: Rate = 0.06;
    const VOL: Volatility = 0.20;

    fn reference() -> Date {
        Date::new(15, Month::June, 2026)
    }

    fn flat_yield(rate: Rate) -> Handle<dyn YieldTermStructure> {
        Handle::new(shared(FlatForward::with_rate(
            reference(),
            rate,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>)
    }

    fn constant_vol(vol: Volatility) -> Shared<dyn BlackVolTermStructure> {
        shared(BlackConstantVol::new(
            reference(),
            Some(Target::new()),
            vol,
            Actual360::new(),
        ))
    }

    struct Fixture {
        spot: RelinkableHandle<dyn Quote>,
        vol: RelinkableHandle<dyn BlackVolTermStructure>,
        process: GeneralizedBlackScholesProcess,
    }

    /// The flat-curve market of `test-suite/europeanoption.cpp`: spot 100,
    /// q 4%, r 6%, vol 20%, Actual360.
    fn european_option_fixture() -> Fixture {
        let spot = make_quote_handle(SPOT);
        let vol = RelinkableHandle::new(constant_vol(VOL));
        let process = BlackScholesMertonProcess::new(
            spot.handle(),
            flat_yield(Q),
            flat_yield(R),
            vol.handle(),
        );
        Fixture { spot, vol, process }
    }

    #[test]
    fn x0_reads_the_spot_quote() {
        let f = european_option_fixture();
        assert_eq!(f.process.x0().unwrap(), SPOT);

        let empty = GeneralizedBlackScholesProcess::new(
            Handle::empty(),
            flat_yield(Q),
            flat_yield(R),
            RelinkableHandle::new(constant_vol(VOL)).handle(),
        );
        assert!(empty.x0().is_err());
    }

    #[test]
    fn diffusion_is_the_constant_black_vol() {
        let f = european_option_fixture();
        for t in [0.0, 0.25, 1.0, 5.0] {
            for x in [50.0, 100.0, 200.0] {
                assert_eq!(f.process.diffusion(t, x).unwrap(), VOL);
            }
        }
    }

    /// The instantaneous forward is recovered over `dt = 0.0001`, which
    /// amplifies the `exp`/`ln` representation error by `1/dt`; 1e-11 is the
    /// honest tolerance for the recipe (the C++ shares it).
    #[test]
    fn drift_matches_the_flat_curves() {
        let f = european_option_fixture();
        let expected = R - Q - 0.5 * VOL * VOL;
        for t in [0.0, 0.5, 2.0] {
            let drift = f.process.drift(t, SPOT).unwrap();
            assert!(
                (drift - expected).abs() < 1e-11,
                "t={t} drift={drift} expected={expected}"
            );
        }
    }

    #[test]
    fn expectation_variance_and_evolve_use_the_exact_curve_formulas() {
        let f = european_option_fixture();
        let (t0, x0, dt, dw): (Time, Real, Time, Real) = (0.25, SPOT, 0.5, 0.5);

        let expectation = f.process.expectation(t0, x0, dt).unwrap();
        assert!((expectation - x0 * ((R - Q) * dt).exp()).abs() < 1e-12);

        let var = f.process.variance(t0, x0, dt).unwrap();
        assert!((var - VOL * VOL * dt).abs() < 1e-14);
        let std_dev = f.process.std_deviation(t0, x0, dt).unwrap();
        assert!((std_dev - var.sqrt()).abs() < 1e-15);

        let evolved = f.process.evolve(t0, x0, dt, dw).unwrap();
        let expected = x0 * ((R - Q) * dt - 0.5 * var + var.sqrt() * dw).exp();
        assert!((evolved - expected).abs() < 1e-12);
    }

    #[test]
    fn process_rejects_non_finite_inputs_and_negative_time_steps() {
        let f = european_option_fixture();

        assert!(f.process.drift(Time::INFINITY, SPOT).is_err());
        assert!(f.process.diffusion(0.25, Real::NAN).is_err());
        assert!(f.process.expectation(0.25, SPOT, -0.5).is_err());
        assert!(f.process.variance(0.25, SPOT, Time::INFINITY).is_err());
        assert!(f.process.std_deviation(0.25, Real::INFINITY, 0.5).is_err());
        assert!(f.process.evolve(0.25, SPOT, 0.5, Real::NAN).is_err());
    }

    #[test]
    fn apply_composes_multiplicatively() {
        let f = european_option_fixture();
        let doubled = f.process.apply(SPOT, 2.0_f64.ln());
        assert!((doubled - 2.0 * SPOT).abs() < 1e-12);
    }

    #[test]
    fn time_uses_the_risk_free_day_counter() {
        let f = european_option_fixture();
        assert_eq!(f.process.time(&(reference() + 180)).unwrap(), 0.5);
    }

    #[test]
    fn local_volatility_is_cached_and_invalidated_by_input_changes() {
        let f = european_option_fixture();
        assert_eq!(f.process.diffusion(1.0, SPOT).unwrap(), VOL);
        let first = f
            .process
            .local_volatility()
            .unwrap()
            .current_link()
            .unwrap();
        let second = f
            .process
            .local_volatility()
            .unwrap()
            .current_link()
            .unwrap();
        assert!(
            Shared::ptr_eq(&first, &second),
            "repeated queries must reuse the cached local volatility"
        );

        f.vol.link_to(constant_vol(0.25));
        assert_eq!(f.process.diffusion(1.0, SPOT).unwrap(), 0.25);
    }

    #[test]
    fn input_changes_notify_process_observers() {
        let f = european_option_fixture();
        let flag = Flag::new();
        f.process
            .observable()
            .register_observer(&as_observer(&flag));

        f.spot
            .link_to(shared(crate::quotes::SimpleQuote::new(105.0)) as Shared<dyn Quote>);
        assert!(Flag::is_up(&flag), "spot relink must notify");

        Flag::lower(&flag);
        f.vol.link_to(constant_vol(0.3));
        assert!(Flag::is_up(&flag), "vol relink must notify");
    }

    struct TimeDependentVol {
        base: TermStructureBase,
    }

    impl AsObservable for TimeDependentVol {
        fn observable(&self) -> &Observable {
            self.base.observable()
        }
    }

    impl TermStructure for TimeDependentVol {
        fn base(&self) -> &TermStructureBase {
            &self.base
        }

        fn max_date(&self) -> Date {
            Date::max_date()
        }
    }

    impl VolatilityTermStructure for TimeDependentVol {
        fn business_day_convention(&self) -> BusinessDayConvention {
            BusinessDayConvention::Following
        }

        fn min_strike(&self) -> Rate {
            Rate::MIN
        }

        fn max_strike(&self) -> Rate {
            Rate::MAX
        }
    }

    impl BlackVolTermStructure for TimeDependentVol {
        fn black_vol_impl(&self, t: Time, _strike: Real) -> QlResult<Volatility> {
            Ok(0.2 + 0.01 * t)
        }

        fn black_variance_impl(&self, t: Time, strike: Real) -> QlResult<Real> {
            self.variance_from_vol(t, strike)
        }
    }

    #[test]
    fn non_constant_black_vol_without_local_vol_is_an_err() {
        let vol = shared(TimeDependentVol {
            base: TermStructureBase::with_reference_date(
                reference(),
                Some(Target::new()),
                Some(Actual360::new()),
            ),
        }) as Shared<dyn BlackVolTermStructure>;
        let process = GeneralizedBlackScholesProcess::new(
            make_quote_handle(SPOT).handle(),
            flat_yield(Q),
            flat_yield(R),
            Handle::new(vol),
        );

        let err = process.diffusion(1.0, SPOT).unwrap_err();
        assert!(err.message().contains("constant Black volatilities"));
        assert!(process.expectation(0.0, SPOT, 0.5).is_err());
    }

    #[test]
    fn external_local_vol_takes_the_discretized_branch() {
        let local = shared(LocalConstantVol::new(reference(), 0.3, Actual360::new()))
            as Shared<dyn LocalVolTermStructure>;
        let process = GeneralizedBlackScholesProcess::with_local_vol(
            make_quote_handle(SPOT).handle(),
            flat_yield(Q),
            flat_yield(R),
            RelinkableHandle::new(constant_vol(VOL)).handle(),
            Handle::new(local),
        );
        let (t0, x0, dt, dw): (Time, Real, Time, Real) = (0.25, SPOT, 0.5, -1.0);

        assert_eq!(process.diffusion(t0, x0).unwrap(), 0.3);
        let err = process.expectation(t0, x0, dt).unwrap_err();
        assert_eq!(err.message(), "not implemented");

        let std_dev = process.std_deviation(t0, x0, dt).unwrap();
        assert!((std_dev - 0.3 * dt.sqrt()).abs() < 1e-15);
        let var = process.variance(t0, x0, dt).unwrap();
        assert!((var - 0.09 * dt).abs() < 1e-15);

        let evolved = process.evolve(t0, x0, dt, dw).unwrap();
        let expected = x0 * (process.drift(t0, x0).unwrap() * dt + std_dev * dw).exp();
        assert!((evolved - expected).abs() < 1e-12);
    }
}
