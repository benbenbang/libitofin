#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static double number(const char *text) {
    char *end = NULL;
    double result = strtod(text, &end);
    assert(end != text && (*end == 0 || *end == '\r' || *end == '\n'));
    assert(isfinite(result));
    return result;
}

static void ordinary_row(char **fields) {
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, dc = 0, process = 0, engine = 0, option = 0;
    int32_t today = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_date_new(15, 6, 2026, &today, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, today, &error) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
    assert(itofin_black_scholes_new(ctx, number(fields[3]), number(fields[4]), number(fields[5]), number(fields[6]), today, dc, &process, &error) == 0);
    ItofinFdConfig cfg = {15, (uint64_t)number(fields[10]), (uint64_t)number(fields[11]), (uint64_t)number(fields[12]), 0};
    assert(itofin_fd_black_scholes_engine_new(ctx, process, cfg, &engine, &error) == 0);
    const int32_t kind = strcmp(fields[2], "call") == 0 ? 0 : 1;
    assert(strcmp(fields[2], "call") == 0 || strcmp(fields[2], "put") == 0);
    assert(itofin_option_asset_or_nothing_new(ctx, kind, number(fields[8]), today + (int32_t)number(fields[7]), settings, &option, &error) == 0);
    double out = -123;
    assert(itofin_option_value(ctx, option, 0, &out, &error) != 0 && out == -123);
    assert(itofin_option_set_engine(ctx, option, process, 0, 0, &error) == 0);
    assert(itofin_option_value(ctx, option, 0, &out, &error) != 0 && out == -123);
    assert(itofin_option_set_engine(ctx, option, engine, 2, 0, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    assert(itofin_handle_release(ctx, dc, &error) == 0);
    assert(itofin_handle_release(ctx, process, &error) == 0);
    assert(itofin_handle_release(ctx, engine, &error) == 0);
    for (int32_t field = 0; field < 4; ++field) {
        assert(itofin_option_value(ctx, option, field, &out, &error) == 0);
        const double want = number(fields[13 + field]);
        assert(isfinite(out) && fabs(out - want) <= (field == 3 ? 1e-6 : 1e-8));
    }
    for (int32_t field = 4; field < 9; ++field) {
        out = -123;
        assert(itofin_option_value(ctx, option, field, &out, &error) != 0 && out == -123);
    }
    assert(itofin_handle_release(ctx, option, &error) == 0);
    out = -123;
    assert(itofin_option_value(ctx, option, 0, &out, &error) == ITOFIN_INVALID_HANDLE && out == -123);
    assert(itofin_context_free(ctx, &error) == 0);
}

int main(void) {
    FILE *input = fopen("crates/itofin-py/tests/fixtures/asset-digital/asset_fd_reference.csv", "rb");
    assert(input != NULL);
    char line[2048];
    assert(fgets(line, sizeof(line), input) != NULL);
    assert(strncmp(line, "id,family,kind,S,r,q,sigma,days,K,L,", strlen("id,family,kind,S,r,q,sigma,days,K,L,")) == 0);
    unsigned ordinary = 0, edges = 0;
    while (fgets(line, sizeof(line), input)) {
        char *fields[18] = {line};
        unsigned count = 1;
        for (char *cursor = line; *cursor; ++cursor) {
            if (*cursor == ',') {
                assert(count < 18);
                *cursor = 0;
                fields[count++] = cursor + 1;
            }
        }
        assert(count == 18 && strcmp(fields[1], "asset") == 0);
        if (number(fields[7]) > 0 && number(fields[6]) > 0 && number(fields[8]) > 0) {
            ordinary_row(fields);
            ++ordinary;
        } else {
            ++edges;
        }
    }
    assert(fclose(input) == 0);
    assert(ordinary == 40 && edges == 24);
    return 0;
}
