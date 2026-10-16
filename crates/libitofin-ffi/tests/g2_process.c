#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

int main(void) {
    struct ItofinContext *ctx = NULL;
    struct ItofinError error = {0, {0}};
    uint64_t id = 91;
    assert(sizeof(struct ItofinG2Vector) == 2 * sizeof(double));
    assert(sizeof(struct ItofinG2Matrix) == 4 * sizeof(double));
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_g2_process_new(ctx, 0.1, 0.03, 0.4, 0.007, -0.6, &id, &error) == 0);
    double scalar = 91;
    assert(itofin_g2_process_scalar(ctx, id, 0, &scalar, &error) == 0);
    assert(scalar == 2);
    struct ItofinG2Vector x = {{0.015, -0.007}};
    struct ItofinG2Vector draws = {{0.75, -0.4}};
    struct ItofinG2Vector result = {{91, 92}};
    struct ItofinG2Matrix root = {{91, 92, 93, 94}};
    struct ItofinG2Matrix cov = {{91, 92, 93, 94}};
    assert(itofin_g2_process_matrix(ctx, id, 1, 0, x, 0.8, &root, &error) == 0);
    assert(itofin_g2_process_matrix(ctx, id, 2, 0, x, 0.8, &cov, &error) == 0);
    assert(fabs(cov.values[0] - root.values[0]*root.values[0]) < 1e-18);
    assert(fabs(cov.values[1] - root.values[0]*root.values[2]) < 1e-18);
    assert(cov.values[1] == cov.values[2]);
    assert(itofin_g2_process_vector(ctx, id, 4, 0, x, 0.8, draws, &result, &error) == 0);
    assert(fabs(result.values[0] - (x.values[0]*exp(-0.08) + root.values[0]*0.75)) < 1e-16);
    result.values[0] = 91;
    result.values[1] = 92;
    assert(itofin_g2_process_vector(ctx, id, 4, 0, x, -1, draws, &result, &error) == ITOFIN_CORE_ERROR);
    assert(result.values[0] == 91 && result.values[1] == 92);
    assert(itofin_g2_process_matrix(ctx, id, 1, 0, x, 1, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    assert(itofin_g2_process_scalar(ctx, id, 0, &scalar, &error) == ITOFIN_INVALID_HANDLE);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
