#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>

static double cdf(double x) { return erfc(-x / sqrt(2.)) / 2.; }
static void near(double actual, double expected) {
    assert(fabs(actual - expected) < 2e-11 * fmax(1., fabs(expected)));
}

int main(void) {
    ItofinContext *ctx = NULL, *other = NULL;
    ItofinError error;
    uint64_t id = 0, wrong = 0, sentinel = 123;
    double out = 999.;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_context_new(&other, &error) == 0);
    for (int option = 0; option < 2; ++option) {
        double sign = option == 0 ? 1. : -1.;
        double forward = 110., strike = 100., deviation = .3, discount = .94;
        double spot = 97., maturity = 2.;
        double d1 = log(forward / strike) / deviation + deviation / 2.;
        double d2 = d1 - deviation;
        double n = exp(-d1 * d1 / 2.) / sqrt(2. * acos(-1.));
        double alpha = sign * cdf(sign * d1), beta = -sign * cdf(sign * d2);
        double value = discount * (forward * alpha + strike * beta);
        double delta = discount * alpha, gamma = discount * n / (forward * deviation);
        double r = -log(discount) / maturity;
        double q = r - log(forward / spot) / maturity;
        double theta = -discount * forward * n * deviation / (2. * maturity)
            + q * discount * forward * alpha + r * discount * strike * beta;
        assert(itofin_black_calculator_new(ctx, option, strike, forward, deviation, discount, &id, &error) == 0);
        assert(itofin_black_calculator_value(ctx, id, &out, &error) == 0); near(out, value);
        assert(itofin_black_calculator_alpha(ctx, id, &out, &error) == 0); near(out, alpha);
        assert(itofin_black_calculator_beta(ctx, id, &out, &error) == 0); near(out, beta);
        assert(itofin_black_calculator_delta_forward(ctx, id, &out, &error) == 0); near(out, delta);
        assert(itofin_black_calculator_delta(ctx, id, spot, &out, &error) == 0); near(out, delta * forward / spot);
        assert(itofin_black_calculator_gamma_forward(ctx, id, &out, &error) == 0); near(out, gamma);
        assert(itofin_black_calculator_gamma(ctx, id, spot, &out, &error) == 0); near(out, gamma * forward * forward / (spot * spot));
        assert(itofin_black_calculator_elasticity_forward(ctx, id, &out, &error) == 0); near(out, delta * forward / value);
        assert(itofin_black_calculator_elasticity(ctx, id, spot, &out, &error) == 0); near(out, delta * forward / value);
        assert(itofin_black_calculator_theta(ctx, id, spot, maturity, &out, &error) == 0); near(out, theta);
        assert(itofin_black_calculator_theta_per_day(ctx, id, spot, maturity, &out, &error) == 0); near(out, theta / 365.);
        assert(itofin_black_calculator_vega(ctx, id, maturity, &out, &error) == 0); near(out, discount * forward * n * sqrt(maturity));
        assert(itofin_black_calculator_rho(ctx, id, maturity, &out, &error) == 0); near(out, -maturity * discount * strike * beta);
        assert(itofin_black_calculator_dividend_rho(ctx, id, maturity, &out, &error) == 0); near(out, -maturity * discount * forward * alpha);
        assert(itofin_black_calculator_itm_cash_probability(ctx, id, &out, &error) == 0); near(out, cdf(d2));
        assert(itofin_black_calculator_itm_asset_probability(ctx, id, &out, &error) == 0); near(out, cdf(d1));
        assert(itofin_black_calculator_strike_sensitivity(ctx, id, &out, &error) == 0); near(out, discount * beta);
        out = 999.;
        assert(itofin_black_calculator_value(other, id, &out, &error) == ITOFIN_INVALID_HANDLE); assert(out == 999.);
        assert(itofin_black_calculator_delta(ctx, id, NAN, &out, &error) != 0); assert(out == 999.);
        assert(itofin_black_calculator_vega(ctx, id, -1., &out, &error) != 0); assert(out == 999.);
        assert(itofin_black_calculator_value(ctx, id, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
        assert(itofin_handle_release(ctx, id, &error) == 0);
        assert(itofin_black_calculator_value(ctx, id, &out, &error) == ITOFIN_INVALID_HANDLE); assert(out == 999.);
    }
    assert(itofin_black_calculator_new(ctx, 999, 100, 100, .2, 1, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT); assert(sentinel == 123);
    assert(itofin_black_calculator_new(ctx, 0, 100, INFINITY, .2, 1, &sentinel, &error) != 0); assert(sentinel == 123);
    assert(itofin_black_calculator_new(ctx, 0, 0, 100, .2, .95, &id, &error) == 0);
    out = 999.;
    assert(itofin_black_calculator_gamma(ctx, id, 100, &out, &error) == ITOFIN_INVALID_ARGUMENT); assert(out == 999.);
    assert(itofin_black_calculator_vega(ctx, id, 2, &out, &error) == ITOFIN_INVALID_ARGUMENT); assert(out == 999.);
    assert(itofin_black_calculator_strike_sensitivity(ctx, id, &out, &error) == ITOFIN_INVALID_ARGUMENT); assert(out == 999.);
    assert(itofin_black_calculator_theta(ctx, id, 100, 2, &out, &error) == ITOFIN_INVALID_ARGUMENT); assert(out == 999.);
    assert(itofin_black_calculator_theta_per_day(ctx, id, 100, 2, &out, &error) == ITOFIN_INVALID_ARGUMENT); assert(out == 999.);
    assert(itofin_black_calculator_value(ctx, id, &out, &error) == 0); near(out, 95.);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    assert(itofin_black_calculator_new(ctx, 1, 100, 110, 0, .95, &id, &error) == 0);
    assert(itofin_black_calculator_delta_forward(ctx, id, &out, &error) == 0); near(out, 0.);
    assert(itofin_black_calculator_theta(ctx, id, NAN, 0, &out, &error) == 0); near(out, 0.);
    assert(itofin_handle_release(ctx, id, &error) == 0);
    assert(itofin_quote_new(ctx, 1., &wrong, &error) == 0);
    out = 999.;
    assert(itofin_black_calculator_value(ctx, wrong, &out, &error) == ITOFIN_INVALID_HANDLE); assert(out == 999.);
    assert(itofin_handle_release(ctx, wrong, &error) == 0);
    assert(itofin_context_free(other, &error) == 0);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
