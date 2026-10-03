use super::*;
use crate::handle::{Handle, RelinkableHandle};
use crate::interestrate::Compounding;
use crate::quotes::{Quote, SimpleQuote};
use crate::shared::WeakMut;
use crate::termstructures::yields::FlatForward;
use crate::termstructures::yieldtermstructure::YieldTermStructure;
use crate::test_support::{Flag, as_observer};
use crate::time::{
    date::{Date, Month},
    daycounters::actual360::Actual360,
    frequency::Frequency,
};

pub(super) fn parameters() -> GjrGarchParameters {
    GjrGarchParameters {
        omega: 2e-6,
        alpha: 0.04,
        beta: 0.88,
        gamma: 0.08,
        lambda: -0.4,
        v0: 0.04 / 252.0,
        days_per_year: 365.0,
    }
}

fn curve(rate: Real) -> Shared<dyn YieldTermStructure> {
    shared(FlatForward::with_rate(
        Date::new(15, Month::June, 2026),
        rate,
        Actual360::new(),
        Compounding::Continuous,
        Frequency::Annual,
    ))
}

fn process(parameters: GjrGarchParameters) -> Shared<GjrGarchProcess> {
    shared(
        GjrGarchProcess::new(
            Handle::new(curve(0.03)),
            Handle::new(curve(0.01)),
            Handle::new(shared(SimpleQuote::new(100.0)) as Shared<dyn Quote>),
            parameters,
            GjrGarchDiscretization::Reflection,
        )
        .unwrap(),
    )
}

pub(super) fn model() -> SharedMut<GjrGarchModel> {
    GjrGarchModel::new(process(parameters())).unwrap()
}

pub(super) fn array(p: GjrGarchParameters) -> Array {
    Array::from([p.omega, p.alpha, p.beta, p.gamma, p.lambda, p.v0])
}

#[test]
fn construction_preserves_daily_parameter_order_and_resets_scheme() {
    let original = process(parameters());
    let model = GjrGarchModel::new(Shared::clone(&original)).unwrap();
    let borrowed = model.borrow();
    let rebuilt = borrowed.process();
    assert_eq!(borrowed.calibrated_model().params(), array(parameters()));
    assert_eq!(array(rebuilt.parameters()), array(parameters()));
    assert_eq!(
        [
            borrowed.omega(),
            borrowed.alpha(),
            borrowed.beta(),
            borrowed.gamma(),
            borrowed.lambda(),
            borrowed.v0()
        ],
        [
            parameters().omega,
            parameters().alpha,
            parameters().beta,
            parameters().gamma,
            parameters().lambda,
            parameters().v0
        ]
    );
    assert_eq!(rebuilt.days_per_year(), 365.0);
    assert_eq!(
        rebuilt.discretization(),
        GjrGarchDiscretization::FullTruncation
    );
    assert!(!Shared::ptr_eq(&original, &rebuilt));
    let state = Array::from([100.0, -0.04]);
    assert!(
        original
            .evolve(0.0, &state, 0.0, &Array::from([0.0; 2]))
            .unwrap()[1]
            > 0.0
    );
    assert_eq!(
        rebuilt
            .evolve(0.0, &state, 0.0, &Array::from([0.0; 2]))
            .unwrap()[1],
        -0.04
    );
}

#[test]
fn successful_setter_rebuilds_all_parameters_before_notification() {
    let model = model();
    let old = model.borrow().process();
    let flag = Flag::new();
    model
        .borrow()
        .observable()
        .register_observer(&as_observer(&flag));
    let params = Array::from([3e-6, 0.06, 0.81, 0.12, 0.2, 0.0002]);
    model.borrow_mut().set_params(&params).unwrap();
    let borrowed = model.borrow();
    let rebuilt = borrowed.process();
    assert_eq!(array(rebuilt.parameters()), params);
    assert_eq!(borrowed.calibrated_model().params(), params);
    assert!(!Shared::ptr_eq(&old, &rebuilt));
    assert_eq!(rebuilt.days_per_year(), 365.0);
    assert_eq!(
        rebuilt.discretization(),
        GjrGarchDiscretization::FullTruncation
    );
    assert!(Flag::is_up(&flag));
}

#[test]
fn setter_rejects_dimensions_domains_nonfinite_and_overflow_atomically() {
    let model = model();
    let seed = model.borrow().calibrated_model().params();
    let original = model.borrow().process();
    let flag = Flag::new();
    model
        .borrow()
        .observable()
        .register_observer(&as_observer(&flag));
    let mut cases = vec![Array::new(), Array::from([2e-6; 5]), Array::from([2e-6; 7])];
    for (index, value) in [
        (0, 0.0),
        (5, 0.0),
        (1, -0.01),
        (1, 1.01),
        (2, -0.01),
        (2, 1.01),
        (3, -1.01),
        (3, 1.01),
        (0, Real::MAX),
        (4, Real::MAX),
    ] {
        let mut trial = seed.clone();
        trial[index] = value;
        cases.push(trial);
    }
    for index in 0..6 {
        for value in [Real::NAN, Real::INFINITY, Real::NEG_INFINITY] {
            let mut trial = seed.clone();
            trial[index] = value;
            cases.push(trial);
        }
    }
    cases.push(Array::from([2e-6, 0.7, 0.1, -0.2, -0.4, 0.0001]));
    cases.push(Array::from([2e-6, 0.04, 0.8, -0.05, -0.4, 0.0001]));
    for trial in cases {
        assert!(
            model.borrow_mut().set_params(&trial).is_err(),
            "accepted {trial:?}"
        );
        let borrowed = model.borrow();
        assert_eq!(borrowed.calibrated_model().params(), seed);
        assert!(Shared::ptr_eq(&original, &borrowed.process()));
        assert!(!Flag::is_up(&flag));
    }
}

