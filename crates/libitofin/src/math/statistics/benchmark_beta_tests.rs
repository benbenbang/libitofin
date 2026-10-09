use super::*;

#[test]
fn independent_fraction_fixtures() {
    for line in include_str!("../../../../../sdk/go/testdata/benchmark-beta.csv")
        .lines()
        .skip(1)
    {
        let cells: Vec<_> = line.split(';').collect();
        let parse = |s: &str| {
            s.split(',')
                .map(|v| v.parse::<f64>().unwrap())
                .collect::<Vec<_>>()
        };
        let asset = parse(cells[1]);
        let benchmark = parse(cells[2]);
        let weights = (!cells[3].is_empty()).then(|| parse(cells[3]));
        let expected = cells[4].parse::<f64>().unwrap();
        let actual = benchmark_beta(&asset, &benchmark, weights.as_deref()).unwrap();
        assert!(
            (actual - expected).abs() <= 2e-12,
            "{}: {actual} != {expected}",
            cells[0]
        );
    }
}

#[test]
fn rejects_invalid_shapes_weights_and_moments() {
    let b = [1.0, 2.0];
    for (a, b, w) in [
        (vec![], vec![], None),
        (vec![1.0], vec![2.0], None),
        (vec![1.0, 2.0], vec![1.0], None),
        (vec![1.0, 2.0], vec![3.0, 3.0], None),
        (vec![f64::NAN, 2.0], b.to_vec(), None),
        (b.to_vec(), vec![f64::INFINITY, 2.0], None),
        (b.to_vec(), b.to_vec(), Some(vec![1.0])),
        (b.to_vec(), b.to_vec(), Some(vec![0.0, 0.0])),
        (b.to_vec(), b.to_vec(), Some(vec![-1.0, 2.0])),
        (b.to_vec(), b.to_vec(), Some(vec![f64::NAN, 1.0])),
        (b.to_vec(), b.to_vec(), Some(vec![f64::INFINITY, 1.0])),
        (b.to_vec(), b.to_vec(), Some(vec![f64::MAX, f64::MAX])),
        (b.to_vec(), b.to_vec(), Some(vec![1.0, 0.0])),
        (vec![f64::NAN, 2.0], b.to_vec(), Some(vec![0.0, 1.0])),
        (vec![f64::MAX, -f64::MAX], b.to_vec(), None),
        (b.to_vec(), vec![1e-200, 2e-200], None),
    ] {
        assert!(benchmark_beta(&a, &b, w.as_deref()).is_err());
    }
    let huge = vec![0.0; super::super::MAX_SEQUENCE_ROWS + 1];
    assert!(benchmark_beta(&huge, &huge, None).is_err());
}

#[test]
fn offset_and_weight_scale_invariance_and_input_ownership() {
    let a = [1e12 + 1.0, 1e12 + 3.0, 1e12 + 2.0];
    let b = [1e12 - 1.0, 1e12, 1e12 + 2.0];
    let original = (a, b);
    for w in [
        [1.0, 2.0, 3.0],
        [1e100, 2e100, 3e100],
        [1e-100, 2e-100, 3e-100],
    ] {
        assert!((benchmark_beta(&a, &b, Some(&w)).unwrap() - 1.0 / 53.0).abs() < 2e-12);
    }
    assert_eq!((a, b), original);
}
