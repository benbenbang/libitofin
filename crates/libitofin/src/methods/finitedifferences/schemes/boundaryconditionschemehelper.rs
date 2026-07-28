//! Fans a step's boundary-condition calls over the whole set.
//!
//! Port of `ql/methods/finitedifferences/schemes/boundaryconditionschemehelper.hpp:32`.

use crate::math::array::Array;
use crate::methods::finitedifferences::operators::FdmLinearOp;
use crate::methods::finitedifferences::utilities::FdmBoundaryConditionSet;
use crate::types::Time;

/// The boundary conditions of a scheme, applied as one.
///
/// Every method forwards to each condition in the set, in order, and is a
/// no-op on an empty set - the European path. C++ holds the helper by value as
/// a `const` member of each scheme (`expliciteulerscheme.hpp:57`), so all
/// methods take `&self`.
pub struct BoundaryConditionSchemeHelper {
    bc_set: FdmBoundaryConditionSet,
}

impl BoundaryConditionSchemeHelper {
    /// Builds the helper over `bc_set`.
    pub fn new(bc_set: FdmBoundaryConditionSet) -> Self {
        Self { bc_set }
    }

    /// Applies each condition to `op` before the operator is applied.
    pub fn apply_before_applying(&self, op: &mut dyn FdmLinearOp) {
        for bc in &self.bc_set {
            bc.apply_before_applying(op);
        }
    }

    /// Applies each condition to `op` and `a` before the system is solved.
    pub fn apply_before_solving(&self, op: &mut dyn FdmLinearOp, a: &mut Array) {
        for bc in &self.bc_set {
            bc.apply_before_solving(op, a);
        }
    }

    /// Applies each condition to `a` after the operator has been applied.
    pub fn apply_after_applying(&self, a: &mut Array) {
        for bc in &self.bc_set {
            bc.apply_after_applying(a);
        }
    }

    /// Applies each condition to `a` after the system has been solved.
    pub fn apply_after_solving(&self, a: &mut Array) {
        for bc in &self.bc_set {
            bc.apply_after_solving(a);
        }
    }

