#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <stdint.h>

int main(void) {
    ItofinContext *ctx = NULL, *other = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, dc = 0, process = 0, option = 0;
    int32_t today = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_context_new(&other, &error) == 0);
    assert(itofin_date_new(15, 6, 2026, &today, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, today, &error) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
    assert(itofin_black_scholes_new(ctx, 100, .05, .02, .2, today, dc, &process, &error) == 0);
    const double pi = acos(-1.0);
    const double d2 = (log(100.0 / 105.0) + .03 - .02) / .2;
    const double density = exp(-d2 * d2 / 2) / sqrt(2 * pi);
    for (int32_t kind = 0; kind < 2; ++kind) {
        assert(itofin_option_cash_or_nothing_new(ctx, kind, 105, 12, today + 360, settings, &option, &error) == 0);
        double out = -123;
        assert(itofin_option_value(ctx, option, 0, &out, &error) != 0 && out == -123);
        assert(itofin_option_set_engine(ctx, option, process, 0, 0, &error) == 0);
        const double sign = kind == 0 ? 1 : -1;
        const double value = 12 * exp(-.05) * erfc(-sign * d2 / sqrt(2.0)) / 2;
        const double delta = sign * 12 * exp(-.05) * density / 20;
        const double gamma = -delta / 100 * (1 + d2 / .2);
        const double theta = .05 * value - .03 * 100 * delta - .5 * .2 * .2 * 100 * 100 * gamma;
        const double wants[] = {value, delta, gamma, theta, -100 * delta * (d2 + .2), 100 * delta - value, -100 * delta};
        for (int32_t field = 0; field < 7; ++field) {
            assert(itofin_option_value(ctx, option, field, &out, &error) == 0);
            assert(isfinite(out) && fabs(out - wants[field]) < 2e-12);
        }
        assert(itofin_handle_release(ctx, option, &error) == 0);
        out = -123;
        assert(itofin_option_value(ctx, option, 0, &out, &error) == ITOFIN_INVALID_HANDLE && out == -123);
    }
    uint64_t sentinel = 123;
    const double bad[][2] = {{-1, 1}, {NAN, 1}, {INFINITY, 1}, {1, NAN}, {1, INFINITY}};
    for (size_t i = 0; i < sizeof(bad) / sizeof(bad[0]); ++i) {
        assert(itofin_option_cash_or_nothing_new(ctx, 0, bad[i][0], bad[i][1], today + 360, settings, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT);
        assert(sentinel == 123);
    }
    assert(itofin_option_cash_or_nothing_new(ctx, 2, 100, 1, today + 360, settings, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT && sentinel == 123);
    assert(itofin_option_cash_or_nothing_new(ctx, 0, 100, 1, 0, settings, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT && sentinel == 123);
    assert(itofin_option_cash_or_nothing_new(ctx, 0, 100, 1, today + 360, dc, &sentinel, &error) == ITOFIN_INVALID_HANDLE && sentinel == 123);
    assert(itofin_option_cash_or_nothing_new(other, 0, 100, 1, today + 360, settings, &sentinel, &error) == ITOFIN_INVALID_HANDLE && sentinel == 123);
    assert(itofin_option_cash_or_nothing_new(ctx, 0, 100, 1, today + 360, settings, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_option_cash_or_nothing_new(ctx, 0, 105, -12, today + 360, settings, &option, &error) == 0);
    assert(itofin_option_set_engine(ctx, option, process, 0, 0, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    assert(itofin_handle_release(ctx, process, &error) == 0);
    assert(itofin_handle_release(ctx, dc, &error) == 0);
    double out = 0;
    assert(itofin_option_value(ctx, option, 0, &out, &error) == 0 && out < 0);
    assert(itofin_handle_release(ctx, option, &error) == 0);
    assert(itofin_context_free(other, &error) == 0);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
