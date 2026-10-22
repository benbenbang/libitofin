#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>

int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error;
    uint64_t id = 0;
    double out = 999.0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_black_delta_new(ctx, 0, ITOFIN_DELTA_PA_FWD,
        1.421, .997306, .992266, .1180654, &id, &error) == 0);
    assert(itofin_black_delta_delta_from_strike(ctx, id, 1.6, &out, &error) == 0);
    assert(fabs(out - .15185269009639454) < 1e-14);
    assert(itofin_black_delta_strike_from_delta(ctx, id, .15, &out, &error) == 0);
    assert(fabs(out - 1.6015501875673384) < 1e-10);
    assert(itofin_black_delta_atm_strike(ctx, id, ITOFIN_ATM_DELTA_NEUTRAL, &out, &error) == 0);
    assert(fabs(out - 1.4039991534456169) < 1e-14);
    out = 999.;
    assert(itofin_black_delta_atm_strike(ctx, id, ITOFIN_ATM_PUT_CALL_50, &out, &error) != 0);
    assert(out == 999.);
    assert(itofin_black_delta_atm_strike(ctx, id, INT32_MAX, &out, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(out == 999.);
    assert(itofin_black_delta_delta_from_strike(ctx, id, NAN, &out, &error) != 0);
    assert(out == 999.);
    assert(itofin_black_delta_delta_from_strike(ctx, id, 1.6, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    assert(itofin_black_delta_atm_strike(ctx, id, ITOFIN_ATM_FWD, &out, &error) == ITOFIN_INVALID_HANDLE);
    assert(out == 999.);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
