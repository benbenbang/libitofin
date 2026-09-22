use super::*;
use crate::cashflows::{AverageBMACoupon, Coupon};
use crate::indexes::UsdLibor;
use crate::interestrate::Compounding;
use crate::math::interpolations::loglinear::LogLinear;
use crate::quotes::SimpleQuote;
use crate::termstructures::bootstraptraits::Discount;
use crate::termstructures::yields::{FlatForward, PiecewiseYieldCurve};
use crate::time::date::Month;
use crate::time::daycounters::{
    actual360::Actual360,
    actualactual::{ActualActual, Convention},
};
use crate::time::frequency::Frequency;

const YEARS: [i32; 10] = [1, 2, 3, 4, 5, 7, 10, 15, 20, 30];
const FRACTIONS: [f64; 10] = [
    0.6756, 0.68, 0.6825, 0.685, 0.6881, 0.695, 0.7044, 0.7169, 0.7269, 0.7381,
];

struct Market {
    settings: Shared<Settings<Date>>,
    bma: Shared<BMAIndex>,
    ibor: Shared<IborIndex>,
    calendar: Calendar,
    today: Date,
    settlement: Date,
}
impl Market {
    fn new() -> Self {
        let today = Date::new(23, Month::October, 2025);
        let settings = shared(Settings::new());
        settings.set_evaluation_date(today);
        let bma = shared(BMAIndex::new(Handle::empty(), settings.clone()));
        let ibor = UsdLibor::new(
            Period::new(3, TimeUnit::Months),
            Handle::empty(),
            settings.clone(),
        )
        .unwrap();
        let calendar = JointCalendar::of_two(
            bma.fixing_calendar(),
            ibor.fixing_calendar(),
            JointCalendarRule::JoinHolidays,
        );
        let settlement = calendar.advance(today, 2, TimeUnit::Days, Bdc::Following, false);
        let risk = shared(FlatForward::with_rate(
            settlement,
            0.04,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        ));
        let ibor = shared(ibor.clone_with(Handle::new(risk as Shared<dyn YieldTermStructure>)));
        bma.add_fixing(Date::new(22, Month::October, 2025), 0.03)
            .unwrap();
        Self {
            settings,
            bma,
            ibor,
            calendar,
            today,
            settlement,
        }
    }
    fn helper(&self, year: i32, quote: &Shared<SimpleQuote>) -> Shared<BMASwapRateHelper> {
        BMASwapRateHelper::new(
            Handle::new(quote.clone() as Shared<dyn Quote>),
            Period::new(year, TimeUnit::Years),
            2,
            self.calendar.clone(),
            Period::new(3, TimeUnit::Months),
            Bdc::Following,
            ActualActual::with_convention(Convention::ISDA),
            &self.bma,
            &self.ibor,
        )
        .unwrap()
    }
    fn swap(&self, year: i32, bma: Shared<BMAIndex>) -> BMASwap {
        let end = self.settlement + Period::new(year, TimeUnit::Years);
        let bs = MakeSchedule::new()
            .from(self.settlement)
            .to(end)
            .with_tenor(Period::new(3, TimeUnit::Months))
            .with_calendar(bma.fixing_calendar())
            .with_convention(Bdc::Following)
            .backwards()
            .build();
        let ls = MakeSchedule::new()
            .from(self.settlement)
            .to(end)
            .with_tenor(self.ibor.tenor())
            .with_calendar(self.ibor.fixing_calendar())
            .with_convention(self.ibor.business_day_convention())
            .end_of_month(self.ibor.end_of_month())
            .backwards()
            .build();
        let mut swap = BMASwap::new(
            SwapType::Payer,
            100.,
            ls,
            0.75,
            0.,
            self.ibor.clone(),
            self.ibor.day_counter().clone(),
            bs,
            bma,
            ActualActual::with_convention(Convention::ISDA),
            self.settings.clone(),
        )
        .unwrap();
        swap.base_mut()
            .set_pricing_engine(shared_mut(DiscountingSwapEngine::new(
                self.ibor.forwarding_term_structure().clone(),
                None,
                None,
                None,
                self.settings.clone(),
            )) as SharedMut<dyn PricingEngine>);
        swap
    }
}

#[test]
fn bma_original_ten_tenor_curve_consistency() {
    let m = Market::new();
    let quotes: Vec<_> = FRACTIONS
        .iter()
        .map(|&f| shared(SimpleQuote::new(f)))
        .collect();
    let helpers: Vec<_> = YEARS
        .iter()
        .zip(&quotes)
        .map(|(&y, q)| m.helper(y, q) as Shared<dyn RateHelper>)
        .collect();
    let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        m.today,
        helpers.clone(),
        Actual360::new(),
        LogLinear,
    )
    .unwrap();
    let bma = shared(
        m.bma
            .clone_with(Handle::new(curve.clone() as Shared<dyn YieldTermStructure>)),
    );
    for (&year, &fraction) in YEARS.iter().zip(&FRACTIONS) {
        let actual = m.swap(year, bma.clone()).fair_libor_fraction().unwrap();
        assert!(
            (actual - fraction).abs() < 1e-9,
            "{year}: {actual} vs {fraction}"
        );
    }
    let before = curve
        .discount_date(helpers[9].pillar_date(), false)
        .unwrap();
    quotes[9].set_value(0.75);
    assert!((m.swap(30, bma).fair_libor_fraction().unwrap() - 0.75).abs() < 1e-9);
    assert!(
        (curve
            .discount_date(helpers[9].pillar_date(), false)
            .unwrap()
            - before)
            .abs()
            > 1e-5
    );
}

