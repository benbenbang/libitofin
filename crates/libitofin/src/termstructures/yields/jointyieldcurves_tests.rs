use super::*;
use crate::indexes::ibor::euribor::Euribor;
use crate::interestrate::Compounding;
use crate::quotes::SimpleQuote;
use crate::settings::Settings;
use crate::termstructures::yields::{DepositRateHelper, FlatForward};
use crate::time::{
    businessdayconvention::BusinessDayConvention, calendars::target::Target, date::Month,
    daycounters::actual360::Actual360, frequency::Frequency, timeunit::TimeUnit,
};

struct Fixture {
    settings: Shared<Settings<Date>>,
    reference: Date,
    base: Shared<IborIndex>,
    helper: Shared<dyn RateHelper>,
    basis: Vec<BasisSwapHelperConfig>,
    rate: Shared<SimpleQuote>,
}

impl Fixture {
    fn new() -> Self {
        let reference = Date::new(23, Month::October, 2025);
        let settings = shared(Settings::<Date>::new());
        settings.set_evaluation_date(reference);
        let base = shared(Euribor::three_months(Handle::empty(), settings.clone()));
        let other = shared(Euribor::six_months(Handle::empty(), settings.clone()));
        let rate = shared(SimpleQuote::new(0.03));
        let helper =
            DepositRateHelper::new(Handle::new(rate.clone()), &base) as Shared<dyn RateHelper>;
        let discount = Handle::new(shared(FlatForward::with_rate(
            reference,
            0.02,
            Actual360::new(),
            Compounding::Continuous,
            Frequency::Annual,
        )) as Shared<dyn YieldTermStructure>);
        let template = BasisSwapHelperConfig {
            quote: Handle::new(shared(SimpleQuote::new(0.002))),
            tenor: Period::new(2, TimeUnit::Years),
            settlement_days: 2,
            calendar: Target::new(),
            convention: BusinessDayConvention::ModifiedFollowing,
            end_of_month: false,
            base_index: base.clone(),
            other_index: other,
            discount,
            bootstrap_base_curve: true,
        };
        let mut other_short = template.clone();
        other_short.tenor = Period::new(6, TimeUnit::Months);
        other_short.bootstrap_base_curve = false;
        let mut other_long = other_short.clone();
        other_long.tenor = Period::new(2, TimeUnit::Years);
        Self {
            settings,
            reference,
            base,
            helper,
            basis: vec![template, other_short, other_long],
            rate,
        }
    }

    fn build(&self) -> QlResult<JointYieldCurves> {
        JointYieldCurves::new(
            self.reference,
            [vec![self.helper.clone()], vec![]],
            &self.basis,
            Actual360::new(),
            1e-12,
        )
    }
}

#[test]
fn exported_index_retains_joint_owner_until_its_last_consumer_drops() {
    let fixture = Fixture::new();
    let joint = fixture.build().unwrap();
    let first = joint.curve(0).unwrap();
    let second = joint.curve(1).unwrap();
    let weak_first = Shared::downgrade(&first.current_link().unwrap());
    let weak_second = Shared::downgrade(&second.current_link().unwrap());
    let consumer = fixture.base.clone_with(first.clone());
    let value = first.current_link().unwrap().discount(1.0, true).unwrap();
    drop(joint);
    drop(first);
    drop(second);
    fixture.rate.set_value(0.031);
    assert!(weak_first.upgrade().is_some());
    assert!(weak_second.upgrade().is_some());
    let forecast = consumer.forwarding_term_structure().clone();
    let updated = forecast
        .current_link()
        .unwrap()
        .discount(1.0, true)
        .unwrap();
    assert!((updated - value).abs() > 1e-8);
    drop(consumer);
    assert!(weak_first.upgrade().is_some());
    drop(forecast);
    assert!(weak_first.upgrade().is_none());
    assert!(weak_second.upgrade().is_none());
}

#[test]
fn joint_rejects_helper_membership_before_an_existing_curve_is_queried() {
    let fixture = Fixture::new();
    let old = PiecewiseYieldCurve::<Discount, LogLinear>::new(
        fixture.reference,
        vec![fixture.helper.clone()],
        Actual360::new(),
        LogLinear,
    )
    .unwrap();
    assert!(
        fixture
            .build()
            .err()
            .unwrap()
            .to_string()
            .contains("belongs")
    );
    assert!(old.discount(0.1, true).unwrap().is_finite());
    assert!(fixture.build().is_err());
    drop(old);
    assert!(fixture.build().is_ok());
}

#[test]
fn joint_rejects_unqueried_additional_helper_membership() {
    let fixture = Fixture::new();
    let primary = DepositRateHelper::from_rate(0.031, &fixture.base) as Shared<dyn RateHelper>;
    let old = Curve::with_bootstrap(
        fixture.reference,
        vec![primary],
        Actual360::new(),
        LogLinear,
        GlobalBootstrap::with_penalties(
            vec![fixture.helper.clone()],
            None,
            Some(1e-12),
            None,
            vec![],
            |_, _| vec![],
        ),
    )
    .unwrap();
    assert!(fixture.build().is_err());
    drop(old);
    assert!(fixture.build().is_ok());
}

#[test]
fn invalid_joint_inputs_leave_helpers_reusable() {
    let mut fixture = Fixture::new();
    assert!(
        JointYieldCurves::new(
            Date::null(),
            [vec![fixture.helper.clone()], vec![]],
            &fixture.basis,
            Actual360::new(),
            1e-12
        )
        .is_err()
    );
    let saved = fixture.basis.clone();
    fixture.basis[2].tenor = Period::new(-2, TimeUnit::Years);
    assert!(fixture.build().is_err());
    fixture.basis = saved;
    assert!(
        JointYieldCurves::new(
            fixture.reference,
            [vec![fixture.helper.clone()], vec![fixture.helper.clone()]],
            &fixture.basis,
            Actual360::new(),
            1e-12
        )
        .is_err()
    );
    let joint = fixture.build().unwrap();
    assert!(joint.curve(2).is_err());
    assert!(fixture.build().is_err());
    assert!(
        joint
            .curve(0)
            .unwrap()
            .current_link()
            .unwrap()
            .discount(1.0, true)
            .unwrap()
            .is_finite()
    );
    drop(joint);
    assert!(fixture.build().is_ok());
}

#[test]
fn joint_requires_compatible_settings_and_both_bootstrap_sides() {
    let mut fixture = Fixture::new();
    fixture
        .basis
        .iter_mut()
        .for_each(|item| item.bootstrap_base_curve = true);
    assert!(fixture.build().is_err());
    fixture.basis[2].bootstrap_base_curve = false;
    let settings = shared(Settings::<Date>::new());
    settings.set_evaluation_date(fixture.reference);
    let foreign = shared(Euribor::six_months(Handle::empty(), settings));
    fixture
        .basis
        .iter_mut()
        .for_each(|item| item.other_index = foreign.clone());
    assert!(fixture.build().is_err());
    assert_eq!(fixture.settings.evaluation_date(), Some(fixture.reference));
}

#[test]
fn legacy_piecewise_reuse_stays_available_and_all_live_owners_are_recorded() {
    let fixture = Fixture::new();
    let make = || {
        PiecewiseYieldCurve::<Discount, LogLinear>::new(
            fixture.reference,
            vec![fixture.helper.clone()],
            Actual360::new(),
            LogLinear,
        )
        .unwrap()
    };
    let first = make();
    let second = make();
    assert!(fixture.build().is_err());
    drop(second);
    assert!(fixture.build().is_err());
    drop(first);
    assert!(fixture.build().is_ok());
}
