use super::*;

fn rational(value: &str) -> f64 {
    let (n, d) = value.split_once('/').unwrap_or((value, "1"));
    n.parse::<f64>().unwrap() / d.parse::<f64>().unwrap()
}

#[test]
fn adx_independent_step_fixture_and_transition_seed() {
    let rows: Vec<Vec<f64>> = include_str!("../../../tests/fixtures/chart_adx.csv")
        .lines()
        .skip(1)
        .map(|line| line.split(',').map(rational).collect())
        .collect();
    let high: Vec<_> = rows.iter().map(|row| row[0]).collect();
    let low: Vec<_> = rows.iter().map(|row| row[1]).collect();
    let close: Vec<_> = rows.iter().map(|row| row[2]).collect();
    let raw = true_range(&high, &low, &close).unwrap();
    let result = adx(&high, &low, &close, 2).unwrap();
    for (channel, series) in [&result.plus_di, &result.minus_di, &result.dx, &result.adx]
        .into_iter()
        .enumerate()
    {
        assert_eq!(series.first_valid, if channel == 3 { 3 } else { 2 });
        for (i, row) in rows.iter().enumerate() {
            assert!((series.values[i] - row[9 + channel]).abs() < 1e-12);
            assert_eq!(series.get(i).is_some(), i >= series.first_valid);
        }
    }
    let plus: Vec<_> = rows.iter().map(|row| row[4]).collect();
    let minus: Vec<_> = rows.iter().map(|row| row[5]).collect();
    for (column, values) in [(6, raw.values), (7, plus), (8, minus)] {
        let mean = wilder(&values, 2, 1).unwrap();
        for (i, row) in rows.iter().enumerate() {
            assert!((mean.values[i] - row[column]).abs() < 1e-12);
        }
    }
    assert_eq!(rows[4][4..6], [0.0, 0.0]);
    assert_ne!(
        super::super::atr(&high, &low, &close, 2).unwrap().values[2],
        rows[2][6]
    );
}

#[test]
fn adx_rising_falling_flat_and_zero_sum() {
    for sign in [1.0, -1.0] {
        let close: Vec<_> = (0..8).map(|i| sign * i as f64).collect();
        let result = adx(&close, &close, &close, 2).unwrap();
        assert_eq!(&result.dx.values[2..], &[100.0; 6]);
        assert_eq!(&result.adx.values[3..], &[100.0; 5]);
        let (active, other) = if sign > 0.0 {
            (result.plus_di, result.minus_di)
        } else {
            (result.minus_di, result.plus_di)
        };
        assert_eq!(&active.values[2..], &[100.0; 6]);
        assert_eq!(other.values, [0.0; 8]);
    }
    for (high, low) in [(-3.0, -3.0), (2.0, -4.0)] {
        let result = adx(&[high; 8], &[low; 8], &[-3.0; 8], 2).unwrap();
        for series in [result.plus_di, result.minus_di, result.dx, result.adx] {
            assert_eq!(series.values, [0.0; 8]);
        }
    }
}

#[test]
fn adx_default_period_one_empty_short_and_large_period() {
    let close: Vec<_> = (0..30).map(|i| i as f64).collect();
    let default = adx_default(&close, &close, &close).unwrap();
    assert_eq!(default, adx(&close, &close, &close, 14).unwrap());
    assert_eq!(default.dx.first_valid, 14);
    assert_eq!(default.adx.first_valid, 27);
    for n in 0..5 {
        let result = adx(&close[..n], &close[..n], &close[..n], 2).unwrap();
        assert_eq!(result.dx.first_valid, 2.min(n));
        assert_eq!(result.adx.first_valid, 3.min(n));
    }
    let huge = adx(&close, &close, &close, usize::MAX).unwrap();
    assert_eq!(huge.adx.first_valid, 30);
    assert_eq!(huge.dx.values, [0.0; 30]);
    let one = adx(&[0.0, f64::MAX, f64::MIN_POSITIVE], &[0.0; 3], &[0.0; 3], 1).unwrap();
    assert_eq!(one.dx.values, [0.0, 100.0, 0.0]);
    assert_eq!(one.adx, one.dx);
    let max = adx(&[0.0, f64::MAX, f64::MAX], &[0.0; 3], &[0.0; 3], 2).unwrap();
    assert!(max.plus_di.values[2].is_finite());
}

#[test]
fn adx_rejects_all_invalid_differences_even_during_warmup() {
    assert!(adx(&[], &[], &[], 0).is_err());
    for (h, l, c) in [
        (vec![], vec![0.0], vec![1.0]),
        (vec![2.0], vec![], vec![1.0]),
        (vec![2.0], vec![0.0], vec![]),
        (vec![f64::NAN], vec![0.0], vec![1.0]),
        (vec![2.0], vec![f64::NEG_INFINITY], vec![1.0]),
        (vec![2.0], vec![0.0], vec![f64::INFINITY]),
        (vec![2.0], vec![3.0], vec![1.0]),
        (vec![f64::MAX], vec![-f64::MAX], vec![0.0]),
        (
            vec![f64::MAX, -f64::MAX],
            vec![0.0, -f64::MAX],
            vec![0.0, -f64::MAX],
        ),
        (
            vec![f64::MAX, 0.0],
            vec![f64::MAX, -f64::MAX],
            vec![f64::MAX, 0.0],
        ),
    ] {
        assert!(adx(&h, &l, &c, 14).is_err());
    }
}
