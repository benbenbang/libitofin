#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <stdint.h>
#include <string.h>

int main(void) {
    ItofinContext *ctx = NULL, *foreign = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, dc = 0, process = 0, engine = 0;
    int32_t today = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_context_new(&foreign, &error) == 0);
    assert(itofin_date_new(15, 6, 2026, &today, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, today, &error) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
    assert(itofin_black_scholes_new(ctx, 100, .05, .02, .2, today, dc, &process, &error) == 0);
    ItofinFdConfig cfg = {7, 400, 400, 2, 0};
    assert(itofin_fd_black_scholes_engine_new(ctx, process, cfg, &engine, &error) == 0);
    for (int32_t kind = 0; kind < 2; ++kind) {
        uint64_t option = 0, vanilla = 0;
        double value = -123, expected = 0;
        assert(itofin_option_gap_new(ctx, kind, 100, 100, today + 360, settings, &option, &error) == 0);
        assert(itofin_option_value(ctx, option, 0, &value, &error) != 0 && value == -123);
        assert(itofin_option_set_engine(ctx, option, process, 0, 0, &error) == 0);
        assert(itofin_option_value(ctx, option, 0, &value, &error) != 0 && value == -123);
        assert(itofin_option_new(ctx, kind, 100, 0, today + 360, 0, settings, &vanilla, &error) == 0);
        assert(itofin_option_set_engine(ctx, option, engine, 2, 0, &error) == 0);
        assert(itofin_option_set_engine(ctx, vanilla, engine, 2, 0, &error) == 0);
        assert(itofin_option_value(ctx, option, 0, &value, &error) == 0);
        assert(itofin_option_value(ctx, vanilla, 0, &expected, &error) == 0);
        assert(isfinite(value) && isfinite(expected) && fabs(value - expected) < 1e-12);
        for (int32_t field = 1; field < 4; ++field) {
            assert(itofin_option_value(ctx, option, field, &value, &error) == 0 && isfinite(value));
        }
        value = -123;
        assert(itofin_option_value(ctx, option, 4, &value, &error) != 0 && value == -123);
        assert(itofin_handle_release(ctx, option, &error) == 0);
        assert(itofin_handle_release(ctx, vanilla, &error) == 0);
    }
    uint64_t sentinel = 123;
    const double bad[][2] = {{-1, 100}, {NAN, 100}, {INFINITY, 100}, {100, NAN}, {100, INFINITY}, {100, -INFINITY}};
    for (size_t i = 0; i < sizeof(bad) / sizeof(bad[0]); ++i) {
        assert(itofin_option_gap_new(ctx, 0, bad[i][0], bad[i][1], today + 360, settings, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT && sentinel == 123);
    }
    assert(itofin_option_gap_new(ctx, 2, 100, 100, today + 360, settings, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT && sentinel == 123);
    assert(itofin_option_gap_new(ctx, 0, 100, 100, 0, settings, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT && sentinel == 123);
    assert(itofin_option_gap_new(ctx, 0, 100, 100, today + 360, dc, &sentinel, &error) == ITOFIN_INVALID_HANDLE && sentinel == 123);
    assert(itofin_option_gap_new(foreign, 0, 100, 100, today + 360, settings, &sentinel, &error) == ITOFIN_INVALID_HANDLE && sentinel == 123);
    assert(itofin_option_gap_new(ctx, 0, 100, 100, today + 360, settings, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_option_gap_new(NULL, 0, 100, 100, today + 360, settings, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT && sentinel == 123);
    union { uint64_t align; unsigned char bytes[sizeof(ItofinError) + 8]; } storage;
    memset(&storage, 0x5a, sizeof(storage));
    assert(itofin_option_gap_new(ctx, 0, 100, 100, today + 360, settings, &sentinel, (ItofinError *)(storage.bytes + 1)) == ITOFIN_INVALID_ARGUMENT && sentinel == 123);
    for (size_t i = 0; i < sizeof(storage.bytes); ++i) assert(storage.bytes[i] == 0x5a);
    assert(itofin_option_gap_new(ctx, 0, 100, 100, today + 360, settings, (uint64_t *)(storage.bytes + 1), &error) == ITOFIN_INVALID_ARGUMENT);
    uint64_t option = 0;
    assert(itofin_option_gap_new(ctx, 0, 100, 160, today + 360, settings, &option, NULL) == 0);
    assert(itofin_option_set_engine(ctx, option, engine, 2, 0, &error) == 0);
    assert(itofin_handle_release(ctx, engine, &error) == 0);
    assert(itofin_handle_release(ctx, process, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    assert(itofin_handle_release(ctx, dc, &error) == 0);
    double value = 0;
    assert(itofin_option_value(ctx, option, 0, &value, &error) == 0 && isfinite(value) && value < 0);
    assert(itofin_handle_release(ctx, option, &error) == 0);
    value = -123;
    assert(itofin_option_value(ctx, option, 0, &value, &error) == ITOFIN_INVALID_HANDLE && value == -123);
    assert(itofin_context_free(foreign, &error) == 0);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
