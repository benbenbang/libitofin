#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static size_t checks = 0;
static int current_row = -1;
static size_t qualified_nontradable_queries = 0;
static void nontradable(int32_t status, double actual, const ItofinError *error) {
    assert(status == ITOFIN_CORE_ERROR && error->code == ITOFIN_CORE_ERROR);
    assert(strstr(error->message, "non tradable at ") != NULL);
    assert(strstr(error->message, " settlement date (maturity being ") != NULL);
    assert(actual == -777.0);
    ++qualified_nontradable_queries;
}
static void ok(int32_t status) { assert(status == 0); }
static void number(int32_t status, double actual, int expected_ok, double expected) {
    ++checks;
    if (!expected_ok) {
        if (status == 0 || actual != -777.0) { fprintf(stderr, "row %d expected query error: status=%d actual=%.17g\n", current_row, status, actual); }
        assert(status != 0 && actual == -777.0);
        return;
    }
    if (status != 0 || !isfinite(actual) || !isfinite(expected) || fabs(actual - expected) > 3e-11 + 3e-13 * fabs(expected)) {
        fprintf(stderr, "row %d native observation %zu: status=%d actual=%.17g expected=%.17g\n", current_row, checks, status, actual, expected);
    }
    assert(status == 0 && isfinite(actual) && isfinite(expected));
    assert(fabs(actual - expected) <= 3e-11 + 3e-13 * fabs(expected));
}
static void run(double *v, size_t size) {
    size_t p = 0;
    int row = (int)v[p++], weekend = (int)v[p++];
    current_row = row;
    uint32_t settlement_days = (uint32_t)v[p++];
    int32_t evaluation = (int32_t)v[p++], discount_ref = (int32_t)v[p++], forward_ref = (int32_t)v[p++];
    double discount_quote = v[p++], forward_quote = v[p++];
    uint8_t at_par = (uint8_t)v[p++];
    int32_t ex_days = (int32_t)v[p++], issue = (int32_t)v[p++];
    int32_t dates[5];
    double balances[4];
    for (size_t i = 0; i < 5; ++i) { dates[i] = (int32_t)v[p++]; }
    for (size_t i = 0; i < 4; ++i) { balances[i] = v[p++]; }
    double spread = v[p++];
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, calendar = 0, counter = 0, currency = 0;
    uint64_t discount = 0, forward = 0, index = 0, schedule = 0, bond = 0, engine = 0;
    ok(itofin_context_new(&ctx, &error));
    ok(itofin_settings_new(ctx, &settings, &error));
    ok(itofin_settings_set_evaluation_date(ctx, settings, evaluation, &error));
    ok(itofin_settings_set_using_at_par_coupons(ctx, settings, at_par, &error));
    ok(itofin_settings_set_include_reference_date_events(ctx, settings, 0, &error));
    ok(itofin_settings_set_include_todays_cash_flows(ctx, settings, 0, &error));
    ok(itofin_calendar_new(ctx, weekend ? 2 : 1, &calendar, &error));
    ok(itofin_day_counter_new(ctx, 0, &counter, &error));
    ok(itofin_currency_new(ctx, 1, &currency, &error));
    ok(itofin_flat_forward_new(ctx, discount_ref, discount_quote, counter, &discount, &error));
    ok(itofin_flat_forward_new(ctx, forward_ref, forward_quote, counter, &forward, &error));
    ItofinIborConfig ic = {3, 2, 0, currency, calendar, 0, 0, 3, false, counter, forward, settings};
    const uint8_t name[] = "CAmortizingNative";
    ok(itofin_ibor_new(ctx, name, sizeof(name) - 1, &ic, false, &index, &error));
    size_t fixings = (size_t)v[p++];
    for (size_t i = 0; i < fixings; ++i) {
        int32_t fixing = (int32_t)v[p++];
        double value = v[p++];
        ok(itofin_ibor_add_fixing(ctx, index, fixing, value, &error));
    }
    ItofinScheduleConfig sc = {dates[0], dates[4], 2, calendar, 2, 0, 2};
    ok(itofin_schedule_new(ctx, sc, &schedule, &error));
    ItofinAmortizingFloatingBondConfig config;
    memset(&config, 0, sizeof(config));
    config.settlement_days = settlement_days;
    config.schedule = schedule;
    config.index = index;
    config.day_counter = counter;
    config.settings = settings;
    config.payment_convention = 1;
    config.issue_date = issue;
    config.has_fixing_days = 1;
    config.fixing_convention = 3;
    config.ex_coupon_convention = 2;
    if (ex_days) {
        config.has_ex_coupon = 1;
        config.ex_coupon_length = ex_days;
        config.ex_coupon_unit = 0;
        config.ex_coupon_calendar = calendar;
    }
    ok(itofin_amortizing_floating_rate_bond_new(ctx, config, balances, 4, &spread, 1, &bond, &error));
    ok(itofin_discounting_bond_engine_new(ctx, discount, settings, -1, &engine, &error));
    ok(itofin_bond_set_engine(ctx, bond, engine, &error));
    int32_t maturity = (int32_t)v[p++], settlement = (int32_t)v[p++], expired = (int32_t)v[p++], integer = 0;
    ok(itofin_bond_integer(ctx, bond, 2, 0, &integer, &error));
    assert(integer == maturity);
    ok(itofin_bond_integer(ctx, bond, 1, 0, &integer, &error));
    assert(integer == settlement);
    ok(itofin_bond_integer(ctx, bond, 4, 0, &integer, &error));
    assert(integer == expired);
    double prices[2] = {0, 0}, outstanding = -1;
    for (int32_t field = 0; field < 6; ++field) {
        int expected_ok = (int)v[p++];
        double expected = v[p++], actual = -777;
        int32_t status = itofin_bond_real(ctx, bond, field, 0, &actual, &error);
        number(status, actual, expected_ok, expected);
        if (field == 2 || field == 3) { prices[field - 2] = expected; }
        if (field == 5 && expected_ok) { outstanding = expected; }
    }
    uint64_t leg = 0, redemptions = 0;
    ok(itofin_bond_handle(ctx, bond, 1, &leg, &error));
    ok(itofin_bond_handle(ctx, bond, 2, &redemptions, &error));
    size_t count = 0, expected_count = (size_t)v[p++], principal_count = 0;
    ok(itofin_leg_count(ctx, leg, &count, &error));
    assert(count == expected_count);
    for (size_t i = 0; i < count; ++i) {
        int32_t payment = (int32_t)v[p++];
        principal_count += (size_t)v[p++];
        int expected_ok = (int)v[p++];
        double expected = v[p++], actual = -777;
        uint64_t flow = 0;
        ok(itofin_leg_item(ctx, leg, (int64_t)i, &flow, &error));
        ok(itofin_cashflow_date(ctx, flow, &integer, &error));
        assert(integer == payment);
        int32_t status = itofin_cashflow_amount(ctx, flow, &actual, &error);
        number(status, actual, expected_ok, expected);
    }
    expected_count = (size_t)v[p++];
    ok(itofin_leg_count(ctx, redemptions, &count, &error));
    assert(count == expected_count && count == principal_count);
    for (size_t i = 0; i < count; ++i) {
        int32_t payment = (int32_t)v[p++];
        int expected_ok = (int)v[p++];
        double expected = v[p++], actual = -777;
        uint64_t flow = 0;
        ok(itofin_leg_item(ctx, redemptions, (int64_t)i, &flow, &error));
        ok(itofin_cashflow_date(ctx, flow, &integer, &error));
        assert(integer == payment);
        int32_t status = itofin_cashflow_amount(ctx, flow, &actual, &error);
        number(status, actual, expected_ok, expected);
    }
    count = (size_t)v[p++];
    for (size_t i = 0; i < count; ++i) {
        int32_t on = (int32_t)v[p++];
        int expected_ok = (int)v[p++];
        double expected = v[p++], actual = -777;
        int32_t status = itofin_bond_real(ctx, bond, 5, on, &actual, &error);
        number(status, actual, expected_ok, expected);
    }
    double yield_values[2] = {0, 0};
    ItofinBondYieldAnalyticsConfig analytics = {counter, 1, 1, settlement};
    ItofinAnalyticsSolverConfig solver = {7, 1e-12, 200, 0.03};
    for (int kind = 0; kind < 4; ++kind) {
        int expected_ok = (int)v[p++];
        double expected = v[p++], actual = -777;
        if (expected_ok < 0) { continue; }
        int32_t status;
        if (kind < 2) {
            status = itofin_bond_yield_rate(ctx, bond, prices[kind], kind, analytics, solver, &actual, &error);
            yield_values[kind] = outstanding == 0.0 ? expected : actual;
        } else if (kind == 2) {
            status = itofin_bond_clean_price_from_yield(ctx, bond, yield_values[0], analytics, &actual, &error);
        } else {
            status = itofin_bond_dirty_price_from_yield(ctx, bond, yield_values[1], analytics, &actual, &error);
        }
        if (outstanding == 0.0) {
            assert(kind == 2 ? expected_ok == 0 : expected_ok == 1 && expected == 0.0);
            nontradable(status, actual, &error);
        } else {
            number(status, actual, expected_ok, expected);
        }
    }
    if (row == 5) {
        assert(outstanding == 0.0);
        ItofinFloatingBondConfig bullet_config;
        memset(&bullet_config, 0, sizeof(bullet_config));
        bullet_config.bond.settlement_days = settlement_days;
        bullet_config.bond.face_amount = balances[0];
        bullet_config.bond.redemption = 100.0;
        bullet_config.bond.settings = settings;
        bullet_config.bond.issue_date = issue;
        bullet_config.bond.payment_convention = 1;
        bullet_config.schedule = schedule;
        bullet_config.index = index;
        bullet_config.day_counter = counter;
        bullet_config.has_fixing_days = 1;
        bullet_config.fixing_convention = 3;
        bullet_config.ex_coupon_convention = 2;
        ItofinFloatingBondVectors vectors;
        memset(&vectors, 0, sizeof(vectors));
        uint64_t bullet = 0;
        ok(itofin_floating_rate_bond_new(ctx, bullet_config, vectors, &bullet, &error));
        double actual = -777;
        ok(itofin_bond_real(ctx, bullet, 5, settlement, &actual, &error));
        assert(actual == 0.0);
        for (int kind = 0; kind < 2; ++kind) {
            actual = -777;
            int32_t status = itofin_bond_yield_rate(ctx, bullet, 0.0, kind, analytics, solver, &actual, &error);
            nontradable(status, actual, &error);
        }
        actual = -777;
        int32_t status = itofin_bond_clean_price_from_yield(ctx, bullet, 0.0, analytics, &actual, &error);
        nontradable(status, actual, &error);
        actual = -777;
        status = itofin_bond_dirty_price_from_yield(ctx, bullet, 0.0, analytics, &actual, &error);
        nontradable(status, actual, &error);
    }
    assert(p == size);
    assert(row >= 0);
    ok(itofin_context_free(ctx, &error));
}
int main(int argc, char **argv) {
    assert(argc == 2);
    FILE *input = fopen(argv[1], "r");
    assert(input != NULL);
    char line[16384];
    assert(fgets(line, sizeof(line), input) != NULL);
    size_t rows = 0;
    while (fgets(line, sizeof(line), input)) {
        double values[256];
        size_t count = 0;
        for (char *token = strtok(line, "\t\n"); token != NULL; token = strtok(NULL, "\t\n")) {
            assert(count < 256);
            char *end = NULL;
            values[count++] = strtod(token, &end);
            assert(end != token && *end == '\0' && isfinite(values[count - 1]));
        }
        run(values, count);
        ++rows;
    }
    assert(fclose(input) == 0 && rows == 52);
    assert(qualified_nontradable_queries == 52);
    printf("amortizing floating native oracle: %zu rows, %zu numeric observations, %zu specifically qualified inherited nontradable queries\n", rows, checks, qualified_nontradable_queries);
    return 0;
}
