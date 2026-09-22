use super::*;
use crate::processes::{OrnsteinUhlenbeckProcess, StochasticProcessArray};
use crate::quotes::{Quote, SimpleQuote};
use crate::test_support::{Flag, as_observer};

struct Scalar {
    quote: SimpleQuote,
}

impl AsObservable for Scalar {
    fn observable(&self) -> &Observable {
        self.quote.observable()
    }
}

impl StochasticProcess1D for Scalar {
    fn x0(&self) -> QlResult<Real> {
        self.quote.value()
    }
    fn drift(&self, _t: Time, _x: Real) -> QlResult<Real> {
        self.quote.value()
    }
    fn diffusion(&self, _t: Time, _x: Real) -> QlResult<Real> {
        Ok(0.5)
    }
    fn apply(&self, x: Real, dx: Real) -> Real {
        x * dx.exp()
    }
    fn time(&self, date: &Date) -> QlResult<Time> {
        Ok((*date - Date::min_date()) as Real / 365.0)
    }
    fn evolve(&self, _t: Time, _x: Real, _dt: Time, _dw: Real) -> QlResult<Real> {
        Ok(77.0)
    }
}

struct Scaled;

impl ProcessDiscretization1D for Scaled {
    fn drift(&self, p: &dyn StochasticProcess1D, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Ok(2.0 * ProcessDiscretization1D::drift(&EulerDiscretization, p, t, x, dt)?)
    }
    fn diffusion(&self, p: &dyn StochasticProcess1D, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Ok(3.0 * ProcessDiscretization1D::diffusion(&EulerDiscretization, p, t, x, dt)?)
    }
    fn variance(&self, p: &dyn StochasticProcess1D, t: Time, x: Real, dt: Time) -> QlResult<Real> {
        Ok(9.0 * ProcessDiscretization1D::variance(&EulerDiscretization, p, t, x, dt)?)
    }
}

impl ProcessDiscretization for Scaled {
    fn drift(&self, p: &dyn StochasticProcess, t: Time, x: &Array, dt: Time) -> QlResult<Array> {
        Ok(&ProcessDiscretization::drift(&EulerDiscretization, p, t, x, dt)? * 2.0)
    }
    fn diffusion(
        &self,
        p: &dyn StochasticProcess,
        t: Time,
        x: &Array,
        dt: Time,
    ) -> QlResult<Matrix> {
        Ok(&ProcessDiscretization::diffusion(&EulerDiscretization, p, t, x, dt)? * 3.0)
    }
    fn covariance(
        &self,
        p: &dyn StochasticProcess,
        t: Time,
        x: &Array,
        dt: Time,
    ) -> QlResult<Matrix> {
        Ok(&ProcessDiscretization::covariance(&EulerDiscretization, p, t, x, dt)? * 9.0)
    }
}

#[test]
fn scalar_policy_controls_transitions_and_preserves_source() {
    let source = shared(Scalar {
        quote: SimpleQuote::new(0.2),
    });
    let euler = DiscretizedProcess1D::new(source.clone());
    let custom = DiscretizedProcess1D::with_discretization(source.clone(), shared(Scaled));
    let (t, x, dt, dw) = (1.0, 2.0, 0.25, -0.5);
    assert_eq!(euler.expectation(t, x, dt).unwrap(), x * 0.05_f64.exp());
    assert_eq!(custom.expectation(t, x, dt).unwrap(), x * 0.1_f64.exp());
    assert_eq!(custom.std_deviation(t, x, dt).unwrap(), 0.75);
    assert_eq!(custom.variance(t, x, dt).unwrap(), 0.5625);
    assert_eq!(
        custom.evolve(t, x, dt, dw).unwrap(),
        x * 0.1_f64.exp() * (-0.375_f64).exp()
    );
    assert_eq!(source.evolve(t, x, dt, dw).unwrap(), 77.0);
    assert_eq!(custom.drift(t, x).unwrap(), 0.2);
    assert_eq!(custom.diffusion(t, x).unwrap(), 0.5);
}

