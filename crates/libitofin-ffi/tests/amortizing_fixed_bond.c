#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

static int32_t date(int day, int year) {
    int32_t serial = 0;
    assert(itofin_date_new(day, 1, year, &serial, NULL) == 0);
    return serial;
}
static void near(double got, double want) {
    assert(isfinite(got) && isfinite(want) && fabs(got - want) <= 2e-10);
}
int main(void) {
    ItofinContext *ctx = NULL, *foreign = NULL;
    uint64_t settings = 0, calendar = 0, dc = 0, schedule = 0, bond = 0;
    assert(itofin_context_new(&ctx, NULL) == 0);
    assert(itofin_context_new(&foreign, NULL) == 0);
    assert(itofin_settings_new(ctx, &settings, NULL) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, date(18, 2024), NULL) == 0);
    assert(itofin_calendar_new(ctx, 2, &calendar, NULL) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, NULL) == 0);
    ItofinScheduleConfig sc = {date(17, 2024), date(17, 2027), 0, calendar, 2, 0, 2};
    assert(itofin_schedule_new(ctx, sc, &schedule, NULL) == 0);
    ItofinAmortizingFixedBondConfig a;
    memset(&a, 0, sizeof(a));
    a.settlement_days = 2;
    a.settings = settings;
    a.issue_date = date(17, 2024);
    a.payment_convention = 1;
    a.calendar = calendar;
    a.schedule = schedule;
    a.day_counter = dc;
    a.ex_coupon_convention = 2;
    double notionals[] = {1000., 750., 400.}, coupons[] = {.045, .052, .039};
    uint64_t sentinel = 987;
    assert(itofin_amortizing_fixed_rate_bond_new(ctx, a, NULL, 3, coupons, 3, &sentinel, NULL) == ITOFIN_INVALID_ARGUMENT);
    assert(sentinel == 987);
    assert(itofin_amortizing_fixed_rate_bond_new(foreign, a, notionals, 3, coupons, 3, &sentinel, NULL) == ITOFIN_INVALID_HANDLE);
    assert(sentinel == 987);
    for (size_t i = 0; i < 3; ++i) {
        double saved = notionals[i];
        notionals[i] = NAN;
        assert(itofin_amortizing_fixed_rate_bond_new(ctx, a, notionals, 3, coupons, 3, &sentinel, NULL) != 0);
        assert(sentinel == 987);
        notionals[i] = saved;
    }
    assert(itofin_amortizing_fixed_rate_bond_new(ctx, a, notionals, 3, coupons, 3, &bond, NULL) == 0);
    uint64_t curve = 0, engine = 0;
    assert(itofin_flat_forward_new(ctx, date(18, 2024), 0., dc, &curve, NULL) == 0);
    assert(itofin_discounting_bond_engine_new(ctx, curve, settings, -1, &engine, NULL) == 0);
    assert(itofin_bond_set_engine(ctx, bond, engine, NULL) == 0);
    double c1 = 1000 * .045 * 366 / 360, c2 = 750 * .052 * 365 / 360, c3 = 400 * .039 * 365 / 360;
    double npv = 0, dirty = 0;
    assert(itofin_bond_real(ctx, bond, 0, 0, &npv, NULL) == 0);
    near(npv, 1000 + c1 + c2 + c3);
    assert(itofin_bond_real(ctx, bond, 3, 0, &dirty, NULL) == 0);
    near(dirty, npv / 10);
    int32_t maturity = 0;
    assert(itofin_bond_integer(ctx, bond, 2, 0, &maturity, NULL) == 0);
    assert(maturity == date(17, 2027));
    uint64_t principal = 0;
    assert(itofin_bond_handle(ctx, bond, 2, &principal, NULL) == 0);
    size_t count = 0;
    assert(itofin_leg_count(ctx, principal, &count, NULL) == 0 && count == 3);
    int32_t dates[] = {date(17, 2025), date(19, 2026), date(18, 2027)};
    double expected[] = {250, 350, 400};
    for (size_t i = 0; i < count; ++i) {
        uint64_t flow = 0;
        double amount = 0;
        int32_t payment = 0;
        assert(itofin_leg_item(ctx, principal, (int64_t)i, &flow, NULL) == 0);
        assert(itofin_cashflow_amount(ctx, flow, &amount, NULL) == 0);
        assert(itofin_cashflow_date(ctx, flow, &payment, NULL) == 0);
        near(amount, expected[i]);
        assert(payment == dates[i]);
        assert(itofin_handle_release(ctx, flow, NULL) == 0);
    }
    uint64_t forward = 0;
    ItofinBondForwardConfig f = {bond, date(18, 2024), date(19, 2026), 0, 105., 0, dc, calendar, 2, curve, 0};
    assert(itofin_bond_forward_new(ctx, f, &forward, NULL) == 0);
    double quote = 0;
    assert(itofin_bond_forward_value(ctx, forward, 1, &quote, NULL) == 0);
    near(quote, dirty - (c1 + 250 + c2 + 350));
    uint64_t currency = 0, index = 0;
    assert(itofin_currency_new(ctx, 0, &currency, NULL) == 0);
    ItofinIborConfig ic = {12, 2, 0, currency, calendar, 0, 0, 2, false, dc, curve, settings};
    assert(itofin_ibor_new(ctx, (const uint8_t *)"AmortizingBoundary", strlen("AmortizingBoundary"), &ic, false, &index, NULL) == 0);
    ItofinAssetSwapConfig as = {bond, index, 0, dc, 100., 0., 1};
    ItofinError error = {0, {0}};
    assert(itofin_asset_swap_new(ctx, &as, &sentinel, &error) != 0 && sentinel == 987);
    assert(strstr(error.message, "bullet") != NULL);
    assert(itofin_handle_release(ctx, bond, NULL) == 0);
    assert(itofin_leg_count(ctx, principal, &count, NULL) == 0 && count == 3);
    assert(itofin_context_free(ctx, NULL) == 0);
    assert(itofin_context_free(foreign, NULL) == 0);
    return 0;
}
