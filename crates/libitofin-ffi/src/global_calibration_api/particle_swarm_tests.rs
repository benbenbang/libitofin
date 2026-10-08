use super::*;
use crate::models_api::itofin_simplex_new;
use libitofin::math::array::Array;
use libitofin::math::optimization::constraint::NoConstraint;
use libitofin::math::optimization::costfunction::CostFunction;
use libitofin::math::optimization::endcriteria::EndCriteria;
use libitofin::math::optimization::problem::Problem;

fn error() -> ItofinError {
    ItofinError {
        code: 0,
        message: [0; 1024],
    }
}
fn new_method(context: &mut Context) -> u64 {
    let mut id = 0;
    let mut e = error();
    let status = unsafe {
        itofin_particle_swarm_new(
            context,
            [-1.0].as_ptr(),
            1,
            [1.0].as_ptr(),
            1,
            std::ptr::null(),
            std::ptr::null(),
            0,
            0,
            &mut id,
            &mut e,
        )
    };
    assert_eq!(status, 0);
    id
}
fn empty_result(x: &mut [f64]) -> ItofinOptimizeResult {
    ItofinOptimizeResult {
        x: x.as_mut_ptr(),
        fun: 0.0,
        nit: 0,
        nfev: 0,
        njev: 0,
        status: 0,
        success: false,
    }
}

#[test]
fn calibration_handle_result_is_shared_typed_and_non_evaluating() {
    struct Square;
    impl CostFunction for Square {
        fn values(&self, x: &Array) -> Array {
            Array::from([x[0] * x[0]])
        }
    }
    let mut context = Context::new();
    let id = new_method(&mut context);
    let mut e = error();
    let mut x = [99.0];
    let mut result = empty_result(&mut x);
    assert_eq!(
        unsafe { itofin_particle_swarm_result(&mut context, id, 1, &mut result, &mut e) },
        INVALID_ARGUMENT
    );
    let shared = context
        .get::<SharedMut<dyn OptimizationMethod>>(id)
        .unwrap();
    let mut problem = Problem::new(&Square, &NoConstraint, Array::from([0.5]));
    let criteria = EndCriteria::new(500, Some(20), 1e-8, 1e-8, None).unwrap();
    shared
        .borrow_mut()
        .minimize(&mut problem, &criteria)
        .unwrap();
    let count = problem.function_evaluation();
    assert_eq!(
        unsafe { itofin_particle_swarm_result(&mut context, id, 1, &mut result, &mut e) },
        0
    );
    assert!(result.success);
    assert_eq!(result.nfev, count as usize);
    assert_eq!(problem.function_evaluation(), count);
    assert_eq!(x.to_vec(), shared.borrow().global_result().unwrap().x);
    assert_eq!(
        unsafe { itofin_particle_swarm_result(&mut context, id, 0, &mut result, &mut e) },
        INVALID_ARGUMENT
    );
    let mut foreign = Context::new();
    assert_eq!(
        unsafe { itofin_particle_swarm_result(&mut foreign, id, 1, &mut result, &mut e) },
        INVALID_HANDLE
    );
    let mut local = 0;
    assert_eq!(
        unsafe { itofin_simplex_new(&mut context, 0.1, &mut local, &mut e) },
        0
    );
    assert_eq!(
        unsafe { itofin_particle_swarm_result(&mut context, local, 1, &mut result, &mut e) },
        INVALID_ARGUMENT
    );
    assert_eq!(
        unsafe { itofin_particle_swarm_result(&mut context, id, 1, std::ptr::null_mut(), &mut e) },
        INVALID_ARGUMENT
    );
}

#[test]
fn invalid_constructor_never_writes_handle_or_poison_context() {
    let mut context = Context::new();
    let mut id = 77;
    let mut e = error();
    for (rows, len) in [(1, 0), (4097, 4097), (usize::MAX, usize::MAX)] {
        assert_eq!(
            unsafe {
                itofin_particle_swarm_new(
                    &mut context,
                    [-1.0].as_ptr(),
                    1,
                    [1.0].as_ptr(),
                    1,
                    std::ptr::null(),
                    std::ptr::null(),
                    rows,
                    len,
                    &mut id,
                    &mut e,
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(id, 77);
    }
    assert!(new_method(&mut context) > 0);
}

#[test]
fn particle_swarm_calibration_caps_precede_pointer_reads() {
    let mut context = Context::new();
    let bad = std::ptr::dangling::<f64>();
    let mut id = 77;
    let mut e = error();
    for (dimension, population, maxiter, maxfev) in [
        (257, 0, 0, 0),
        (256, 4096, 0, 0),
        (1, 4097, 0, 0),
        (1, 0, 1_000_001, 0),
        (1, 0, 0, 10_000_001),
    ] {
        let options = ItofinParticleSwarmOptions {
            global: crate::optimize_api::ItofinGlobalOptions {
                population_size: population,
                maxiter,
                maxfev,
                ..Default::default()
            },
            ..Default::default()
        };
        assert_eq!(
            unsafe {
                itofin_particle_swarm_new(
                    &mut context,
                    bad,
                    dimension,
                    bad,
                    dimension,
                    &options,
                    bad,
                    0,
                    0,
                    &mut id,
                    &mut e,
                )
            },
            INVALID_ARGUMENT
        );
        assert_eq!(id, 77);
    }
    assert!(new_method(&mut context) > 0);
}

#[test]
fn particle_swarm_calibration_invalid_coefficient_leaves_handle_untouched() {
    let mut context = Context::new();
    let mut id = 77;
    let mut e = error();
    let options = ItofinParticleSwarmOptions {
        social: 5.0,
        has_social: true,
        ..Default::default()
    };
    assert_eq!(
        unsafe {
            itofin_particle_swarm_new(
                &mut context,
                [-1.0].as_ptr(),
                1,
                [1.0].as_ptr(),
                1,
                &options,
                std::ptr::null(),
                0,
                0,
                &mut id,
                &mut e,
            )
        },
        INVALID_ARGUMENT
    );
    assert_eq!(id, 77);
    assert!(new_method(&mut context) > 0);
}