#[test]
fn exact_ornstein_uhlenbeck_override_remains_unchanged() {
    let source = shared(OrnsteinUhlenbeckProcess::new(0.5, 0.3, 1.0, 2.0).unwrap());
    let adapter = DiscretizedProcess1D::new(source.clone());
    let exact = 2.0 - (-0.5_f64).exp();
    assert_eq!(source.expectation(0.0, 1.0, 1.0).unwrap(), exact);
    assert_eq!(adapter.expectation(0.0, 1.0, 1.0).unwrap(), 1.5);
    assert_eq!(adapter.variance(0.0, 1.0, 1.0).unwrap(), 0.09);
    assert!(
        (source.variance(0.0, 1.0, 1.0).unwrap() - 0.09 * (1.0 - (-1.0_f64).exp())).abs() < 1e-15
    );
}

#[test]
fn retained_source_notifies_and_recovers_after_coefficient_error() {
    let source = shared(Scalar {
        quote: SimpleQuote::new(0.2),
    });
    let strategy = shared(Scaled);
    let weak_source = Shared::downgrade(&source);
    let weak_strategy = Shared::downgrade(&strategy);
    let adapter = DiscretizedProcess1D::with_discretization(source.clone(), strategy.clone());
    let flag = Flag::new();
    adapter.observable().register_observer(&as_observer(&flag));
    source.quote.reset();
    assert!(Flag::is_up(&flag));
    assert!(adapter.x0().is_err());
    assert!(adapter.expectation(0.0, 2.0, 0.25).is_err());
    source.quote.set_value(0.4);
    drop(source);
    drop(strategy);
    assert_eq!(adapter.x0().unwrap(), 0.4);
    assert_eq!(
        adapter.expectation(0.0, 2.0, 0.25).unwrap(),
        2.0 * 0.2_f64.exp()
    );
    assert!(weak_strategy.upgrade().is_some());
    drop(adapter);
    assert!(weak_source.upgrade().is_none());
    assert!(weak_strategy.upgrade().is_none());
}

struct Multi(Observable);

impl AsObservable for Multi {
    fn observable(&self) -> &Observable {
        &self.0
    }
}

impl StochasticProcess for Multi {
    fn size(&self) -> Size {
        2
    }
    fn factors(&self) -> Size {
        3
    }
    fn initial_values(&self) -> QlResult<Array> {
        Ok(vec![10.0, 20.0].into())
    }
    fn drift(&self, _t: Time, _x: &Array) -> QlResult<Array> {
        Ok(vec![2.0, -1.0].into())
    }
    fn diffusion(&self, _t: Time, _x: &Array) -> QlResult<Matrix> {
        let mut m = Matrix::with_size(2, 3);
        m[0].copy_from_slice(&[1.0, 2.0, 0.0]);
        m[1].copy_from_slice(&[0.0, 1.0, 3.0]);
        Ok(m)
    }
    fn time(&self, _date: &Date) -> QlResult<Time> {
        Ok(7.25)
    }
    fn apply(&self, x: &Array, dx: &Array) -> Array {
        x + &(dx * 2.0)
    }
    fn evolve(&self, _t: Time, _x: &Array, _dt: Time, _dw: &Array) -> QlResult<Array> {
        Ok(vec![77.0, 88.0].into())
    }
}

#[test]
fn rectangular_multifactor_policy_and_custom_apply() {
    let source = shared(Multi(Observable::new()));
    let euler = DiscretizedProcess::new(source.clone());
    let custom = DiscretizedProcess::with_discretization(source.clone(), shared(Scaled));
    let x = source.initial_values().unwrap();
    let dw = vec![0.5, -1.0, 2.0].into();
    assert_eq!(custom.size(), 2);
    assert_eq!(custom.factors(), 3);
    assert_eq!(custom.initial_values().unwrap(), x);
    assert_eq!(custom.time(&Date::min_date()).unwrap(), 7.25);
    assert_eq!(
        euler.expectation(0.0, &x, 0.25).unwrap(),
        vec![11.0, 19.5].into()
    );
    assert_eq!(
        custom.expectation(0.0, &x, 0.25).unwrap(),
        vec![12.0, 19.0].into()
    );
    assert_eq!(
        custom.std_deviation(0.0, &x, 0.25).unwrap()[0],
        [1.5, 3.0, 0.0]
    );
    let cov = custom.covariance(0.0, &x, 0.25).unwrap();
    assert_eq!(cov[0], [11.25, 4.5]);
    assert_eq!(cov[1], [4.5, 22.5]);
    assert_eq!(
        custom.evolve(0.0, &x, 0.25, &dw).unwrap(),
        vec![7.5, 34.0].into()
    );
    assert_eq!(
        source.evolve(0.0, &x, 0.25, &dw).unwrap(),
        vec![77.0, 88.0].into()
    );
}

