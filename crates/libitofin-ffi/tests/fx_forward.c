#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

int main(void) {
    ItofinContext *ctx = NULL, *other = NULL;
    ItofinError e = {0, {0}};
    uint64_t base = 0, quote = 0, dc = 0, bc = 0, qc = 0, forward = 0;
    int32_t reference = 0, settlement = 0, delivery = 0, after = 0;
    assert(itofin_context_new(&ctx, &e) == 0);
    assert(itofin_context_new(&other, &e) == 0);
    assert(itofin_currency_new(ctx, 0, &base, &e) == 0);
    assert(itofin_currency_new(ctx, 1, &quote, &e) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc, &e) == 0);
    assert(itofin_date_new(1, 1, 2025, &reference, &e) == 0);
    assert(itofin_date_new(3, 1, 2025, &settlement, &e) == 0);
    assert(itofin_date_new(3, 1, 2026, &delivery, &e) == 0);
    assert(itofin_date_new(4, 1, 2026, &after, &e) == 0);
    assert(itofin_flat_forward_new(ctx, reference, .03, dc, &bc, &e) == 0);
    assert(itofin_flat_forward_new(ctx, reference + 1, .05, dc, &qc, &e) == 0);
    ItofinFxForwardConfig a = {base, quote, 1.1, bc, qc, 1e6, 1.12, settlement, delivery, 0};
    assert(itofin_fx_forward_new(ctx, a, &forward, &e) == 0);
    double got = -123.0;
    assert(itofin_fx_forward_value(ctx, forward, 3, 0, &got, &e) == 0);
    assert(fabs(got - 1.1 * exp(.02)) < 3e-15);
    assert(itofin_fx_forward_value(ctx, forward, 4, reference, &got, &e) == 0);
    assert(fabs(got - 1e6 * (1.1 * exp(.02) - 1.12) * exp(-.05 * 366 / 365)) < 1e-9);
    int32_t integer = -123;
    assert(itofin_fx_forward_integer(ctx, forward, 3, delivery, &integer, &e) == 0 && integer == 0);
    assert(itofin_fx_forward_integer(ctx, forward, 3, after, &integer, &e) == 0 && integer == 1);
    assert(itofin_handle_release(ctx, base, &e) == 0);
    assert(itofin_handle_release(ctx, quote, &e) == 0);
    assert(itofin_handle_release(ctx, bc, &e) == 0);
    assert(itofin_handle_release(ctx, qc, &e) == 0);
    assert(itofin_handle_release(ctx, dc, &e) == 0);
    assert(itofin_fx_forward_value(ctx, forward, 3, 0, &got, &e) == 0);
    uint64_t currency = 0;
    assert(itofin_fx_forward_currency(ctx, forward, 1, &currency, &e) == 0);
    got = -123.0;
    assert(itofin_fx_forward_value(ctx, forward, 99, 0, &got, &e) == ITOFIN_INVALID_ARGUMENT);
    assert(got == -123.0);
    assert(itofin_fx_forward_value(ctx, forward, 4, 0, &got, &e) != 0 && got == -123.0);
    assert(itofin_fx_forward_value(ctx, forward, 3, 0, NULL, &e) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_fx_forward_value(other, forward, 3, 0, &got, &e) == ITOFIN_INVALID_HANDLE && got == -123.0);
    integer = -123;
    assert(itofin_fx_forward_integer(ctx, forward, 99, 0, &integer, &e) == ITOFIN_INVALID_ARGUMENT && integer == -123);
    uint64_t sentinel = 123;
    assert(itofin_fx_forward_currency(ctx, forward, 99, &sentinel, &e) == ITOFIN_INVALID_ARGUMENT && sentinel == 123);
    assert(itofin_handle_release(ctx, forward, &e) == 0);
    assert(itofin_currency_integer(ctx, currency, 0, &integer, &e) == 0 && integer == 840);
    assert(itofin_handle_release(ctx, currency, &e) == 0);
    assert(itofin_fx_forward_value(ctx, forward, 3, 0, &got, &e) == ITOFIN_INVALID_HANDLE && got == -123.0);
    assert(itofin_context_free(other, &e) == 0);
    assert(itofin_context_free(ctx, &e) == 0);
    return 0;
}
