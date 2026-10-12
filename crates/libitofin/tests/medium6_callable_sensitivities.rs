//! Native QuantLib 1.43 clean-normalized OAS sensitivity fixtures.

#[path = "fixtures/medium6_callable_sensitivities/support.rs"]
mod support;

use libitofin::instrument::Instrument;
use libitofin::interestrate::Compounding;
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::frequency::Frequency;
use support::{close, engine, fixed, rights, settings, zero};

#[test]
fn thirty_six_native_clean_price_bumps_and_sensitivities() {
    for row in include_str!("fixtures/medium6_callable_sensitivities/native.tsv")
        .lines()
        .skip(1)
    {
        let columns: Vec<_> = row.split('\t').collect();
        let numbers: Vec<f64> = columns[3..].iter().map(|s| s.parse().unwrap()).collect();
        let (comp, freq) = match columns[2] {
            "continuous" => (Compounding::Continuous, Frequency::Annual),
            "compounded" => (Compounding::Compounded, Frequency::Semiannual),
            "simple" => (Compounding::Simple, Frequency::Annual),
            _ => unreachable!(),
        };
        let settings = settings();
        let engine = engine(settings.clone());
        let spread = 0.0075;
        let bump = numbers[0];
        macro_rules! check {
            ($bond:expr) => {{
                let bond = $bond;
                let accrued = bond.bond().accrued_amount(None).unwrap();
                close(accrued, numbers[1], 1e-12);
                let mut prices = vec![];
                for (value, expected) in [spread, spread + bump, spread - bump]
                    .into_iter()
                    .zip(&numbers[2..5])
                {
                    let price = bond
                        .clean_price_oas(value, &engine, Actual365Fixed::new(), comp, freq, None)
                        .unwrap();
                    close(price, *expected, 2e-11);
                    prices.push(price);
                }
                let duration = bond
                    .effective_duration(
                        spread,
                        &engine,
                        Actual365Fixed::new(),
                        comp,
                        freq,
                        Some(bump),
                    )
                    .unwrap();
                let convexity = bond
                    .effective_convexity(
                        spread,
                        &engine,
                        Actual365Fixed::new(),
                        comp,
                        freq,
                        Some(bump),
                    )
                    .unwrap();
                close(duration, numbers[5], 2e-9);
                close(convexity, numbers[6], 2e-6);
                close(
                    duration,
                    (prices[2] - prices[1]) / (2.0 * prices[0] * bump),
                    1e-12,
                );
                close(
                    convexity,
                    (prices[1] + prices[2] - 2.0 * prices[0]) / (bump * bump * prices[0]),
                    1e-12,
                );
                if accrued > 0.0 {
                    let dirty_duration =
                        (prices[2] - prices[1]) / (2.0 * (prices[0] + accrued) * bump);
                    assert!((duration - dirty_duration).abs() > 0.01);
                }
                if bump == 2e-4 {
                    assert_eq!(
                        duration,
                        bond.effective_duration(
                            spread,
                            &engine,
                            Actual365Fixed::new(),
                            comp,
                            freq,
                            None
                        )
                        .unwrap()
                    );
                    assert_eq!(
                        convexity,
                        bond.effective_convexity(
                            spread,
                            &engine,
                            Actual365Fixed::new(),
                            comp,
                            freq,
                            None
                        )
                        .unwrap()
                    );
                }
                assert!(bond.base().pricing_engine().is_none());
                assert!(!bond.base().is_calculated());
            }};
        }
        if columns[0] == "fixed" {
            check!(fixed(settings, rights(columns[1])));
        } else {
            check!(zero(settings, rights(columns[1]), 100.0));
        }
    }
}
