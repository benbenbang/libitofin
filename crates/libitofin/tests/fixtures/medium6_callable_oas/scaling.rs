use super::*;

#[test]
fn oas_inversion_is_invariant_under_extreme_face_scaling() {
    let s = settings();
    let e = engine(s.clone());
    for kind in ["fixed", "zero"] {
        let row = include_str!("reference.tsv")
            .lines()
            .skip(1)
            .map(|line| line.split('\t').collect::<Vec<_>>())
            .find(|c| c[0] == kind && c[1] == "2" && c[2] == "0")
            .unwrap();
        let price = row[6].parse().unwrap();
        let spread = row[4].parse().unwrap();
        for face in [1e-250, 1e250] {
            let (actual, recovered) = if kind == "fixed" {
                let b = support::fixed_face(2, face, s.clone(), calls());
                (
                    b.clean_price_oas(
                        spread,
                        &e,
                        Actual365Fixed::new(),
                        Compounding::Continuous,
                        Frequency::NoFrequency,
                        None,
                    )
                    .unwrap(),
                    b.oas(
                        price,
                        &e,
                        Actual365Fixed::new(),
                        Compounding::Continuous,
                        Frequency::NoFrequency,
                        None,
                        1e-10,
                        100,
                        0.0,
                    )
                    .unwrap(),
                )
            } else {
                let b = support::zero_face(2, face, s.clone(), calls());
                (
                    b.clean_price_oas(
                        spread,
                        &e,
                        Actual365Fixed::new(),
                        Compounding::Continuous,
                        Frequency::NoFrequency,
                        None,
                    )
                    .unwrap(),
                    b.oas(
                        price,
                        &e,
                        Actual365Fixed::new(),
                        Compounding::Continuous,
                        Frequency::NoFrequency,
                        None,
                        1e-10,
                        100,
                        0.0,
                    )
                    .unwrap(),
                )
            };
            close(actual, price, 2e-10);
            close(recovered, spread, 2e-10);
        }
    }
}