#[test]
fn array_delegates_time_and_constituent_strategy() {
    let source = shared(Scalar {
        quote: SimpleQuote::new(0.2),
    });
    let strategy = shared(Scaled);
    let weak_strategy = Shared::downgrade(&strategy);
    let weak_source = Shared::downgrade(&source);
    let custom = shared(DiscretizedProcess1D::with_discretization(
        source.clone(),
        strategy,
    ));
    let weak_adapter = Shared::downgrade(&custom);
    let correlation = Matrix::filled(1, 1, 1.0);
    let array = StochasticProcessArray::new(vec![custom.clone()], &correlation).unwrap();
    let x = vec![2.0].into();
    let dw = vec![-0.5].into();
    assert_eq!(array.time(&(Date::min_date() + 365)).unwrap(), 1.0);
    assert_eq!(
        array.evolve(0.0, &x, 0.25, &dw).unwrap()[0],
        custom.evolve(0.0, 2.0, 0.25, -0.5).unwrap()
    );
    assert_eq!(array.covariance(0.0, &x, 0.25).unwrap()[0][0], 0.5625);
    let flag = Flag::new();
    array.observable().register_observer(&as_observer(&flag));
    source.quote.set_value(0.4);
    assert!(Flag::is_up(&flag));
    let exact = StochasticProcessArray::new(vec![source], &correlation).unwrap();
    assert_eq!(exact.evolve(0.0, &x, 0.25, &dw).unwrap()[0], 77.0);
    let ou = shared(OrnsteinUhlenbeckProcess::new(0.5, 0.3, 1.0, 2.0).unwrap());
    let mut two = Matrix::with_size(2, 2);
    two[0][0] = 1.0;
    two[1][1] = 1.0;
    let mixed = StochasticProcessArray::new(vec![custom.clone(), ou.clone()], &two).unwrap();
    assert_eq!(mixed.time(&(Date::min_date() + 365)).unwrap(), 1.0);
    drop(mixed);
    let no_time = StochasticProcessArray::new(vec![ou], &correlation).unwrap();
    assert!(
        no_time
            .time(&Date::min_date())
            .unwrap_err()
            .message()
            .contains("date/time conversion")
    );
    drop(custom);
    assert!(weak_adapter.upgrade().is_some());
    drop(array);
    assert!(weak_adapter.upgrade().is_none());
    assert!(weak_strategy.upgrade().is_none());
    assert!(weak_source.upgrade().is_some());
    drop(exact);
    assert!(weak_source.upgrade().is_none());
}

struct BadPolicy(u8);

impl ProcessDiscretization for BadPolicy {
    fn drift(
        &self,
        _p: &dyn StochasticProcess,
        _t: Time,
        _x: &Array,
        _dt: Time,
    ) -> QlResult<Array> {
        match self.0 {
            0 => Ok(vec![1.0].into()),
            1 => Ok(vec![f64::NAN, 0.0].into()),
            2 => crate::fail!("custom strategy failure"),
            _ => Ok(vec![0.0, 0.0].into()),
        }
    }
    fn diffusion(
        &self,
        _p: &dyn StochasticProcess,
        _t: Time,
        _x: &Array,
        _dt: Time,
    ) -> QlResult<Matrix> {
        Ok(Matrix::filled(
            2,
            if self.0 == 0 { 2 } else { 3 },
            if self.0 == 1 { f64::NAN } else { 0.0 },
        ))
    }
    fn covariance(
        &self,
        _p: &dyn StochasticProcess,
        _t: Time,
        _x: &Array,
        _dt: Time,
    ) -> QlResult<Matrix> {
        Ok(Matrix::filled(
            2,
            if self.0 == 0 { 3 } else { 2 },
            if self.0 == 1 { f64::INFINITY } else { 0.0 },
        ))
    }
}

impl ProcessDiscretization1D for BadPolicy {
    fn drift(&self, _p: &dyn StochasticProcess1D, _t: Time, _x: Real, _dt: Time) -> QlResult<Real> {
        crate::fail!("custom scalar strategy failure")
    }
    fn diffusion(
        &self,
        _p: &dyn StochasticProcess1D,
        _t: Time,
        _x: Real,
        _dt: Time,
    ) -> QlResult<Real> {
        Ok(f64::INFINITY)
    }
    fn variance(
        &self,
        _p: &dyn StochasticProcess1D,
        _t: Time,
        _x: Real,
        _dt: Time,
    ) -> QlResult<Real> {
        Ok(-1.0)
    }
}

