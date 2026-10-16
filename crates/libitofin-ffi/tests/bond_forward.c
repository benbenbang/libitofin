#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, calendar = 0, dc = 0, curve = 0, bond = 0, forward = 0;
    int32_t today = 0, delivery = 0, maturity = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_calendar_new(ctx, 0, &calendar, &error) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc, &error) == 0);
    assert(itofin_date_new(2, 1, 2025, &today, &error) == 0);
    assert(itofin_date_new(2, 7, 2025, &delivery, &error) == 0);
    assert(itofin_date_new(2, 7, 2026, &maturity, &error) == 0);
    assert(itofin_flat_forward_new(ctx, today, 0.03, dc, &curve, &error) == 0);
    ItofinBondConfig bc = {2, 100.0, 100.0, settings, 0, 2, calendar};
    assert(itofin_zero_coupon_bond_new(ctx, bc, maturity, &bond, &error) == 0);
    ItofinBondForwardConfig config = {bond, today, delivery, 0, 105.0, 0, dc, calendar, 2, curve, 0};
    assert(itofin_bond_forward_new(ctx, config, &forward, &error) == 0);
    double value = -123.0;
    assert(itofin_bond_forward_value(ctx, forward, 3, &value, &error) == 0);
    assert(value == 105.0);
    assert(itofin_bond_forward_value(ctx, forward, 0, &value, &error) != 0);
    assert(value == 105.0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, today, &error) == 0);
    uint64_t engine = 0;
    assert(itofin_discounting_bond_engine_new(ctx, curve, settings, -1, &engine, &error) == 0);
    assert(itofin_bond_set_engine(ctx, bond, engine, &error) == 0);
    assert(itofin_handle_release(ctx, bond, &error) == 0);
    assert(itofin_handle_release(ctx, engine, &error) == 0);
    assert(itofin_handle_release(ctx, curve, &error) == 0);
    assert(itofin_handle_release(ctx, calendar, &error) == 0);
    assert(itofin_handle_release(ctx, dc, &error) == 0);
    assert(itofin_bond_forward_value(ctx, forward, 2, &value, &error) == 0);
    assert(isfinite(value) && fabs(value - 105.0) > 1.0);
    int32_t date = 0;
    assert(itofin_bond_forward_integer(ctx, forward, 0, &date, &error) == 0);
    assert(date == today);
    value = -123.0;
    assert(itofin_bond_forward_value(ctx, forward, 99, &value, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(value == -123.0);
    assert(itofin_bond_forward_value(ctx, forward, 0, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_bond_forward_control(ctx, forward, 1, &error) == 0);
    assert(itofin_bond_forward_control(ctx, forward, 0, &error) == 0);
    assert(itofin_bond_forward_control(ctx, forward, 2, &error) == 0);
    assert(itofin_handle_release(ctx, forward, &error) == 0);
    assert(itofin_bond_forward_value(ctx, forward, 0, &value, &error) == ITOFIN_INVALID_HANDLE);
    assert(value == -123.0);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
