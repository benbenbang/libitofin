use super::*;
use std::ptr::null_mut;

#[test]
fn native_quantlib_euler_fixture_cases() {
    let csv = include_str!("../../../sdk/go/testdata/geometric-brownian/cases.csv");
    let mut count = 0;
    for row in csv.lines().skip(1) {
        let fields: Vec<_> = row.split(',').collect();
        assert_eq!(fields.len(), 15);
        let a: Vec<f64> = fields[1..].iter().map(|v| v.parse().unwrap()).collect();
        let mut c = Context::new();
        let mut id = 0;
        assert_eq!(
            unsafe { itofin_geometric_brownian_new(&mut c, a[0], a[1], a[2], &mut id, null_mut()) },
            0
        );
        for (kind, index) in [(0, 7), (3, 8), (4, 9), (5, 10), (6, 11), (7, 12), (8, 13)] {
            let mut out = 91.;
            assert_eq!(
                unsafe {
                    itofin_geometric_brownian_query(
                        &mut c,
                        id,
                        kind,
                        a[3],
                        a[4],
                        a[5],
                        a[6],
                        &mut out,
                        null_mut(),
                    )
                },
                0
            );
            let expected = a[index];
            let tolerance = 2e-14_f64.max(3e-12 * expected.abs());
            assert!(out.is_finite() && expected.is_finite());
            assert!(
                (out - expected).abs() <= tolerance,
                "{} kind {kind}: {out} vs {expected}",
                fields[0]
            );
        }
        count += 1;
    }
    assert_eq!(count, 9);
}