#[test]
fn bma_holiday_fixings_and_missing_history_recovery() {
    let m = Market::new();
    let bma = &m.bma;
    assert!(!bma.is_valid_fixing_date(Date::new(25, Month::December, 2024)));
    assert!(bma.is_valid_fixing_date(Date::new(26, Month::December, 2024)));
    assert!(!bma.is_valid_fixing_date(Date::new(27, Month::December, 2024)));
    assert!(!bma.is_valid_fixing_date(Date::new(1, Month::January, 2025)));
    assert!(bma.is_valid_fixing_date(Date::new(2, Month::January, 2025)));
    assert!(
        bma.add_fixing(Date::new(27, Month::December, 2024), 0.01)
            .is_err()
    );
    let first = Date::new(27, Month::December, 2024);
    let end = Date::new(10, Month::January, 2025);
    let coupon = AverageBMACoupon::new(
        end,
        100.,
        first,
        end,
        bma.clone(),
        1.2,
        0.001,
        None,
        None,
        Actual360::new(),
    )
    .unwrap();
    assert!(coupon.rate().is_err());
    for (d, rate) in [
        (Date::new(26, Month::December, 2024), 0.02),
        (Date::new(2, Month::January, 2025), 0.04),
        (Date::new(8, Month::January, 2025), 0.06),
    ] {
        bma.add_fixing(d, rate).unwrap();
    }
    let expected = 1.2 * (0.02 * 7. + 0.04 * 6. + 0.06) / 14. + 0.001;
    assert!((coupon.rate().unwrap() - expected).abs() < 1e-15);
    assert!((coupon.amount().unwrap() - 100. * 14. / 360. * expected).abs() < 1e-15);
    assert_eq!(
        bma.maturity_date(first).unwrap(),
        Date::new(2, Month::January, 2025)
    );
}

#[test]
fn bma_helper_weak_curve_and_invalid_date_recovery() {
    let m = Market::new();
    let h = m.helper(1, &shared(SimpleQuote::new(0.6756)));
    let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        m.today,
        vec![h.clone() as Shared<dyn RateHelper>],
        Actual360::new(),
        LogLinear,
    )
    .unwrap();
    let before = curve.discount_date(h.pillar_date(), false).unwrap();
    m.settings.set_evaluation_date(Date::max_date());
    assert!(h.implied_quote().is_err());
    assert!(curve.discount_date(h.pillar_date(), false).is_err());
    m.settings.set_evaluation_date(m.today);
    assert!((curve.discount_date(h.pillar_date(), false).unwrap() - before).abs() < 1e-12);
    let weak = Shared::downgrade(&curve);
    drop(curve);
    assert!(weak.upgrade().is_none());
    assert!(h.implied_quote().is_err());
}

#[test]
fn bma_fixing_invalidation_and_retained_inputs() {
    let m = Market::new();
    let h = m.helper(1, &shared(SimpleQuote::new(0.6756)));
    let curve = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        m.today,
        vec![h.clone() as Shared<dyn RateHelper>],
        Actual360::new(),
        LogLinear,
    )
    .unwrap();
    let bma = shared(
        m.bma
            .clone_with(Handle::new(curve.clone() as Shared<dyn YieldTermStructure>)),
    );
    let mut swap = m.swap(1, bma);
    let initial = swap.npv().unwrap();
    assert!(swap.base().is_calculated());
    m.bma.clear_fixings();
    assert!(!swap.base().is_calculated());
    assert!(swap.npv().is_err());
    m.bma
        .add_fixing(Date::new(22, Month::October, 2025), 0.03)
        .unwrap();
    assert!((swap.npv().unwrap() - initial).abs() < 1e-9);
    let weak = Shared::downgrade(&curve);
    drop(curve);
    drop(h);
    drop(m);
    assert!(weak.upgrade().is_some());
    assert!((swap.npv().unwrap() - initial).abs() < 1e-9);
    drop(swap);
    assert!(weak.upgrade().is_none());
}

#[test]
fn bma_invalid_constructor_inputs_are_fallible() {
    let m = Market::new();
    for period in [
        Period::new(0, TimeUnit::Years),
        Period::new(i32::MAX, TimeUnit::Years),
        Period::new(1, TimeUnit::Hours),
    ] {
        assert!(
            BMASwapRateHelper::new(
                Handle::new(shared(SimpleQuote::new(0.67)) as Shared<dyn Quote>),
                period,
                2,
                m.calendar.clone(),
                Period::new(3, TimeUnit::Months),
                Bdc::Following,
                Actual360::new(),
                &m.bma,
                &m.ibor
            )
            .is_err()
        );
    }
    let other = shared(BMAIndex::new(Handle::empty(), shared(Settings::new())));
    assert!(
        BMASwapRateHelper::new(
            Handle::new(shared(SimpleQuote::new(0.67)) as Shared<dyn Quote>),
            Period::new(1, TimeUnit::Years),
            2,
            m.calendar.clone(),
            Period::new(3, TimeUnit::Months),
            Bdc::Following,
            Actual360::new(),
            &other,
            &m.ibor
        )
        .is_err()
    );
    assert!(
        AverageBMACoupon::new(
            m.today,
            100.,
            m.today,
            m.today,
            m.bma.clone(),
            1.,
            0.,
            None,
            None,
            Actual360::new()
        )
        .is_err()
    );
    assert!(
        AverageBMACoupon::new(
            m.today + 10,
            100.,
            m.today,
            m.today + 10,
            m.bma.clone(),
            f64::NAN,
            0.,
            None,
            None,
            Actual360::new()
        )
        .is_err()
    );
    assert!(m.bma.fixing_schedule(m.today + 1, m.today).is_err());
}
