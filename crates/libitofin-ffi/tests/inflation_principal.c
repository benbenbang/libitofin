#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <string.h>

static int32_t serial(int32_t day, int32_t month, int32_t year) {
    struct ItofinError error;
    int32_t out = 0;
    assert(itofin_date_new(day, month, year, &out, &error) == 0);
    return out;
}

int main(void) {
    struct ItofinContext *ctx = NULL;
    struct ItofinError error;
    uint64_t settings = 0, index = 0, explicit_flow = 0, live_flow = 0;
    uint64_t generic = 0, leg = 0, item = 0;
    double value = -73.0;
    int32_t payment = serial(15, 7, 2026);
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, serial(18, 8, 2025), &error) == 0);
    assert(itofin_zero_index_new(ctx, 0, settings, &index, &error) == 0);
    assert(itofin_inflation_index_add_fixing(ctx, index, 0, serial(1, 1, 2024), 100.0, &error) == 0);
    assert(itofin_inflation_index_add_fixing(ctx, index, 0, serial(1, 7, 2024), 110.0, &error) == 0);
    struct ItofinCpiCashFlowConfig a;
    memset(&a, 0, sizeof(a));
    a.index = index;
    a.nominal = 1000.0;
    a.base_cpi = 100.0;
    a.observation_date = serial(15, 7, 2024);
    a.payment_date = payment;
    a.lag_unit = 2;
    a.growth_only = 1;
    assert(itofin_cpi_cashflow_new(ctx, a, &explicit_flow, &error) == 0);
    assert(itofin_inflation_cashflow_value(ctx, explicit_flow, 0, &value, &error) == 0);
    assert(fabs(value - 100.0) < 1e-10);
    struct ItofinZeroInflationCashFlowConfig b;
    memset(&b, 0, sizeof(b));
    b.index = index;
    b.nominal = 1000.0;
    b.start_date = serial(15, 1, 2024);
    b.end_date = a.observation_date;
    b.payment_date = payment;
    b.lag_unit = 2;
    b.growth_only = 1;
    assert(itofin_zero_inflation_cashflow_new(ctx, b, &live_flow, &error) == 0);
    assert(itofin_inflation_cashflow_convert(ctx, live_flow, 0, &generic, &error) == 0);
    assert(itofin_inflation_cashflow_convert(ctx, live_flow, 1, &leg, &error) == 0);
    assert(itofin_handle_release(ctx, live_flow, &error) == 0);
    assert(itofin_handle_release(ctx, index, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    assert(itofin_cashflow_amount(ctx, generic, &value, &error) == 0);
    assert(fabs(value - 100.0) < 1e-10);
    assert(itofin_leg_item(ctx, leg, 0, &item, &error) == 0);
    assert(itofin_cashflow_amount(ctx, item, &value, &error) == 0);
    assert(fabs(value - 100.0) < 1e-10);
    int32_t out_date = 0;
    assert(itofin_inflation_cashflow_date(ctx, explicit_flow, &out_date, &error) == 0);
    assert(out_date == payment);
    value = -73.0;
    assert(itofin_inflation_cashflow_value(ctx, explicit_flow, 99, &value, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(value == -73.0);
    uint64_t unchanged = 81;
    assert(itofin_inflation_cashflow_convert(ctx, explicit_flow, 2, &unchanged, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(unchanged == 81);
    assert(itofin_inflation_cashflow_value(ctx, explicit_flow, 0, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
