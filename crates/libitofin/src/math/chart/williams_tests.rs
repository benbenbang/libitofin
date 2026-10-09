use super::{williams_r, williams_r_default};

#[test]
fn williams_independent_rational_fixture_and_expiring_extrema() {
    let rows: Vec<Vec<f64>> = include_str!("../../../tests/data/chart/williams_r.csv")
        .lines()
        .skip(1)
        .map(|line| {
            line.split(',')
                .map(|value| value.parse().unwrap())
                .collect()
        })
        .collect();
    let high: Vec<_> = rows.iter().map(|row| row[0]).collect();
    let low: Vec<_> = rows.iter().map(|row| row[1]).collect();
    let close: Vec<_> = rows.iter().map(|row| row[2]).collect();
    let result = williams_r(&high, &low, &close, 3).unwrap();
    assert_eq!(result.first_valid, 2);
    for (value, row) in result.values.iter().zip(rows) {
        assert!((value - row[3]).abs() < 1e-12);
    }
    assert_eq!(result.get(1), None);
    assert_eq!(result.get(6), Some(0.0));
    assert_eq!(result.get(9), None);
    for period in 1..=high.len() {
        let actual = williams_r(&high, &low, &close, period).unwrap();
        for (index, value) in actual.values.iter().enumerate().skip(period - 1) {
            let start = index + 1 - period;
            let h = high[start..=index]
                .iter()
                .copied()
                .fold(f64::NEG_INFINITY, f64::max);
            let l = low[start..=index]
                .iter()
                .copied()
                .fold(f64::INFINITY, f64::min);
            let expected = if h == l {
                -50.0
            } else {
                -100.0 * (h - close[index]) / (h - l)
            };
            assert!((value - expected).abs() < 1e-12);
        }
    }
}

#[test]
fn williams_monotone_bounds_flat_negative_and_tiny_windows() {
    let rising = [1.0, 2.0, 3.0, 4.0];
    let falling = [4.0, 3.0, 2.0, 1.0];
    assert_eq!(
        williams_r(&rising, &rising, &rising, 2).unwrap().values,
        [0.0, 0.0, 0.0, 0.0]
    );
    assert_eq!(
        williams_r(&falling, &falling, &falling, 2).unwrap().values,
        [0.0, -100.0, -100.0, -100.0]
    );
    assert_eq!(
        williams_r(&[-3.0; 4], &[-3.0; 4], &[-3.0; 4], 2)
            .unwrap()
            .values,
        [0.0, -50.0, -50.0, -50.0]
    );
    assert_eq!(
        williams_r(&[2.0, 2.0], &[0.0, 0.0], &[2.0, 0.0], 1)
            .unwrap()
            .values,
        [0.0, -100.0]
    );
    let tiny = f64::from_bits(1);
    assert_eq!(
        williams_r(&[tiny], &[0.0], &[0.0], 1).unwrap().values,
        [-100.0]
    );
    assert_eq!(
        williams_r(&[f64::MAX], &[0.0], &[0.0], 1).unwrap().values,
        [-100.0]
    );
}

#[test]
fn williams_default_empty_short_and_huge_period() {
    assert_eq!(williams_r_default(&[], &[], &[]).unwrap().first_valid, 0);
    let short = williams_r_default(&[2.0], &[0.0], &[1.0]).unwrap();
    assert_eq!(short.first_valid, 1);
    assert_eq!(short.values, [0.0]);
    let values = vec![2.0; 15];
    let default = williams_r_default(&values, &values, &values).unwrap();
    assert_eq!(default.first_valid, 13);
    assert_eq!(default, williams_r(&values, &values, &values, 14).unwrap());
    assert_eq!(&default.values[13..], &[-50.0, -50.0]);
    let huge = williams_r(&values, &values, &values, usize::MAX).unwrap();
    assert_eq!(huge.first_valid, 15);
    assert_eq!(huge.values, [0.0; 15]);
}

#[test]
fn williams_rejects_invalid_hlc_period_and_overflow_during_warmup() {
    assert!(williams_r(&[], &[], &[], 0).is_err());
    for (high, low, close) in [
        (vec![], vec![0.0], vec![1.0]),
        (vec![2.0], vec![], vec![1.0]),
        (vec![2.0], vec![0.0], vec![]),
        (vec![f64::NAN], vec![0.0], vec![1.0]),
        (vec![2.0], vec![f64::NEG_INFINITY], vec![1.0]),
        (vec![2.0], vec![0.0], vec![f64::INFINITY]),
        (vec![2.0], vec![3.0], vec![1.0]),
        (vec![2.0], vec![0.0], vec![3.0]),
        (vec![f64::MAX], vec![-f64::MAX], vec![0.0]),
        (
            vec![-f64::MAX, f64::MAX],
            vec![-f64::MAX, f64::MAX],
            vec![-f64::MAX, f64::MAX],
        ),
    ] {
        assert!(williams_r(&high, &low, &close, 14).is_err());
    }
    let flat = [-f64::MAX, f64::MAX];
    assert_eq!(
        williams_r(&flat, &flat, &flat, 1).unwrap().values,
        [-50.0, -50.0]
    );
}
