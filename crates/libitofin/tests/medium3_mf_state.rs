use libitofin::math::array::Array;
use libitofin::processes::MfStateProcess;
use libitofin::stochasticprocess::StochasticProcess1D;

fn array(text: &str) -> Array {
    if text == "-" {
        Array::new()
    } else {
        Array::from(
            text.split(';')
                .map(|v| v.parse().unwrap())
                .collect::<Vec<f64>>(),
        )
    }
}

fn close(actual: f64, expected: f64, name: &str) {
    assert!(
        (actual - expected).abs() <= 2e-13 * expected.abs().max(f64::MIN_POSITIVE),
        "{name}: actual {actual:.17e}, expected {expected:.17e}"
    );
}

#[test]
fn source_derived_high_precision_transitions() {
    for line in include_str!("fixtures/medium3_mf_state/reference.tsv")
        .lines()
        .skip(1)
    {
        let row = line.split('\t').collect::<Vec<_>>();
        let p = MfStateProcess::new(row[1].parse().unwrap(), array(row[2]), array(row[3])).unwrap();
        let t0 = row[4].parse().unwrap();
        let dt = row[5].parse().unwrap();
        close(
            p.variance(t0, 3.0, dt).unwrap(),
            row[6].parse().unwrap(),
            row[0],
        );
        close(
            p.std_deviation(t0, -2.0, dt).unwrap(),
            row[7].parse().unwrap(),
            row[0],
        );
        assert_eq!(p.expectation(t0, -2.0, dt).unwrap(), -2.0);
        close(
            p.evolve(t0, 0.0, dt, 1.0).unwrap(),
            row[7].parse().unwrap(),
            row[0],
        );
    }
}

#[test]
fn native_bucket_conventions_and_accessors() {
    let p = MfStateProcess::new(
        0.7,
        Array::from([0.0, 1.0, 2.0]),
        Array::from([0.9, 0.2, 0.3, 0.4]),
    )
    .unwrap();
    assert_eq!(p.reversion(), 0.7);
    assert_eq!(p.times(), &[0.0, 1.0, 2.0]);
    assert_eq!(p.volatilities(), &[0.9, 0.2, 0.3, 0.4]);
    let dynamic: &dyn StochasticProcess1D = &p;
    assert_eq!(dynamic.x0().unwrap(), 0.0);
    assert_eq!(dynamic.drift(2.0, 7.0).unwrap(), 0.0);
    for (t, expected) in [
        (0.0, 0.2),
        (0.999, 0.2),
        (1.0, 0.3),
        (2.0, 0.4),
        (10.0, 0.4),
    ] {
        assert_eq!(dynamic.diffusion(t, -7.0).unwrap(), expected);
    }
    let empty = MfStateProcess::new(0.0, Array::new(), Array::from([0.0])).unwrap();
    assert_eq!(empty.diffusion(0.0, 0.0).unwrap(), 0.0);
    assert_eq!(empty.variance(0.7, 0.0, 0.2).unwrap(), 0.2);
    assert_eq!(empty.variance(0.0, 0.0, f64::EPSILON / 2.0).unwrap(), 0.0);
    assert_eq!(
        empty.variance(0.0, 0.0, f64::EPSILON).unwrap(),
        f64::EPSILON
    );
}

#[test]
fn exact_gaussian_transition_additivity_and_zero_volatility() {
    for a in [-0.4, 0.0, 0.4] {
        let p =
            MfStateProcess::new(a, Array::from([1.0, 2.0]), Array::from([0.2, 0.0, 0.4])).unwrap();
        close(
            p.variance(0.0, 0.0, 3.0).unwrap(),
            p.variance(0.0, 4.0, 1.0).unwrap() + p.variance(1.0, -3.0, 2.0).unwrap(),
            "additivity",
        );
        assert_eq!(p.variance(1.0, 0.0, 1.0).unwrap(), 0.0);
        assert_eq!(p.evolve(1.0, 7.0, 1.0, -4.0).unwrap(), 7.0);
        assert_eq!(p.variance(0.0, 0.0, 0.0).unwrap(), 0.0);
    }
}

