#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

static int32_t date(int day, int month, int year) {
    int32_t result = 0;
    assert(itofin_date_new(day, month, year, &result, NULL) == 0);
    return result;
}
static void near(double actual, double expected) {
    assert(isfinite(actual) && isfinite(expected));
    assert(fabs(actual - expected) <= 3e-11);
}
int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, cal = 0, limited = 0, dc = 0, currency = 0;
    uint64_t curve = 0, index = 0, schedule = 0, bond = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, date(28, 9, 2026), &error) == 0);
    assert(itofin_calendar_new(ctx, 1, &cal, &error) == 0);
    assert(itofin_calendar_country_new(ctx, 17, 0, &limited, &error) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
    assert(itofin_currency_new(ctx, 0, &currency, &error) == 0);
    assert(itofin_flat_forward_new(ctx, date(28, 9, 2026), 0.04, dc, &curve, &error) == 0);
    ItofinIborConfig ic = {6, 2, 2, currency, cal, cal, limited, 1, false, dc, curve, settings};
    const uint8_t name[] = "AmortizingGuard";
    assert(itofin_ibor_new(ctx, name, sizeof(name) - 1, &ic, true, &index, &error) == 0);
    ItofinScheduleConfig sc = {date(5, 10, 2026), date(5, 10, 2027), 2, cal, 2, 0, 2};
    assert(itofin_schedule_new(ctx, sc, &schedule, &error) == 0);
    ItofinAmortizingFloatingBondConfig config;
    memset(&config, 0, sizeof(config));
    config.schedule = schedule;
    config.index = index;
    config.day_counter = dc;
    config.settings = settings;
    config.payment_convention = 1;
    config.fixing_convention = 3;
    config.ex_coupon_convention = 2;
    double balances[] = {1000, 800, 800, 500};
    union { ItofinError aligned; unsigned char bytes[sizeof(ItofinError) + 8]; } bad_error;
    memset(&bad_error, 0, sizeof(bad_error));
    uint64_t unchanged = 777;
    assert(itofin_amortizing_floating_rate_bond_new(ctx, config, balances, 4, NULL, 0,
        &unchanged, (ItofinError *)(void *)(bad_error.bytes + 1)) == ITOFIN_INVALID_ARGUMENT);
    assert(unchanged == 777);
    for (size_t i = 0; i < sizeof(bad_error.bytes); ++i) { assert(bad_error.bytes[i] == 0); }
    uint32_t bound = (uint32_t)(date(31, 12, 2199) - date(1, 1, 1901));
    for (int kind = 0; kind < 2; ++kind) {
        config.settlement_days = kind == 0 ? bound + 1 : 0;
        config.has_fixing_days = 1;
        config.fixing_days = kind == 1 ? bound + 1 : 0;
        assert(itofin_amortizing_floating_rate_bond_new(ctx, config, balances, 4, NULL, 0, &unchanged, &error) == ITOFIN_INVALID_ARGUMENT);
        assert(unchanged == 777);
    }
    uint64_t oversized_index = 0;
    ic.settlement_days = bound + 1;
    assert(itofin_ibor_new(ctx, name, sizeof(name) - 1, &ic, true, &oversized_index, &error) == 0);
    config.index = oversized_index;
    config.fixing_days = 0;
    assert(itofin_amortizing_floating_rate_bond_new(ctx, config, balances, 4, NULL, 0, &unchanged, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(unchanged == 777);
    config.index = index;
    config.has_fixing_days = 0;
    assert(itofin_handle_release(ctx, oversized_index, &error) == 0);
    assert(itofin_amortizing_floating_rate_bond_new(ctx, config, balances, 4, NULL, 0, &bond, &error) == 0);
    balances[0] = 1;
    uint64_t leg = 0, retained_index = 0, redemption_leg = 0, flows[7] = {0};
    size_t count = 0;
    assert(itofin_bond_handle(ctx, bond, 1, &leg, &error) == 0);
    assert(itofin_bond_handle(ctx, bond, 2, &redemption_leg, &error) == 0);
    assert(itofin_floating_bond_index(ctx, bond, &retained_index, &error) == 0);
    assert(itofin_leg_count(ctx, leg, &count, &error) == 0 && count == 7);
    for (size_t i = 0; i < 7; ++i) {
        assert(itofin_leg_item(ctx, leg, (int64_t)i, &flows[i], &error) == 0);
    }
    double amount = -91;
    assert(itofin_bond_real(ctx, bond, 5, date(5, 1, 2027), &amount, &error) == 0);
    near(amount, 800);
    uint64_t owners[] = {bond, leg, index, schedule, dc, cal, limited, currency, curve};
    for (size_t i = 0; i < sizeof(owners) / sizeof(owners[0]); ++i) {
        assert(itofin_handle_release(ctx, owners[i], &error) == 0);
    }
    const size_t principals[] = {1, 4, 6};
    const double principal_amounts[] = {200, 300, 500};
    for (size_t i = 0; i < 3; ++i) {
        assert(itofin_cashflow_amount(ctx, flows[principals[i]], &amount, &error) == 0);
        near(amount, principal_amounts[i]);
    }
    const size_t coupons[] = {0, 2, 3, 5};
    for (size_t i = 0; i < 4; ++i) {
        amount = -91;
        assert(itofin_cashflow_amount(ctx, flows[coupons[i]], &amount, &error) == ITOFIN_INVALID_ARGUMENT);
        assert(amount == -91);
    }
    assert(itofin_settings_set_evaluation_date(ctx, settings, date(10, 10, 2027), &error) == 0);
    const int32_t starts[] = {date(5, 10, 2026), date(5, 1, 2027), date(5, 4, 2027), date(5, 7, 2027), date(5, 10, 2027)};
    const double copied_balances[] = {1000, 800, 800, 500};
    for (size_t i = 0; i < 4; ++i) {
        amount = -91;
        assert(itofin_cashflow_amount(ctx, flows[coupons[i]], &amount, &error) != 0);
        assert(amount == -91);
        assert(itofin_ibor_add_fixing(ctx, retained_index, starts[i] - 2, 0.037, &error) == 0);
        assert(itofin_cashflow_amount(ctx, flows[coupons[i]], &amount, &error) == 0);
        near(amount, copied_balances[i] * 0.037 * (starts[i + 1] - starts[i]) / 360.0);
    }
    for (size_t i = 0; i < 3; ++i) {
        uint64_t payment = 0;
        assert(itofin_leg_item(ctx, redemption_leg, (int64_t)i, &payment, &error) == 0);
        assert(itofin_cashflow_amount(ctx, payment, &amount, &error) == 0);
        near(amount, principal_amounts[i]);
    }
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