    /// Sets the current time on each condition.
    pub fn set_time(&self, t: Time) {
        for bc in &self.bc_set {
            bc.set_time(t);
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    use std::cell::RefCell;

    use crate::methods::finitedifferences::BoundaryCondition;
    use crate::shared::{Shared, SharedMut, shared, shared_mut};
    use crate::types::Real;

    struct ScaleOp {
        factor: Real,
    }

    impl FdmLinearOp for ScaleOp {
        fn apply(&self, r: &Array) -> Array {
            r.iter().map(|x| x * self.factor).collect()
        }
    }

    struct Recorder {
        id: Real,
        log: Shared<RefCell<Vec<String>>>,
    }

    impl Recorder {
        fn record(&self, call: &str) {
            self.log.borrow_mut().push(format!("{call}:{}", self.id));
        }

        fn record_op(&self, call: &str, op: &dyn FdmLinearOp) {
            let probe = op.apply(&Array::from([1.0]))[0];
            self.log
                .borrow_mut()
                .push(format!("{call}:{}:{probe}", self.id));
        }
    }

    impl BoundaryCondition for Recorder {
        fn apply_before_applying(&self, op: &mut dyn FdmLinearOp) {
            self.record_op("before_applying", op);
        }

        fn apply_after_applying(&self, a: &mut Array) {
            self.record("after_applying");
            a[0] = a[0] * 10.0 + self.id;
        }

        fn apply_before_solving(&self, op: &mut dyn FdmLinearOp, rhs: &mut Array) {
            self.record_op("before_solving", op);
            rhs[0] = rhs[0] * 10.0 + self.id;
        }

        fn apply_after_solving(&self, a: &mut Array) {
            self.record("after_solving");
            a[0] = a[0] * 10.0 + self.id;
        }

        fn set_time(&self, t: Time) {
            self.log
                .borrow_mut()
                .push(format!("set_time:{}:{t}", self.id));
        }
    }

    struct Silent;

    impl BoundaryCondition for Silent {
        fn apply_before_applying(&self, _op: &mut dyn FdmLinearOp) {}
        fn apply_after_applying(&self, _a: &mut Array) {}
        fn apply_before_solving(&self, _op: &mut dyn FdmLinearOp, _rhs: &mut Array) {}
        fn apply_after_solving(&self, _a: &mut Array) {}
        fn set_time(&self, _t: Time) {}
    }

    fn recorders(log: &Shared<RefCell<Vec<String>>>) -> BoundaryConditionSchemeHelper {
        BoundaryConditionSchemeHelper::new(vec![
            shared(Recorder {
                id: 1.0,
                log: Shared::clone(log),
            }),
            shared(Recorder {
                id: 2.0,
                log: Shared::clone(log),
            }),
        ])
    }

    #[test]
    fn every_method_is_a_no_op_on_an_empty_set() {
        let helper = BoundaryConditionSchemeHelper::new(Vec::new());
        let mut op = ScaleOp { factor: 3.0 };
        let mut values = Array::from([1.5, -2.0, 4.0]);
        let before = values.clone();

        helper.set_time(0.5);
        helper.apply_before_applying(&mut op);
        helper.apply_after_applying(&mut values);
        helper.apply_before_solving(&mut op, &mut values);
        helper.apply_after_solving(&mut values);

        assert_eq!(values, before);
        assert_eq!(op.factor, 3.0);
    }

    #[test]
    fn operator_calls_reach_every_condition_in_order() {
        let log = shared(RefCell::new(Vec::new()));
        let helper = recorders(&log);
        let mut op = ScaleOp { factor: 7.0 };
        let mut values = Array::from([0.0]);

        helper.apply_before_applying(&mut op);
        helper.apply_before_solving(&mut op, &mut values);

        assert_eq!(
            *log.borrow(),
            vec![
                "before_applying:1:7".to_string(),
                "before_applying:2:7".to_string(),
                "before_solving:1:7".to_string(),
                "before_solving:2:7".to_string(),
            ]
        );
        assert_eq!(values[0], 12.0);
    }

    #[test]
    fn array_calls_compose_over_the_set_in_order() {
        let log = shared(RefCell::new(Vec::new()));
        let helper = recorders(&log);
        let mut applied = Array::from([0.0]);
        let mut solved = Array::from([0.0]);

        helper.apply_after_applying(&mut applied);
        helper.apply_after_solving(&mut solved);
        helper.set_time(0.25);

        assert_eq!(applied[0], 12.0);
        assert_eq!(solved[0], 12.0);
        assert_eq!(
            *log.borrow(),
            vec![
                "after_applying:1".to_string(),
                "after_applying:2".to_string(),
                "after_solving:1".to_string(),
                "after_solving:2".to_string(),
                "set_time:1:0.25".to_string(),
                "set_time:2:0.25".to_string(),
            ]
        );
    }

    #[test]
    fn the_set_holds_distinct_condition_types() {
        let log = shared(RefCell::new(Vec::new()));
        let helper = BoundaryConditionSchemeHelper::new(vec![
            shared(Recorder {
                id: 1.0,
                log: Shared::clone(&log),
            }),
            shared(Silent),
        ]);
        let mut values = Array::from([0.0]);

        helper.apply_after_applying(&mut values);

        assert_eq!(*log.borrow(), vec!["after_applying:1".to_string()]);
        assert_eq!(values[0], 1.0);
    }

    #[test]
    fn a_shared_operator_reaches_the_conditions() {
        let log = shared(RefCell::new(Vec::new()));
        let helper = recorders(&log);
        let op: SharedMut<ScaleOp> = shared_mut(ScaleOp { factor: 5.0 });

        helper.apply_before_applying(&mut *op.borrow_mut());

        assert_eq!(
            *log.borrow(),
            vec![
                "before_applying:1:5".to_string(),
                "before_applying:2:5".to_string(),
            ]
        );
    }
}
