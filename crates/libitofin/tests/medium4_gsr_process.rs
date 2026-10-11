use libitofin::patterns::observable::{AsObservable, Observer};
use libitofin::processes::{ForwardMeasureProcess1D, GsrProcess};
use libitofin::shared::{SharedMut, shared_mut};
use libitofin::stochasticprocess::StochasticProcess1D;

fn process(a: f64, sigma: f64, horizon: f64) -> GsrProcess {
    GsrProcess::new(vec![], vec![sigma], vec![a], horizon).unwrap()
}

fn close(actual: f64, expected: f64) {
    assert!(
        (actual - expected).abs() <= 2e-13 * expected.abs().max(f64::MIN_POSITIVE),
        "actual {actual:.17e}, expected {expected:.17e}"
    );
}

#[test]
fn independent_high_precision_moments() {
    for line in include_str!("fixtures/medium4_gsr_process/reference.tsv")
        .lines()
        .skip(1)
    {
        let row = line.split('\t').collect::<Vec<_>>();
        let v = row[1..]
            .iter()
            .map(|value| value.parse::<f64>().unwrap())
            .collect::<Vec<_>>();
        let p = process(v[0], v[1], v[2]);
        let dynamic: &dyn StochasticProcess1D = &p;
        close(dynamic.drift(v[3], v[4]).unwrap(), v[6]);
        close(p.g(v[3], v[2]).unwrap(), v[7]);
        close(p.y(v[3]).unwrap(), v[8]);
        close(dynamic.expectation(v[3], v[4], v[5]).unwrap(), v[9]);
        close(dynamic.variance(v[3], v[4], v[5]).unwrap(), v[10]);
        close(
            dynamic.std_deviation(v[3], v[4], v[5]).unwrap(),
            v[10].sqrt(),
        );
        close(
            dynamic.evolve(v[3], v[4], v[5], 0.75).unwrap(),
            v[9] + 0.75 * v[10].sqrt(),
        );
        assert_eq!(p.sigma(v[3]).unwrap(), v[1]);
        assert_eq!(p.reversion(v[3]).unwrap(), v[0]);
        assert_eq!(p.x0().unwrap(), 0.0);
    }
}

#[test]
fn repeated_constant_grids_and_transition_semigroup() {
    for a in [-0.03, -1e-14, 0.0, 1e-14, 0.03] {
        let p = process(a, 0.01, 10.0);
        let grid = GsrProcess::new(vec![0.0, 1.0, 12.0], vec![0.01; 4], vec![a; 4], 10.0).unwrap();
        close(
            p.expectation(0.5, 0.03, 2.0).unwrap(),
            grid.expectation(0.5, 0.03, 2.0).unwrap(),
        );
        let first_mean = p.expectation(0.5, 0.03, 0.75).unwrap();
        let composed_mean = p.expectation(1.25, first_mean, 1.25).unwrap();
        close(composed_mean, p.expectation(0.5, 0.03, 2.0).unwrap());
        let composed_variance = (-2.0 * a * 1.25).exp() * p.variance(0.5, 0.03, 0.75).unwrap()
            + p.variance(1.25, first_mean, 1.25).unwrap();
        close(composed_variance, p.variance(0.5, 0.03, 2.0).unwrap());
    }
}

struct Counter(usize);

impl Observer for Counter {
    fn update(&mut self) {
        self.0 += 1;
    }
}

#[test]
fn forward_measure_updates_change_drift_and_mean_and_notify() {
    let mut p = process(0.03, 0.01, 10.0);
    let count = shared_mut(Counter(0));
    p.observable()
        .register_observer(&(count.clone() as SharedMut<dyn Observer>));
    let before = p.expectation(0.5, 0.03, 2.0).unwrap();
    let variance = p.variance(0.5, 0.03, 2.0).unwrap();
    let after = process(0.03, 0.01, 5.0);
    let dynamic: &mut dyn ForwardMeasureProcess1D = &mut p;
    dynamic.set_forward_measure_time(5.0).unwrap();
    assert_eq!(count.borrow().0, 1);
    assert_ne!(dynamic.expectation(0.5, 0.03, 2.0).unwrap(), before);
    close(
        dynamic.expectation(0.5, 0.03, 2.0).unwrap(),
        after.expectation(0.5, 0.03, 2.0).unwrap(),
    );
    close(
        dynamic.drift(0.5, 0.03).unwrap(),
        after.drift(0.5, 0.03).unwrap(),
    );
    assert_eq!(dynamic.variance(0.5, 0.03, 2.0).unwrap(), variance);
    dynamic.set_forward_measure_time(5.0).unwrap();
    assert_eq!(count.borrow().0, 2);
    for invalid in [-1.0, f64::NAN, f64::INFINITY] {
        assert!(dynamic.set_forward_measure_time(invalid).is_err());
        assert_eq!(dynamic.forward_measure_time(), 5.0);
        assert_eq!(count.borrow().0, 2);
    }
    dynamic.set_forward_measure_time(0.0).unwrap();
    assert_eq!(dynamic.expectation(0.0, 0.03, 0.0).unwrap(), 0.03);
    assert!(dynamic.diffusion(0.001, 0.0).is_err());
}