#[test]
fn constructor_rejects_process_only_domain_values_outside_model_constraints() {
    for (index, value) in [(0, 0.0), (5, 0.0), (1, 1.1), (2, 1.1), (3, 1.1)] {
        let mut p = parameters();
        match index {
            0 => p.omega = value,
            1 => p.alpha = value,
            2 => p.beta = value,
            3 => p.gamma = value,
            5 => p.v0 = value,
            _ => unreachable!(),
        }
        assert!(GjrGarchModel::new(process(p)).is_err());
    }
    let mut p = parameters();
    p.alpha = 0.7;
    p.beta = 0.1;
    p.gamma = -0.2;
    assert!(GjrGarchModel::new(process(p)).is_err());
}

#[test]
fn coupled_constraint_supports_boundaries_and_nonstationary_processes() {
    let domain = GjrGarchConstraint::new(365.0);
    for params in [
        Array::from([2e-6, 0.0, 0.0, 0.0, 0.0, 0.0001]),
        Array::from([2e-6, 1.0, 1.0, -1.0, 0.0, 0.0001]),
        Array::from([2e-6, 1.0, 1.0, 1.0, -0.4, 0.0001]),
    ] {
        assert!(domain.test(&params), "rejected {params:?}");
        model().borrow_mut().set_params(&params).unwrap();
    }
    assert!(!domain.test(&Array::new()));
    assert!(!domain.test(&Array::from([2e-6; 5])));
    assert_eq!(
        domain.lower_bound(&array(parameters())),
        Array::from([0.0, 0.0, 0.0, -1.0, -Real::MAX, 0.0])
    );
    assert_eq!(
        domain.upper_bound(&array(parameters())),
        Array::from([Real::MAX, 1.0, 1.0, 1.0, Real::MAX, Real::MAX])
    );
}

struct Reader {
    model: WeakMut<GjrGarchModel>,
    seen: Option<GjrGarchParameters>,
}

impl Observer for Reader {
    fn update(&mut self) {
        if let Some(model) = self.model.upgrade() {
            let borrowed = model.borrow();
            assert_eq!(
                borrowed.calibrated_model().params(),
                array(borrowed.process().parameters())
            );
            self.seen = Some(borrowed.process().parameters());
        }
    }
}

#[test]
fn market_notifications_rebuild_coherently_and_inputs_outlive_callers() {
    let risk_free = RelinkableHandle::new(curve(0.03));
    let dividend = RelinkableHandle::new(curve(0.01));
    let spot = shared(SimpleQuote::new(100.0));
    let process = shared(
        GjrGarchProcess::new(
            risk_free.handle(),
            dividend.handle(),
            Handle::new(Shared::clone(&spot) as Shared<dyn Quote>),
            parameters(),
            GjrGarchDiscretization::Reflection,
        )
        .unwrap(),
    );
    let weak_process = Shared::downgrade(&process);
    let model = GjrGarchModel::new(process).unwrap();
    assert!(weak_process.upgrade().is_none());
    let reader = shared_mut(Reader {
        model: SharedMut::downgrade(&model),
        seen: None,
    });
    model
        .borrow()
        .observable()
        .register_observer(&(reader.clone() as SharedMut<dyn Observer>));
    for (handle, rate) in [(&risk_free, 0.06), (&dividend, 0.025)] {
        let before = model.borrow().process();
        handle.link_to(curve(rate));
        assert!(!Shared::ptr_eq(&before, &model.borrow().process()));
        assert_eq!(reader.borrow().seen, Some(parameters()));
    }
    let before = model.borrow().process();
    spot.set_value(123.0);
    assert!(!Shared::ptr_eq(&before, &model.borrow().process()));
    assert_eq!(reader.borrow().seen, Some(parameters()));
    drop(risk_free);
    drop(dividend);
    drop(spot);
    let process = model.borrow().process();
    assert_eq!(process.initial_values().unwrap()[0], 123.0);
    let drift = process.drift(0.0, &Array::from([123.0, 0.04])).unwrap();
    assert!((drift[0] - 0.015).abs() < 1e-12);
    let weak_model = SharedMut::downgrade(&model);
    drop(model);
    assert!(weak_model.upgrade().is_none());
}

#[test]
fn invalid_live_spot_notifies_without_panicking_and_rejects_setter_atomically() {
    let spot = shared(SimpleQuote::new(100.0));
    let process = shared(
        GjrGarchProcess::new(
            Handle::new(curve(0.03)),
            Handle::new(curve(0.01)),
            Handle::new(Shared::clone(&spot) as Shared<dyn Quote>),
            parameters(),
            GjrGarchDiscretization::FullTruncation,
        )
        .unwrap(),
    );
    let model = GjrGarchModel::new(process).unwrap();
    let original = model.borrow().process();
    let flag = Flag::new();
    model
        .borrow()
        .observable()
        .register_observer(&as_observer(&flag));
    spot.reset();
    assert!(Flag::is_up(&flag));
    assert!(model.borrow().process().initial_values().is_err());
    assert!(model.borrow_mut().set_params(&array(parameters())).is_err());
    assert!(Shared::ptr_eq(&original, &model.borrow().process()));
    spot.set_value(-1.0);
    assert!(model.borrow().process().initial_values().is_err());
    spot.set_value(120.0);
    assert_eq!(model.borrow().process().initial_values().unwrap()[0], 120.0);
    assert!(!Shared::ptr_eq(&original, &model.borrow().process()));
}
