/* Compile and execute as both C and C++ against the generated public header. */
#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <string.h>

static int32_t shifted_square(size_t userdata, const double *x, size_t n, double *out, ItofinError *error) {
    (void)userdata;
    (void)n;
    (void)error;
    *out = (x[0] - 3.0) * (x[0] - 3.0);
    return 0;
}

static int32_t shifted_gradient(size_t userdata, const double *x, size_t n, double *out, ItofinError *error) {
    (void)userdata;
    (void)n;
    (void)error;
    out[0] = 2.0 * (x[0] - 3.0);
    return 0;
}

static int32_t cancel_first(size_t userdata, const ItofinIterationState *state, bool *stop, ItofinError *error) {
    (void)userdata;
    (void)error;
    *stop = state->nit >= 1;
    return 0;
}

static int releases = 0;
static int constraint_releases = 0;

static void count_release(size_t userdata) {
    (void)userdata;
    releases += 1;
}

static void count_constraint_release(size_t userdata) {
    (void)userdata;
    constraint_releases += 1;
}

static int32_t interval_constraint(size_t userdata, const double *x, size_t n, double *out, size_t dimension, ItofinError *error) {
    (void)userdata;
    (void)n;
    (void)error;
    assert(dimension == 2);
    out[0] = x[0] - 1.0;
    out[1] = 2.0 - x[0];
    return 0;
}

static int32_t interval_jacobian(size_t userdata, const double *x, size_t n, double *out, size_t length, ItofinError *error) {
    (void)userdata;
    (void)x;
    (void)n;
    (void)error;
    assert(length == 2);
    out[0] = 1.0;
    out[1] = -1.0;
    return 0;
}