#[test]
fn constructor_rejects_bad_lengths_grids_and_varying_parameters() {
    for (times, vols, reversions) in [
        (vec![], vec![], vec![]),
        (vec![1.0], vec![0.01], vec![0.03]),
        (vec![], vec![0.01], vec![]),
        (vec![], vec![0.01], vec![0.03; 2]),
        (vec![1.0], vec![0.01, 0.02], vec![0.03]),
        (vec![1.0], vec![0.01; 2], vec![0.03, 0.04]),
        (vec![-1.0], vec![0.01; 2], vec![0.03]),
        (vec![f64::NAN], vec![0.01; 2], vec![0.03]),
        (vec![f64::INFINITY], vec![0.01; 2], vec![0.03]),
        (vec![1.0, 1.0], vec![0.01; 3], vec![0.03]),
        (vec![2.0, 1.0], vec![0.01; 3], vec![0.03]),
    ] {
        assert!(GsrProcess::new(times, vols, reversions, 10.0).is_err());
    }
    for bad in [-0.01, f64::NAN, f64::INFINITY] {
        assert!(GsrProcess::new(vec![], vec![bad], vec![0.03], 10.0).is_err());
        assert!(GsrProcess::new(vec![], vec![0.01], vec![0.03], bad).is_err());
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(GsrProcess::new(vec![], vec![0.01], vec![bad], 10.0).is_err());
    }
}

#[test]
fn invalid_dynamics_are_errors_not_nan_or_negative_variance() {
    let p = process(0.03, 0.01, 10.0);
    for bad in [-1.0, 11.0, f64::NAN, f64::INFINITY] {
        assert!(p.drift(bad, 0.0).is_err());
        assert!(p.diffusion(bad, 0.0).is_err());
        assert!(p.sigma(bad).is_err());
        assert!(p.reversion(bad).is_err());
        assert!(p.y(bad).is_err());
        assert!(p.g(0.0, bad).is_err());
        assert!(p.expectation(bad, 0.0, 0.0).is_err());
        assert!(p.variance(0.0, 0.0, bad).is_err());
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(p.drift(0.0, bad).is_err());
        assert!(p.diffusion(0.0, bad).is_err());
        assert!(p.expectation(0.0, bad, 0.0).is_err());
        assert!(p.variance(0.0, bad, 0.0).is_err());
        assert!(p.evolve(0.0, 0.0, 0.0, bad).is_err());
    }
    assert!(p.g(2.0, 1.0).is_err());
    assert!(p.expectation(9.0, 0.0, 2.0).is_err());
    assert!(p.variance(9.0, 0.0, 2.0).is_err());
    assert!(
        process(-1000.0, 0.01, 10.0)
            .variance(0.0, 0.0, 1.0)
            .is_err()
    );
    assert!(
        process(-1000.0, 0.0, 10.0)
            .expectation(0.0, 1.0, 1.0)
            .is_err()
    );
    assert!(
        process(0.0, 1e154, 10.0)
            .evolve(0.0, 0.0, 1.0, f64::MAX)
            .is_err()
    );
}

#[test]
fn zero_sigma_zero_steps_and_small_nonzero_steps() {
    for a in [-0.03, 0.0, 0.03] {
        let p = process(a, 0.0, 10.0);
        assert_eq!(p.y(10.0).unwrap(), 0.0);
        assert_eq!(p.variance(0.0, 1.0, 10.0).unwrap(), 0.0);
        close(p.expectation(0.0, 1.0, 10.0).unwrap(), (-a * 10.0).exp());
        assert_eq!(p.expectation(10.0, 0.123, 0.0).unwrap(), 0.123);
        assert_eq!(p.g(10.0, 10.0).unwrap(), 0.0);
    }
    let p = process(0.0, 0.01, 10.0);
    assert!(p.variance(0.0, 0.0, f64::EPSILON / 2.0).unwrap() > 0.0);
    assert_eq!(p.variance(10.0, 0.0, 0.0).unwrap(), 0.0);
    assert_eq!(p.evolve(10.0, 0.123, 0.0, 7.0).unwrap(), 0.123);
}

#[test]
fn variance_scaling_avoids_intermediate_overflow_and_underflow() {
    close(
        process(f64::MAX, 0.5, 1.0).y(1.0).unwrap(),
        0.125 / f64::MAX,
    );
    close(process(0.0, 1e200, 1.0).y(1e-200).unwrap(), 1e200);
    close(process(0.0, 1e-200, 1e200).y(1e200).unwrap(), 1e-200);
    close(
        process(-1000.0, 1e-300, 1.0)
            .variance(0.0, 0.0, 1.0)
            .unwrap(),
        1.9405900971421843e265,
    );
}
