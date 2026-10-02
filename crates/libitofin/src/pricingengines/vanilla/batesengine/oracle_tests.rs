use super::tests::*;
use crate::time::daycounter::DayCounter;
use crate::time::daycounters::actual365fixed::Actual365Fixed;
use crate::time::daycounters::actualactual::{ActualActual, Convention};

fn number(fields: &[&str], index: usize) -> Real {
    fields[index].parse().unwrap()
}
fn date(fields: &[&str], index: usize) -> Date {
    Date::new(
        fields[index].parse().unwrap(),
        Month::from_ordinal(fields[index + 1].parse().unwrap()),
        fields[index + 2].parse().unwrap(),
    )
}

#[test]
fn independent_quantlib_and_poisson_heston_price_corpus() {
    let fixture = include_str!("../../../../../../sdk/go/testdata/bates-prices.tsv");
    let mut count = 0;
    for line in fixture.lines().skip(1) {
        let fields: Vec<_> = line.split('\t').collect();
        assert_eq!(fields.len(), 24);
        let reference = date(&fields, 1);
        let expiry = date(&fields, 4);
        let day_counter: DayCounter = match fields[7] {
            "ActualActualISDA" => ActualActual::with_convention(Convention::ISDA),
            "Actual360" => Actual360::new(),
            "Actual365Fixed" => Actual365Fixed::new(),
            other => panic!("unknown oracle day counter {other}"),
        };
        let curve = |rate| {
            Handle::new(shared(FlatForward::with_rate(
                reference,
                rate,
                day_counter.clone(),
                Compounding::Continuous,
                Frequency::Annual,
            )) as Shared<dyn YieldTermStructure>)
        };
        let process = shared(
            BatesProcess::new(
                curve(number(&fields, 8)),
                curve(number(&fields, 9)),
                quote_handle(&quote(number(&fields, 10))),
                number(&fields, 17),
                number(&fields, 14),
                number(&fields, 13),
                number(&fields, 15),
                number(&fields, 16),
                number(&fields, 20),
                number(&fields, 18),
                number(&fields, 19),
            )
            .unwrap(),
        );
        let model = BatesModel::new(process).unwrap();
        let kind = match fields[12] {
            "call" => OptionType::Call,
            "put" => OptionType::Put,
            other => panic!("unknown oracle kind {other}"),
        };
        let settings = shared(Settings::new());
        settings.set_evaluation_date(reference);
        let payoff = shared(PlainVanillaPayoff::new(kind, number(&fields, 11)))
            as Shared<dyn StrikedTypePayoff>;
        let exercise = shared(EuropeanExercise::new(expiry)) as Shared<dyn Exercise>;
        let mut option = VanillaOption::new(payoff, exercise, settings);
        option.base_mut().set_pricing_engine(shared_mut(
            BatesEngine::new(model, fields[21].parse().unwrap()).unwrap(),
        ) as SharedMut<dyn PricingEngine>);
        let actual = option.npv().unwrap();
        let expected = number(&fields, 23);
        assert!(
            (actual - expected).abs() <= 2e-10,
            "{}: price {actual:.17} vs oracle {expected:.17}",
            fields[0]
        );
        count += 1;
    }
    assert_eq!(count, 52);
}
