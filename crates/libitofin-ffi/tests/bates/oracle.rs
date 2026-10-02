use super::*;
use libitofin::time::daycounter::DayCounter;
use libitofin::time::daycounters::actualactual::{ActualActual, Convention};

#[test]
fn independent_quantlib_price_fixtures() {
    let data = include_str!("../../../../sdk/go/testdata/bates-prices.tsv");
    let rows: Vec<_> = data.lines().skip(1).collect();
    assert_eq!(rows.len(), 52);
    for row in rows {
        let f: Vec<_> = row.split('\t').collect();
        let integer = |index: usize| f[index].parse::<i32>().unwrap();
        let real = |index: usize| f[index].parse::<f64>().unwrap();
        let date = |index| {
            Date::new(
                integer(index),
                Month::from_ordinal(integer(index + 1)),
                integer(index + 2),
            )
        };
        let today = date(1);
        let expiry = date(4);
        let mut context = Context::new();
        let dc: DayCounter = if f[7] == "ActualActualISDA" {
            ActualActual::with_convention(Convention::ISDA)
        } else {
            Actual365Fixed::new()
        };
        let dc = context.insert(dc).unwrap();
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(today);
        let settings = context.insert(settings).unwrap();
        let mut quotes = [0; 3];
        let mut curves = [0; 2];
        let mut process = 0;
        let mut model = 0;
        let mut engine = 0;
        let mut option = 0;
        let mut price = -999.0;
        let parameters = ItofinBatesParameters {
            theta: real(13),
            kappa: real(14),
            sigma: real(15),
            rho: real(16),
            v0: real(17),
            nu: real(18),
            delta: real(19),
            lambda: real(20),
        };
        unsafe {
            for (id, value) in quotes.iter_mut().zip([real(10), real(8), real(9)]) {
                assert_eq!(itofin_quote_new(&mut context, value, id, null_mut()), 0);
            }
            for i in 0..2 {
                assert_eq!(
                    itofin_flat_forward_from_quote(
                        &mut context,
                        today.serial_number(),
                        quotes[i + 1],
                        dc,
                        &mut curves[i],
                        null_mut()
                    ),
                    0
                );
            }
            assert_eq!(
                itofin_bates_process_new(
                    &mut context,
                    quotes[0],
                    curves[0],
                    curves[1],
                    &parameters,
                    &mut process,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_bates_model_new(&mut context, process, &mut model, null_mut()),
                0
            );
            assert_eq!(
                itofin_bates_engine_new(
                    &mut context,
                    model,
                    integer(21) as usize,
                    &mut engine,
                    null_mut()
                ),
                0
            );
            let kind = if f[12] == "call" { 0 } else { 1 };
            assert_eq!(
                itofin_option_new(
                    &mut context,
                    kind,
                    real(11),
                    0,
                    expiry.serial_number(),
                    0,
                    settings,
                    &mut option,
                    null_mut()
                ),
                0
            );
            assert_eq!(
                itofin_option_set_engine(&mut context, option, engine, 2, 0, null_mut()),
                0
            );
            assert_eq!(
                itofin_option_value(&mut context, option, 0, &mut price, null_mut()),
                0
            );
        }
        assert!(
            (price - real(23)).abs() <= 2e-10,
            "{}: {price} vs {}",
            f[0],
            real(23)
        );
    }
}
