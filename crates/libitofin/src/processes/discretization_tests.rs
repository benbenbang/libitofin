use super::*;
use crate::quotes::{Quote, SimpleQuote};

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
