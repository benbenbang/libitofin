//! Native QuantLib 1.43 whole-swap parity and additive OIS-side contracts.
use libitofin::errors::QlResult;
use libitofin::handle::RelinkableHandle;
use libitofin::indexes::ibor::Estr;
use libitofin::instrument::Instrument;
use libitofin::instruments::{MakeOis, OvernightIndexedSwap, SwapType};
use libitofin::settings::Settings;
use libitofin::shared::{Shared, shared};
use libitofin::termstructures::yields::DiscountCurve;
use libitofin::termstructures::yieldtermstructure::YieldTermStructure;
use libitofin::time::businessdayconvention::BusinessDayConvention;
use libitofin::time::date::{Date, Month};
use libitofin::time::daycounters::actual365fixed::Actual365Fixed;
use libitofin::time::{frequency::Frequency, period::Period, timeunit::TimeUnit};

fn today() -> Date {
    Date::new(2, Month::January, 2026)
}
fn curve(discounts: [f64; 5]) -> Shared<dyn YieldTermStructure> {
    shared(
        DiscountCurve::new(
            [0, 180, 500, 1000, 2000]
                .map(|days| today() + days)
                .to_vec(),
            discounts.to_vec(),
            Actual365Fixed::new(),
            None,
        )
        .unwrap(),
    )
}
fn close(actual: f64, expected: f64, tolerance: f64) {
    assert!(
        (actual - expected).abs() <= tolerance,
        "actual {actual:.17}, expected {expected:.17}"
    );
}
struct Market {
    settings: Shared<Settings<Date>>,
    forward: RelinkableHandle<dyn YieldTermStructure>,
    discount: RelinkableHandle<dyn YieldTermStructure>,
}
impl Market {
    fn new() -> Self {
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today());
        Self {
            settings,
            forward: RelinkableHandle::new(curve([1.0, 0.986, 0.955, 0.905, 0.82])),
            discount: RelinkableHandle::new(curve([1.0, 0.992, 0.978, 0.95, 0.90])),
        }
    }
    fn builder(&self, rate: Option<f64>, spread: f64, eom: bool) -> MakeOis {
        MakeOis::new(
            Period::new(30, TimeUnit::Months),
            shared(Estr::new(self.forward.handle(), self.settings.clone())),
            rate,
            Period::new(0, TimeUnit::Days),
            self.settings.clone(),
        )
        .with_effective_date(if eom {
            Date::new(27, Month::February, 2026)
        } else {
            Date::new(7, Month::January, 2026)
        })
        .with_termination_date(Date::new(19, Month::July, 2028))
        .with_nominal(1_230_000.0)
        .with_overnight_leg_spread(spread)
        .with_fixed_leg_day_count(Actual365Fixed::new())
        .with_payment_frequency(Frequency::Semiannual)
        .with_payment_lag(2)
        .with_payment_adjustment(BusinessDayConvention::ModifiedFollowing)
        .with_end_of_month(eom)
        .with_discounting_term_structure(self.discount.handle())
    }
    fn build(&self, side: SwapType, rate: Option<f64>) -> QlResult<OvernightIndexedSwap> {
        self.builder(rate, -0.0012, false).with_type(side).build()
    }
}
fn measures(swap: &mut OvernightIndexedSwap) -> [f64; 8] {
    [
        swap.npv().unwrap(),
        swap.fixed_vs_floating_mut().fixed_leg_npv().unwrap(),
        swap.overnight_leg_npv().unwrap(),
        swap.fixed_vs_floating_mut().fixed_leg_bps().unwrap(),
        swap.overnight_leg_bps().unwrap(),
        swap.fixed_vs_floating_mut().fair_rate().unwrap(),
        swap.fixed_vs_floating_mut().fair_spread().unwrap(),
        swap.fixed_vs_floating().fixed_rate(),
    ]
}
#[test]
fn both_sides_match_nonflat_native_ois_fixture_including_eom_and_fair_fill() {
    for line in include_str!("fixtures/medium5_ois/reference.tsv")
        .lines()
        .skip(1)
    {
        let mut cells = line.split('\t');
        let name = cells.next().unwrap();
        let expected: Vec<f64> = cells.map(|cell| cell.parse().unwrap()).collect();
        let side = if name.starts_with("payer") {
            SwapType::Payer
        } else {
            SwapType::Receiver
        };
        let market = Market::new();
        let mut swap = market
            .builder(
                if name.ends_with("fair") {
                    None
                } else {
                    Some(0.034)
                },
                -0.0012,
                name.ends_with("eom"),
            )
            .with_type(side)
            .build()
            .unwrap();
        assert_eq!(swap.fixed_vs_floating().swap_type(), side);
        for (i, actual) in measures(&mut swap).into_iter().enumerate() {
            close(actual, expected[i], if i < 5 { 2e-9 } else { 2e-14 });
        }
        assert_eq!(
            swap.overnight_schedule().start_date().serial_number(),
            expected[8] as i32
        );
        assert_eq!(
            swap.overnight_schedule().end_date().serial_number(),
            expected[9] as i32
        );
        assert_eq!(
            swap.fixed_vs_floating().fixed_leg().len(),
            expected[10] as usize
        );
    }
}
#[test]
fn default_is_payer_and_repeated_side_setter_uses_last_value() {
    let market = Market::new();
    let mut default = market.builder(Some(0.034), -0.0012, false).build().unwrap();
    let mut payer = market.build(SwapType::Payer, Some(0.034)).unwrap();
    let mut receiver = market
        .builder(Some(0.034), -0.0012, false)
        .with_type(SwapType::Payer)
        .with_type(SwapType::Receiver)
        .build()
        .unwrap();
    let mut reset = market
        .builder(Some(0.034), -0.0012, false)
        .with_type(SwapType::Receiver)
        .with_type(SwapType::Payer)
        .build()
        .unwrap();
    assert_eq!(default.fixed_vs_floating().swap_type(), SwapType::Payer);
    assert_eq!(receiver.fixed_vs_floating().swap_type(), SwapType::Receiver);
    assert_eq!(measures(&mut default), measures(&mut payer));
    assert_eq!(measures(&mut default), measures(&mut reset));
    let p = measures(&mut payer);
    let r = measures(&mut receiver);
    for i in 0..5 {
        assert_eq!(p[i], -r[i]);
    }
    assert_eq!(&p[5..], &r[5..]);
}
#[test]
fn both_signed_bps_and_fair_rate_and_spread_rebuilds_are_consistent() {
    let market = Market::new();
    for side in [SwapType::Payer, SwapType::Receiver] {
        let mut swap = market.build(side, Some(0.034)).unwrap();
        let values = measures(&mut swap);
        let mut rate_shift = market.build(side, Some(0.034 + 1e-4)).unwrap();
        let mut spread_shift = market
            .builder(Some(0.034), -0.0012 + 1e-4, false)
            .with_type(side)
            .build()
            .unwrap();
        close(rate_shift.npv().unwrap() - values[0], values[3], 2e-9);
        close(spread_shift.npv().unwrap() - values[0], values[4], 2e-9);
        let mut fair = market.build(side, Some(values[5])).unwrap();
        let mut filled = market.build(side, None).unwrap();
        let mut fair_spread = market
            .builder(Some(0.034), values[6], false)
            .with_type(side)
            .build()
            .unwrap();
        for rebuilt in [&mut fair, &mut filled, &mut fair_spread] {
            close(rebuilt.npv().unwrap(), 0.0, 2e-9);
        }
    }
}
#[test]
fn shared_forward_and_discount_relinks_reprice_both_sides() {
    let market = Market::new();
    let mut payer = market.build(SwapType::Payer, Some(0.034)).unwrap();
    let mut receiver = market.build(SwapType::Receiver, Some(0.034)).unwrap();
    let mut last = payer.npv().unwrap();
    for (handle, discounts) in [
        (&market.forward, [1.0, 0.981, 0.946, 0.887, 0.79]),
        (&market.discount, [1.0, 0.978, 0.952, 0.917, 0.83]),
    ] {
        handle.link_to(curve(discounts));
        let next = payer.npv().unwrap();
        assert!((next - last).abs() > 1.0);
        assert_eq!(next, -receiver.npv().unwrap());
        last = next;
    }
    market.settings.set_evaluation_date(today() + 1);
    let next = payer.npv().unwrap();
    assert_eq!(next, last);
    assert_eq!(next, -receiver.npv().unwrap());
    market.settings.set_evaluation_date(today() + 1100);
    assert_eq!(payer.npv().unwrap(), 0.0);
    assert_eq!(receiver.npv().unwrap(), 0.0);
}
#[test]
fn empty_engine_curve_errors_and_recovery_are_the_same_for_both_sides() {
    let market = Market::new();
    market.discount.reset();
    for side in [SwapType::Payer, SwapType::Receiver] {
        let mut swap = market.build(side, Some(0.034)).unwrap();
        assert!(swap.npv().is_err());
        assert!(market.build(side, None).is_err());
        market
            .discount
            .link_to(curve([1.0, 0.992, 0.978, 0.95, 0.90]));
        assert!(swap.npv().unwrap().is_finite());
        market.forward.reset();
        assert!(swap.npv().is_err());
        market
            .forward
            .link_to(curve([1.0, 0.986, 0.955, 0.905, 0.82]));
        assert!(swap.npv().unwrap().is_finite());
        market.discount.reset();
    }
}
