#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

int main(void) {
    struct ItofinContext *ctx = NULL;
    struct ItofinError error = {0, {0}};
    uint64_t dc = 0, curve = 0, p = 0;
    double out = 99;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc, &error) == 0);
    assert(itofin_flat_forward_new(ctx, 45000, .03, dc, &curve, &error) == 0);
    assert(itofin_hw_forward_process_new(ctx, curve, 0, .02, &p, &error) == 0);
    assert(itofin_hw_forward_process_set_forward_measure_time(ctx, p, 5, &error) == 0);
    assert(itofin_hw_forward_process_query(ctx, p, 4, 1, 0, 0, 0, &out, &error) == 0);
    assert(fabs(out - .0302) <= 2e-12);
    assert(itofin_hw_forward_process_query(ctx, p, 5, 1, 0, 5, 0, &out, &error) == 0);
    assert(out == 4);
    assert(itofin_hw_forward_process_query(ctx, p, 6, 1, 2, 5, 0, &out, &error) == 0);
    assert(fabs(out - .0014) <= 2e-18);
    assert(itofin_hw_forward_process_query(ctx, p, 9, 1, .03, 1, 0, &out, &error) == 0);
    assert(fabs(out - .0292) <= 2e-12);
    assert(itofin_hw_forward_process_query(ctx, p, 11, 1, .03, 1, 0, &out, &error) == 0);
    assert(out == .0004);
    assert(itofin_handle_release(ctx, curve, &error) == 0);
    assert(itofin_handle_release(ctx, dc, &error) == 0);
    assert(itofin_hw_forward_process_query(ctx, p, 4, 1, 0, 0, 0, &out, &error) == 0);
    assert(fabs(out - .0302) <= 2e-12);
    assert(itofin_hw_forward_process_set_forward_measure_time(ctx, p, NAN, &error) == ITOFIN_CORE_ERROR);
    assert(itofin_hw_forward_process_query(ctx, p, 3, 0, 0, 0, 0, &out, &error) == 0);
    assert(out == 5);
    out = 99;
    assert(itofin_hw_forward_process_query(ctx, p, 9, 0, .03, -1, 0, &out, &error) == ITOFIN_CORE_ERROR);
    assert(out == 99);
    assert(itofin_hw_forward_process_query(ctx, p, 99, 0, 0, 0, 0, &out, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(out == 99);
    assert(itofin_handle_release(ctx, p, &error) == 0);
    assert(itofin_hw_forward_process_query(ctx, p, 0, 0, 0, 0, 0, &out, &error) == ITOFIN_INVALID_HANDLE);
    assert(out == 99);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
