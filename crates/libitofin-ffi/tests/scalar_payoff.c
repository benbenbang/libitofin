#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdio.h>

int main(int argc, char **argv) {
    assert(argc == 2);
    FILE *input = fopen(argv[1], "r");
    assert(input);
    char header[128];
    assert(fgets(header, sizeof header, input));
    ItofinContext *ctx = NULL;
    ItofinError error;
    assert(itofin_context_new(&ctx, &error) == 0);
    int kind, option, count = 0;
    double strike, extra, price, expected;
    while (fscanf(input, "%d %d %lf %lf %lf %lf", &kind, &option, &strike, &extra, &price, &expected) == 6) {
        uint64_t payoff = 0;
        assert(itofin_scalar_payoff_new(ctx, kind, option, strike, extra, &payoff, &error) == 0);
        ItofinScalarPayoffInfo info;
        assert(itofin_scalar_payoff_info(ctx, payoff, &info, &error) == 0);
        assert(info.kind == kind && info.option_kind == option && info.strike == strike && info.extra == extra);
        double value = -123.0;
        assert(itofin_scalar_payoff_value(ctx, payoff, price, &value, &error) == 0);
        assert(value == expected);
        assert(itofin_handle_release(ctx, payoff, &error) == 0);
        value = 37.0;
        assert(itofin_scalar_payoff_value(ctx, payoff, price, &value, &error) == ITOFIN_INVALID_HANDLE);
        assert(value == 37.0);
        ++count;
    }
    assert(feof(input));
    assert(fclose(input) == 0);
    assert(count == 384);
    uint64_t output = 123;
    assert(itofin_scalar_payoff_new(ctx, 5, 0, 1.0, 0.0, &output, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(output == 123);
    assert(itofin_scalar_payoff_new(ctx, 0, 7, 1.0, 0.0, &output, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(output == 123);
    assert(itofin_scalar_payoff_new(ctx, 0, 0, 1.0, 0.0, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    uint64_t payoff;
    assert(itofin_scalar_payoff_new(ctx, 0, 0, 100.0, 0.0, &payoff, &error) == 0);
    double value = 91.0;
    assert(itofin_scalar_payoff_value(ctx, payoff, NAN, &value, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(value == 91.0);
    assert(itofin_scalar_payoff_value(ctx, payoff, 101.0, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_scalar_payoff_info(ctx, payoff, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
