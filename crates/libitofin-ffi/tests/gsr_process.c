#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

int main(void) {
    struct ItofinContext *ctx = NULL;
    struct ItofinError error = {0, {0}};
    const double sigma = 0.03, a = 0;
    uint64_t id = 91;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_gsr_process_new(ctx, NULL, 0, &sigma, 1, &a, 1, 10, &id, &error) == 0);
    double out = 91;
    assert(itofin_gsr_process_query(ctx, id, 8, 2, 0.1, 0.8, 0, &out, &error) == 0);
    assert(fabs(out - (0.1 + sigma*sigma*(2*0.8 - 0.8*(10-2-0.8)))) < 1e-16);
    assert(itofin_gsr_process_set_forward_measure_time(ctx, id, 0, &error) == 0);
    assert(itofin_gsr_process_query(ctx, id, 11, 0, 0.3, 0, 4, &out, &error) == 0);
    assert(out == 0.3);
    assert(itofin_gsr_process_set_forward_measure_time(ctx, id, NAN, &error) == ITOFIN_CORE_ERROR);
    assert(itofin_gsr_process_query(ctx, id, 1, 0, 0, 0, 0, &out, &error) == 0);
    assert(out == 0);
    out = 91;
    assert(itofin_gsr_process_query(ctx, id, 8, 0, 0, 1, 0, &out, &error) == ITOFIN_CORE_ERROR);
    assert(out == 91);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    assert(itofin_gsr_process_query(ctx, id, 0, 0, 0, 0, 0, &out, &error) == ITOFIN_INVALID_HANDLE);
    assert(out == 91);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
