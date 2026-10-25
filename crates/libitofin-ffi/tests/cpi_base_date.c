#include "itofin.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stdint.h>
#include <string.h>

static int32_t date(int day, int month, int year) {
    int32_t result = 0;
    assert(itofin_date_new(day, month, year, &result, NULL) == 0);
    return result;
}
int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, index = 0, dc = 0, coupon = 0, pricer = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, date(15, 6, 2025), &error) == 0);
    assert(itofin_zero_index_new(ctx, 0, settings, &index, &error) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc, &error) == 0);
    assert(itofin_inflation_index_add_fixing(ctx, index, 0, date(1, 4, 2025), 100.8, &error) == 0);
    ItofinCpiCouponConfig a;
    memset(&a, 0, sizeof a);
    a.index = index; a.base_cpi = NAN; a.nominal = 100000; a.fixed_rate = .02;
    a.payment = date(15, 7, 2025); a.start = date(15, 1, 2025); a.end = a.payment;
    a.lag_length = 3; a.lag_unit = 2; a.day_counter = dc;
    assert(itofin_cpi_coupon_from_base_date(ctx, a, date(15, 12, 2024), &coupon, &error) == 0);
    double value = -701;
    assert(itofin_cpi_coupon_value(ctx, coupon, 6, 0, &value, &error) != 0 && value == -701);
    assert(itofin_inflation_index_add_fixing(ctx, index, 0, date(1, 12, 2024), 100, &error) == 0);
    assert(itofin_cpi_coupon_value(ctx, coupon, 6, 0, &value, &error) == 0 && value == 100);
    assert(itofin_cpi_coupon_pricer_new(ctx, 0, &pricer, &error) == 0);
    assert(itofin_cpi_coupon_set_pricer(ctx, coupon, pricer, &error) == 0);
    assert(itofin_cpi_coupon_value(ctx, coupon, 1, 0, &value, &error) == 0);
    assert(fabs(value - 999.7150684931507) < 1e-10);
    uint64_t sentinel = 719;
    assert(itofin_cpi_coupon_from_base_date(ctx, a, 0, &sentinel, &error) != 0 && sentinel == 719);
    assert(itofin_cpi_coupon_from_base_date(ctx, a, date(15, 12, 2024), NULL, &error) != 0);
    assert(itofin_cpi_coupon_new(ctx, a, &sentinel, &error) != 0 && sentinel == 719);
    a.base_cpi = DBL_MAX;
    uint64_t old = 0;
    assert(itofin_cpi_coupon_new(ctx, a, &old, &error) == 0);
    assert(itofin_cpi_coupon_value(ctx, old, 6, 0, &value, &error) == 0 && value == DBL_MAX);
    uint64_t calendar = 0, schedule = 0, builder = 0, leg = 0, principal = 0;
    assert(itofin_calendar_new(ctx, 1, &calendar, &error) == 0);
    ItofinScheduleConfig sc = {a.start, a.end, 1, calendar, 2, 1, -1};
    assert(itofin_schedule_new(ctx, sc, &schedule, &error) == 0);
    const double notionals[] = {100000}, rates[] = {.02};
    ItofinCpiLegConfig l;
    memset(&l, 0, sizeof l);
    l.schedule = schedule; l.index = index; l.base_cpi = NAN; l.lag_length = 3; l.lag_unit = 2;
    l.day_counter = dc; l.payment_calendar = calendar; l.payment_convention = 2;
    l.notionals = notionals; l.notionals_len = 1; l.fixed_rates = rates; l.fixed_rates_len = 1;
    l.growth_only = 1;
    assert(itofin_cpi_leg_from_base_date(ctx, l, 0, &builder, &error) == 0);
    assert(itofin_cpi_leg_build(ctx, builder, &leg, &error) == 0);
    assert(itofin_leg_item(ctx, leg, 1, &principal, &error) == 0);
    value = -701;
    assert(itofin_cashflow_amount(ctx, principal, &value, &error) != 0 && value == -701);
    assert(itofin_inflation_index_add_fixing(ctx, index, 0, date(1, 10, 2024), 99.5, &error) == 0);
    assert(itofin_cashflow_amount(ctx, principal, &value, &error) == 0);
    assert(fabs(value - 100000 * (100.8 / 99.5 - 1)) < 1e-8);
    assert(itofin_cpi_leg_from_base_date(ctx, l, -1, &sentinel, &error) != 0 && sentinel == 719);
    assert(itofin_cpi_leg_from_base_date(ctx, l, 0, NULL, &error) != 0);
    assert(itofin_handle_release(ctx, index, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    assert(itofin_handle_release(ctx, pricer, &error) == 0);
    assert(itofin_handle_release(ctx, leg, &error) == 0);
    assert(itofin_handle_release(ctx, builder, &error) == 0);
    assert(itofin_cashflow_amount(ctx, principal, &value, &error) == 0 && isfinite(value));
    assert(itofin_cpi_coupon_value(ctx, coupon, 1, 0, &value, &error) == 0 && isfinite(value));
    assert(itofin_handle_release(ctx, coupon, &error) == 0);
    value = -701;
    assert(itofin_cpi_coupon_value(ctx, coupon, 6, 0, &value, &error) == ITOFIN_INVALID_HANDLE && value == -701);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
