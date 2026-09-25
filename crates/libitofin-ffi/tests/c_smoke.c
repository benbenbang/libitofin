/* Compile and execute as both C and C++ against the generated public header. */
#include "itofin.h"
#include <assert.h>
#include <string.h>

static int32_t shifted_square(size_t userdata, const double *x, size_t n, double *out, ItofinError *error) {
    (void)userdata;
    (void)n;
    (void)error;
    *out = (x[0] - 3.0) * (x[0] - 3.0);
    return 0;
}

static int32_t cancel_first(size_t userdata, const ItofinIterationState *state, bool *stop, ItofinError *error) {
    (void)userdata;
    (void)error;
    *stop = state->nit >= 1;
    return 0;
}

static int releases = 0;

static void count_release(size_t userdata) {
    (void)userdata;
    releases += 1;
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
    return 0;
}