#[test]
fn constructor_rejects_invalid_inputs() {
    for a in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(MfStateProcess::new(a, Array::new(), Array::from([0.2])).is_err());
    }
    for (times, vols) in [
        (vec![], vec![]),
        (vec![1.0], vec![0.2]),
        (vec![1.0, 1.0], vec![0.2; 3]),
        (vec![2.0, 1.0], vec![0.2; 3]),
        (vec![-1.0], vec![0.2; 2]),
        (vec![f64::NAN], vec![0.2; 2]),
        (vec![f64::INFINITY], vec![0.2; 2]),
        (vec![], vec![-0.1]),
        (vec![], vec![f64::NAN]),
        (vec![], vec![f64::INFINITY]),
    ] {
        assert!(MfStateProcess::new(0.0, Array::from(times), Array::from(vols)).is_err());
    }
}

#[test]
fn invalid_dynamics_and_unrepresentable_results_are_errors() {
    let p = MfStateProcess::new(0.3, Array::from([1.0]), Array::from([0.2, 0.3])).unwrap();
    for bad in [-0.1, f64::NAN, f64::INFINITY] {
        assert!(p.drift(bad, 0.0).is_err());
        assert!(p.diffusion(bad, 0.0).is_err());
        assert!(p.expectation(bad, 0.0, 0.0).is_err());
        assert!(p.variance(0.0, 0.0, bad).is_err());
        assert!(p.std_deviation(bad, 0.0, 0.0).is_err());
        assert!(p.evolve(0.0, 0.0, bad, 1.0).is_err());
    }
    for bad in [f64::NAN, f64::INFINITY, f64::NEG_INFINITY] {
        assert!(p.drift(0.0, bad).is_err());
        assert!(p.diffusion(0.0, bad).is_err());
        assert!(p.expectation(0.0, bad, 0.0).is_err());
        assert!(p.variance(0.0, bad, 0.0).is_err());
        assert!(p.evolve(0.0, 0.0, 0.0, bad).is_err());
    }
    assert!(p.variance(f64::MAX, 0.0, f64::MAX).is_err());
    assert!(p.variance(2000.0, 0.0, 1.0).is_err());
    let huge = MfStateProcess::new(0.0, Array::from([2.0]), Array::from([1e154; 2])).unwrap();
    assert!(huge.evolve(0.0, 0.0, 1.0, f64::MAX).is_err());
    let decaying = MfStateProcess::new(-f64::MAX, Array::new(), Array::from([0.2])).unwrap();
    close(
        decaying.variance(0.0, 0.0, 1.0).unwrap(),
        0.5 / f64::MAX,
        "extreme signed reversion",
    );
}

#[test]
fn compiled_original_quantlib_transitions() {
    for line in include_str!("fixtures/medium3_mf_state/native.tsv")
        .lines()
        .skip(1)
    {
        let row = line.split('\t').collect::<Vec<_>>();
        let (times, vols) = if row[6] == "1" {
            (Array::new(), Array::from([0.2]))
        } else {
            (Array::from([0.5, 1.5]), Array::from([0.2, 0.4, 0.1]))
        };
        let p = MfStateProcess::new(row[0].parse().unwrap(), times, vols).unwrap();
        let t0 = row[1].parse().unwrap();
        let dt = row[2].parse().unwrap();
        assert_eq!(
            p.diffusion(t0, 7.0).unwrap(),
            row[3].parse::<f64>().unwrap()
        );
        close(
            p.variance(t0, 7.0, dt).unwrap(),
            row[4].parse().unwrap(),
            line,
        );
        close(
            p.std_deviation(t0, 7.0, dt).unwrap(),
            row[5].parse().unwrap(),
            line,
        );
    }
}

#[test]
fn rounded_endpoints_use_native_binary64_time_grid() {
    let t0 = 1e20;
    let dt = 1.0;
    assert_eq!(t0 + dt, t0);
    for a in [0.0, -0.3, 0.3] {
        let bucketed = MfStateProcess::new(a, Array::from([1.0]), Array::from([0.2; 2])).unwrap();
        assert_eq!(bucketed.variance(t0, 0.0, dt).unwrap(), 0.0);
        let empty = MfStateProcess::new(a, Array::new(), Array::from([0.2])).unwrap();
        assert_eq!(
            empty.variance(t0, 0.0, dt).unwrap(),
            if a == 0.0 { dt } else { 0.0 }
        );
    }
}
