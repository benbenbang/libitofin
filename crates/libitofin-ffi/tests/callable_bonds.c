#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

static void near(double got, double want) {
    assert(isfinite(got));
    assert(fabs(got - want) <= 2e-12 * fmax(1.0, fabs(want)));
}
int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, calendar = 0, dc = 0, schedule = 0, fixed = 0, zero = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, 45306, &error) == 0);
    assert(itofin_calendar_new(ctx, 0, &calendar, &error) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc, &error) == 0);
    ItofinScheduleConfig sc = {45306, 46402, 1, calendar, 2, 0, 2};
    assert(itofin_schedule_new(ctx, sc, &schedule, &error) == 0);
    ItofinBondConfig common = {2, 1000.0, 100.0, settings, 45306, 1, 0};
    ItofinFixedBondConfig config = {common, schedule, dc, 0, 0, 0, 0, 2, 0, 0};
    ItofinCallability calls[3] = {{100.0, 0, 0, 45672}, {95.0, 1, 1, 45672}, {0.0, 1, 0, 45306}};
    ItofinCallability checked = {0.0, 0, 0, 0};
    assert(itofin_callability_new(calls[0], &checked, &error) == 0);
    assert(checked.price == 100.0 && checked.call_type == 0 && checked.price_type == 0);
    const double coupons[1] = {0.05};
    assert(itofin_callable_fixed_rate_bond_new(ctx, config, coupons, 1, calls, 3, &fixed, &error) == 0);
    common.calendar = calendar;
    assert(itofin_callable_zero_coupon_bond_new(ctx, common, 46402, dc, calls, 3, &zero, &error) == 0);
    calls[0].price = 999.0;
    size_t count = 0;
    ItofinCallability copied[3] = {{0.0, 0, 0, 0}, {0.0, 0, 0, 0}, {0.0, 0, 0, 0}};
    assert(itofin_callable_bond_callability(ctx, fixed, NULL, 0, &count, &error) == 0 && count == 3);
    assert(itofin_callable_bond_callability(ctx, fixed, copied, 3, &count, &error) == 0);
    assert(copied[0].price == 100.0 && copied[1].call_type == 1 && copied[2].date == 45306);
    assert(itofin_handle_release(ctx, schedule, &error) == 0);
    assert(itofin_handle_release(ctx, calendar, &error) == 0);
    double value = -123.0;
    assert(itofin_bond_real(ctx, fixed, 0, 0, &value, &error) == ITOFIN_CORE_ERROR);
    assert(value == -123.0);
    assert(itofin_bond_real(ctx, fixed, 4, 0, &value, &error) == 0);
    near(value, 0.02739726027396472);
    ItofinBondYieldAnalyticsConfig analytics = {dc, 1, 1, 0};
    assert(itofin_bond_clean_price_from_yield(ctx, fixed, 0.047, analytics, &value, &error) == 0);
    near(value, 100.8292095622104);
    assert(itofin_callable_zero_bond_day_counter(ctx, zero, &dc, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, 46403, &error) == 0);
    assert(itofin_bond_real(ctx, fixed, 3, 0, &value, &error) == 0);
    near(value, 0.0);
    assert(itofin_bond_real(ctx, zero, 3, 0, &value, &error) == 0);
    near(value, 0.0);
    assert(itofin_handle_release(ctx, fixed, &error) == 0);
    assert(itofin_handle_release(ctx, fixed, &error) == ITOFIN_INVALID_HANDLE);
    uint64_t country = 0, long_schedule = 0, beyond = 0;
    int32_t future = 0;
    assert(itofin_date_new(15, 1, 2045, &future, &error) == 0);
    assert(itofin_calendar_country_new(ctx, 25, 0, &country, &error) == 0);
    common.calendar = country;
    common.payment_convention = 2;
    assert(itofin_callable_zero_coupon_bond_new(ctx, common, future, dc, NULL, 0, &beyond, &error) == 0);
    common.payment_convention = 1;
    uint64_t untouched = 991;
    assert(itofin_callable_zero_coupon_bond_new(ctx, common, future, dc, NULL, 0, &untouched, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(untouched == 991);
    sc.end = future;
    sc.calendar = country;
    assert(itofin_schedule_new(ctx, sc, &long_schedule, &error) == 0);
    common.calendar = 0;
    common.payment_convention = 2;
    config.bond = common;
    config.schedule = long_schedule;
    config.has_ex_coupon = 1;
    config.ex_coupon_length = 7;
    assert(itofin_callable_fixed_rate_bond_new(ctx, config, coupons, 1, NULL, 0, &beyond, &error) == 0);
    config.ex_coupon_calendar = country;
    assert(itofin_callable_fixed_rate_bond_new(ctx, config, coupons, 1, NULL, 0, &untouched, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(untouched == 991);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
