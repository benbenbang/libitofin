#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

int main(void) {
    struct ItofinContext *ctx = NULL;
    struct ItofinError error = {0, {0}};
    double times[] = {0.5, 1.5}, vols[] = {0.2, 0.4, 0.1};
    uint64_t id = 91;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_mf_state_process_new(ctx, 0, times, 2, vols, 3, &id, &error) == 0);
    times[0] = 9;
    vols[0] = 9;
    double out = 91;
    assert(itofin_mf_state_process_query(ctx, id, 3, 0, 0, 0, 0, &out, &error) == 0);
    assert(out == 0.2);
    assert(itofin_mf_state_process_query(ctx, id, 6, 0, 0, 1, 0, &out, &error) == 0);
    assert(fabs(out - 0.1) < 1e-16);
    size_t written = 91;
    double copied[3] = {91, 91, 91};
    assert(itofin_mf_state_process_grid(ctx, id, 0, NULL, 0, &written, &error) == 0);
    assert(written == 2);
    assert(itofin_mf_state_process_grid(ctx, id, 0, copied, 3, &written, &error) == 0);
    assert(copied[0] == 0.5 && copied[1] == 1.5 && copied[2] == 91);
    written = 91;
    assert(itofin_mf_state_process_grid(ctx, id, 1, copied, 2, &written, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(written == 91 && copied[0] == 0.5);
    out = 91;
    assert(itofin_mf_state_process_query(ctx, id, 7, 0, 0, 1, NAN, &out, &error) == ITOFIN_CORE_ERROR);
    assert(out == 91);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    assert(itofin_mf_state_process_query(ctx, id, 0, 0, 0, 0, 0, &out, &error) == ITOFIN_INVALID_HANDLE);
    assert(out == 91);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
