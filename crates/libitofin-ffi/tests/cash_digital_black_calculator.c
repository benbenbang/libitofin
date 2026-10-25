#include "itofin.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static double cdf(double x) { return erfc(-x / sqrt(2.)) / 2.; }
static void near(double actual, double expected) {
    assert(isfinite(actual));
    assert(fabs(actual - expected) <= 2e-11 * fmax(1., fabs(expected)));
}

static int32_t native_query(ItofinContext *ctx, uint64_t id, unsigned column, double *out, ItofinError *error) {
    switch (column) {
    case 0: return itofin_black_calculator_value(ctx, id, out, error);
    case 1: return itofin_black_calculator_alpha(ctx, id, out, error);
    case 2: return itofin_black_calculator_beta(ctx, id, out, error);
    case 3: return itofin_black_calculator_delta_forward(ctx, id, out, error);
    case 4: return itofin_black_calculator_delta(ctx, id, 98., out, error);
    case 5: return itofin_black_calculator_elasticity_forward(ctx, id, out, error);
    case 6: return itofin_black_calculator_elasticity(ctx, id, 98., out, error);
    case 7: return itofin_black_calculator_gamma_forward(ctx, id, out, error);
    case 8: return itofin_black_calculator_gamma(ctx, id, 98., out, error);
    case 9: return itofin_black_calculator_theta(ctx, id, 98., 1.75, out, error);
    case 10: return itofin_black_calculator_theta_per_day(ctx, id, 98., 1.75, out, error);
    case 11: return itofin_black_calculator_vega(ctx, id, 1.75, out, error);
    case 12: return itofin_black_calculator_rho(ctx, id, 1.75, out, error);
    case 13: return itofin_black_calculator_dividend_rho(ctx, id, 1.75, out, error);
    case 14: return itofin_black_calculator_strike_sensitivity(ctx, id, out, error);
    case 15: return itofin_black_calculator_itm_cash_probability(ctx, id, out, error);
    case 16: return itofin_black_calculator_itm_asset_probability(ctx, id, out, error);
    default: assert(0); return -1;
    }
}

static void native_fixture(ItofinContext *ctx) {
    FILE *file = fopen("sdk/go/testdata/cash_digital_black_calculator.tsv", "r");
    assert(file != NULL);
    char line[8192];
    unsigned count = 0, rejected = 0;
    while (fgets(line, sizeof(line), file) != NULL) {
        if (line[0] == '#' || strncmp(line, "id\t", 3) == 0) continue;
        char *fields[27];
        unsigned n = 0;
        for (char *token = strtok(line, "\t\r\n"); token != NULL && n < 27; token = strtok(NULL, "\t\r\n")) fields[n++] = token;
        assert(n == 27);
        assert(strtod(fields[8], NULL) == 98. && strtod(fields[9], NULL) == 1.75);
        uint64_t id = 123;
        ItofinError error;
        int32_t status = itofin_black_calculator_cash_digital_new(ctx, atoi(fields[2]),
            strtod(fields[3], NULL), strtod(fields[4], NULL), strtod(fields[5], NULL),
            strtod(fields[6], NULL), strtod(fields[7], NULL), &id, &error);
        ++count;
        if (strcmp(fields[1], "diagnostic_rejected") == 0) {
            assert(status == ITOFIN_INVALID_ARGUMENT && id == 123); ++rejected; continue;
        }
        assert(status == 0);
        for (unsigned column = 0; column < 17; ++column) {
            double out = 999.;
            status = native_query(ctx, id, column, &out, &error);
            const char *expected = fields[column + 10];
            if (strcmp(expected, "nan") == 0 || strcmp(expected, "+inf") == 0 || strcmp(expected, "-inf") == 0 || strcmp(expected, "error") == 0) {
                assert(status != 0 && out == 999.);
            } else {
                assert(status == 0); near(out, strtod(expected, NULL));
            }
        }
        assert(itofin_handle_release(ctx, id, &error) == 0);
    }
    assert(fclose(file) == 0);
    assert(count == 78 && rejected == 30);
}

int main(void) {
    ItofinContext *ctx = NULL, *other = NULL;
    ItofinError error;
    uint64_t id = 0, sentinel = 123;
    double out = 999.;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_context_new(&other, &error) == 0);
    for (int option = 0; option < 2; ++option) {
        double sign = option == 0 ? 1. : -1.;
        for (int cash = -7; cash <= 7; cash += 7) {
            double forward = 110., strike = 100., deviation = .3, discount = .94;
            double d2 = log(forward / strike) / deviation - deviation / 2.;
            double density = exp(-d2 * d2 / 2.) / sqrt(2. * acos(-1.));
            double delta = discount * cash * sign * density / (forward * deviation);
            assert(itofin_black_calculator_cash_digital_new(ctx, option, strike, cash, forward, deviation, discount, &id, &error) == 0);
            assert(itofin_black_calculator_value(ctx, id, &out, &error) == 0);
            near(out, discount * cash * cdf(sign * d2));
            assert(itofin_black_calculator_delta_forward(ctx, id, &out, &error) == 0); near(out, delta);
            assert(itofin_black_calculator_gamma_forward(ctx, id, &out, &error) == 0);
            near(out, -delta / forward * (1. + d2 / deviation));
            assert(itofin_black_calculator_strike_sensitivity(ctx, id, &out, &error) == 0); near(out, -delta * forward / strike);
            assert(itofin_black_calculator_alpha(ctx, id, &out, &error) == 0); near(out, 0.);
            out = 999.;
            assert(itofin_black_calculator_value(other, id, &out, &error) == ITOFIN_INVALID_HANDLE); assert(out == 999.);
            assert(itofin_handle_release(ctx, id, &error) == 0);
            assert(itofin_black_calculator_value(ctx, id, &out, &error) == ITOFIN_INVALID_HANDLE); assert(out == 999.);
        }
    }
    const double invalid[] = {-1., 0., 1e-20, DBL_EPSILON, NAN, INFINITY};
    for (unsigned i = 0; i < sizeof(invalid) / sizeof(invalid[0]); ++i) {
        assert(itofin_black_calculator_cash_digital_new(ctx, 0, 100, 7, 110, invalid[i], .95, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT);
        assert(sentinel == 123);
    }
    assert(itofin_black_calculator_cash_digital_new(ctx, 9, 100, 7, 110, .2, .95, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT); assert(sentinel == 123);
    assert(itofin_black_calculator_cash_digital_new(ctx, 0, 100, NAN, 110, .2, .95, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT); assert(sentinel == 123);
    assert(itofin_black_calculator_cash_digital_new(ctx, 0, 100, 7, 110, .2, .95, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_black_calculator_cash_digital_new(ctx, 0, 100, 7, 110, nextafter(DBL_EPSILON, INFINITY), .95, &id, &error) == 0);
    assert(itofin_black_calculator_value(ctx, id, &out, &error) == 0); near(out, 6.65);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    assert(itofin_black_calculator_cash_digital_new(ctx, 0, 0, 7, 110, .2, .95, &id, &error) == 0);
    out = 999.;
    assert(itofin_black_calculator_gamma_forward(ctx, id, &out, &error) == ITOFIN_INVALID_ARGUMENT); assert(out == 999.);
    assert(itofin_black_calculator_value(ctx, id, &out, &error) == 0); near(out, 6.65);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    native_fixture(ctx);
    assert(itofin_context_free(other, &error) == 0);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