static void smoke_optimize(void) {
    ItofinError error;
    ItofinObjective objective;
    ItofinOptimizeOptions options;
    ItofinOptimizeResult result;
    double x0 = 0.0;
    double x = 0.0;
    objective.userdata = 0;
    objective.value = shifted_square;
    objective.gradient = NULL;
    objective.callback = NULL;
    objective.release = count_release;
    memset(&options, 0, sizeof options);
    result.x = &x;
    assert(itofin_optimize_nelder_mead(&objective, &x0, 1, &options, &result, &error) == 0);
    assert(result.success && result.status == ITOFIN_OPTIMIZE_CONVERGED_XTOL);
    assert(x > 2.99 && x < 3.01 && result.x == &x);
    objective.callback = cancel_first;
    assert(itofin_optimize_nelder_mead(&objective, &x0, 1, &options, &result, &error) == 0);
    assert(!result.success && result.status == ITOFIN_OPTIMIZE_CANCELLED && result.nit == 1);
    assert(itofin_optimize_nelder_mead(&objective, &x0, 0, &options, &result, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(error.code == ITOFIN_INVALID_ARGUMENT && releases == 3);
    ItofinBfgsOptions bfgs;
    memset(&bfgs, 0, sizeof bfgs);
    objective.callback = NULL;
    objective.gradient = shifted_gradient;
    assert(itofin_optimize_bfgs(&objective, &x0, 1, &bfgs, &result, &error) == 0);
    assert(result.success && result.status == ITOFIN_OPTIMIZE_CONVERGED_GTOL && result.njev > 0);
    assert(x > 2.99 && x < 3.01 && releases == 4);
    ItofinLbfgsbOptions lbfgsb;
    memset(&lbfgsb, 0, sizeof lbfgsb);
    double lower = -INFINITY;
    double upper = 1.0;
    assert(itofin_optimize_lbfgsb(&objective, &x0, 1, &lower, 1, &upper, 1, &lbfgsb, &result, &error) == 0);
    assert(result.success && x > 0.999 && x <= 1.0 && releases == 5);
    assert(itofin_optimize_lbfgsb(&objective, &x0, 1, &lower, 0, &upper, 1, &lbfgsb, &result, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(releases == 6);
    ItofinSlsqpOptions slsqp;
    memset(&slsqp, 0, sizeof slsqp);
    ItofinConstraint constraints[2];
    memset(constraints, 0, sizeof constraints);
    constraints[0].kind = 1;
    constraints[0].dimension = 2;
    constraints[0].fun = interval_constraint;
    constraints[0].jac = interval_jacobian;
    constraints[0].release = count_constraint_release;
    assert(itofin_optimize_slsqp(&objective, &x0, 1, NULL, 0, NULL, 0, constraints, 1, &slsqp, &result, &error) == 0);
    assert(result.success && fabs(x - 2.0) < 1e-6 && releases == 7 && constraint_releases == 1);
    constraints[1] = constraints[0];
    constraints[1].kind = 5;
    assert(itofin_optimize_slsqp(&objective, &x0, 1, NULL, 0, NULL, 0, constraints, 2, &slsqp, &result, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(releases == 8 && constraint_releases == 3);
    assert(itofin_optimize_slsqp(&objective, &x0, 1, NULL, 0, NULL, 0, NULL, 1, &slsqp, &result, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(releases == 9 && constraint_releases == 3);
}

static void smoke_particle_swarm(void) {
    ItofinObjective objective;
    ItofinParticleSwarmOptions options;
    ItofinOptimizeResult result;
    memset(&objective, 0, sizeof(objective));
    memset(&options, 0, sizeof(options));
    memset(&result, 0, sizeof(result));
    ItofinError error;
    double x0 = 0.0, lower = -4.0, upper = 4.0, x = 99.0;
    int before = releases;
    objective.value = shifted_square;
    objective.release = count_release;
    result.x = &x;
    assert(itofin_optimize_particle_swarm(&objective, &x0, 1, &lower, 1, &upper, 1,
        NULL, 0, 0, &options, &result, &error) == 0);
    assert(result.success && fabs(x - 3.0) < 1e-5 && result.njev == 0);
    assert(releases == before + 1);
    ItofinContext *ctx = NULL;
    uint64_t method = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_particle_swarm_new(ctx, &lower, 1, &upper, 1, &options,
        NULL, 0, 0, &method, &error) == 0);
    assert(itofin_particle_swarm_result(ctx, method, 1, &result, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_handle_release(ctx, method, &error) == 0);
    assert(itofin_context_free(ctx, &error) == 0);
}

static void smoke_hybrid_simulated_annealing(void) {
    ItofinObjective objective;
    ItofinHybridSimulatedAnnealingOptions options;
    ItofinOptimizeResult result;
    memset(&objective, 0, sizeof(objective));
    memset(&options, 0, sizeof(options));
    memset(&result, 0, sizeof(result));
    ItofinError error;
    double x0 = 0.0, lower = -4.0, upper = 4.0, x = 99.0;
    int before = releases;
    objective.value = shifted_square;
    objective.release = count_release;
    result.x = &x;
    assert(itofin_optimize_hybrid_simulated_annealing(&objective, &x0, 1, &lower, 1, &upper, 1,
        NULL, &result, &error) == 0);
    assert(result.success && fabs(x - 3.0) < 1e-5 && result.njev == 0);
    assert(releases == before + 1);
    options.has_cooling_rate = true;
    options.cooling_rate = 1.0;
    assert(itofin_optimize_hybrid_simulated_annealing(&objective, &x0, 1, &lower, 1, &upper, 1,
        &options, &result, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(releases == before + 2);
    ItofinContext *ctx = NULL;
    uint64_t method = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_hybrid_simulated_annealing_new(ctx, &lower, 1, &upper, 1, NULL, &method, &error) == 0);
    assert(itofin_hybrid_simulated_annealing_result(ctx, method, 1, &result, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_handle_release(ctx, method, &error) == 0);
    assert(itofin_context_free(ctx, &error) == 0);
}

static void smoke_firefly(void) {
    ItofinObjective objective;
    ItofinFireflyOptions options;
    ItofinOptimizeResult result;
    memset(&objective, 0, sizeof(objective));
    memset(&options, 0, sizeof(options));
    memset(&result, 0, sizeof(result));
    ItofinError error;
    double x0 = 0.0, lower = -4.0, upper = 4.0, x = 99.0;
    int before = releases;
    objective.value = shifted_square;
    objective.release = count_release;
    result.x = &x;
    assert(itofin_optimize_firefly(&objective, &x0, 1, &lower, 1, &upper, 1,
        NULL, 0, 0, NULL, &result, &error) == 0);
    assert(result.success && fabs(x - 3.0) < 1e-5 && result.njev == 0);
    assert(releases == before + 1);
    ItofinContext *ctx = NULL;
    uint64_t method = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_firefly_new(ctx, &lower, 1, &upper, 1, &options,
        NULL, 0, 0, &method, &error) == 0);
    assert(itofin_firefly_result(ctx, method, 1, &result, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_handle_release(ctx, method, &error) == 0);
    assert(itofin_context_free(ctx, &error) == 0);
}

int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error;
    int32_t serial = -1;
    uint64_t settings = 0;
    assert(itofin_abi_version() == 1);
    assert(strlen(itofin_version()) > 0);
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_date_new(31, 2, 2024, &serial, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(error.code == ITOFIN_INVALID_ARGUMENT && error.message[0] != '\0');
    assert(serial == -1);
    assert(itofin_date_new(29, 2, 2024, &serial, &error) == 0);
    assert(error.code == 0 && error.message[0] == '\0');
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, serial, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == ITOFIN_INVALID_HANDLE);
    assert(itofin_context_free(ctx, &error) == 0);
    smoke_optimize();
    smoke_particle_swarm();
    smoke_firefly();
    smoke_hybrid_simulated_annealing();
    return 0;
}