#[test]
fn strategy_errors_dimensions_and_nonfinite_results_are_fallible() {
    let source = shared(Multi(Observable::new()));
    let x = source.initial_values().unwrap();
    for mode in [0, 1] {
        let adapter =
            DiscretizedProcess::with_discretization(source.clone(), shared(BadPolicy(mode)));
        assert!(adapter.expectation(0.0, &x, 1.0).is_err());
        assert!(adapter.std_deviation(0.0, &x, 1.0).is_err());
        assert!(adapter.covariance(0.0, &x, 1.0).is_err());
        assert!(adapter.evolve(0.0, &x, 1.0, &Array::with_size(3)).is_err());
    }
    let adapter = DiscretizedProcess::with_discretization(source, shared(BadPolicy(2)));
    assert_eq!(
        adapter.expectation(0.0, &x, 1.0).unwrap_err().message(),
        "custom strategy failure"
    );
    let scalar = DiscretizedProcess1D::with_discretization(
        shared(Scalar {
            quote: SimpleQuote::new(0.2),
        }),
        shared(BadPolicy(0)),
    );
    assert_eq!(
        scalar.expectation(0.0, 1.0, 1.0).unwrap_err().message(),
        "custom scalar strategy failure"
    );
    assert!(scalar.std_deviation(0.0, 1.0, 1.0).is_err());
    assert!(
        scalar
            .variance(0.0, 1.0, 1.0)
            .unwrap_err()
            .message()
            .contains("negative")
    );
}

#[test]
fn invalid_transition_inputs_fail_before_process_arithmetic() {
    let source = shared(Multi(Observable::new()));
    let adapter = DiscretizedProcess::new(source);
    let x = adapter.initial_values().unwrap();
    assert!(adapter.expectation(0.0, &Array::with_size(1), 1.0).is_err());
    assert!(
        adapter
            .expectation(0.0, &vec![f64::NAN, 0.0].into(), 1.0)
            .is_err()
    );
    assert!(adapter.evolve(0.0, &x, 1.0, &Array::with_size(2)).is_err());
    assert!(
        adapter
            .evolve(0.0, &x, 1.0, &Array::filled(3, f64::INFINITY))
            .is_err()
    );
    let scalar = DiscretizedProcess1D::new(shared(Scalar {
        quote: SimpleQuote::new(0.2),
    }));
    for dt in [-0.1, f64::NAN, f64::INFINITY] {
        assert!(scalar.expectation(0.0, 1.0, dt).is_err());
        assert!(scalar.std_deviation(0.0, 1.0, dt).is_err());
        assert!(scalar.variance(0.0, 1.0, dt).is_err());
        assert!(adapter.expectation(0.0, &x, dt).is_err());
        assert!(adapter.std_deviation(0.0, &x, dt).is_err());
        assert!(adapter.covariance(0.0, &x, dt).is_err());
    }
    assert!(scalar.expectation(f64::NAN, 1.0, 1.0).is_err());
    assert!(scalar.expectation(0.0, f64::NAN, 1.0).is_err());
    assert!(scalar.evolve(0.0, 1.0, 1.0, f64::NAN).is_err());
    assert_eq!(scalar.evolve(0.0, 2.0, 0.0, 1.0).unwrap(), 2.0);
    assert_eq!(
        adapter
            .evolve(0.0, &x, 0.0, &Array::filled(3, 1.0))
            .unwrap(),
        x
    );
}

#[test]
fn multifactor_retention_and_observation_share_the_original_process() {
    let source = shared(Multi(Observable::new()));
    let strategy = shared(Scaled);
    let weak_source = Shared::downgrade(&source);
    let weak_strategy = Shared::downgrade(&strategy);
    let adapter = DiscretizedProcess::with_discretization(source.clone(), strategy.clone());
    let flag = Flag::new();
    adapter.observable().register_observer(&as_observer(&flag));
    source.observable().notify_observers();
    assert!(Flag::is_up(&flag));
    drop(source);
    drop(strategy);
    assert_eq!(adapter.initial_values().unwrap(), vec![10.0, 20.0].into());
    assert!(weak_source.upgrade().is_some());
    assert!(weak_strategy.upgrade().is_some());
    drop(adapter);
    assert!(weak_source.upgrade().is_none());
    assert!(weak_strategy.upgrade().is_none());
}
