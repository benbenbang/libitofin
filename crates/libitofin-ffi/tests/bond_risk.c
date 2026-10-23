#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, calendar = 0, dc = 0, bond = 0;
    int32_t settlement = 0, maturity = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_calendar_new(ctx, 0, &calendar, &error) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc, &error) == 0);
    assert(itofin_date_new(2, 1, 2025, &settlement, &error) == 0);
    assert(itofin_date_new(2, 1, 2028, &maturity, &error) == 0);
    ItofinBondConfig bc = {100000, 375.0, 100.0, settings, 0, 2, calendar};
    assert(itofin_zero_coupon_bond_new(ctx, bc, maturity, &bond, &error) == 0);
    ItofinBondYieldAnalyticsConfig config = {dc, 1, 1, settlement};
    double t = (maturity - settlement) / 365.0;
    double expected[6] = {t, t, t, t * (t + 0.5), -375 * t * 0.0001 + t * (t + 0.5) / 100 * 375 * 0.0001 * 0.0001 / 2, 0.01 / (-375 * t)};
    double value = -123;
    assert(itofin_handle_release(ctx, calendar, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    for (int32_t field = 0; field < 6; ++field) {
        assert(itofin_bond_risk(ctx, bond, field, 0, config, &value, &error) == 0);
        assert(fabs(value - expected[field]) < 3e-12);
    }
    for (int32_t field = -1; field <= 6; field += 7) {
        value = -123;
        assert(itofin_bond_risk(ctx, bond, field, 0, config, &value, &error) == ITOFIN_INVALID_ARGUMENT);
        assert(value == -123);
    }
    assert(itofin_bond_risk(ctx, bond, 0, 0, config, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    config.compounding = 2;
    assert(itofin_bond_risk(ctx, bond, 1, 0, config, &value, &error) != 0);
    assert(value == -123);
    assert(itofin_bond_risk(ctx, bond, 2, 0, config, &value, &error) == 0);
    assert(fabs(value - t) < 3e-12);
    value = -123;
    assert(itofin_bond_risk(ctx, bond, 2, NAN, config, &value, &error) != 0);
    assert(value == -123);
    config.settlement = maturity;
    assert(itofin_bond_risk(ctx, bond, 2, 0, config, &value, &error) != 0);
    assert(value == -123);
    config.settlement = settlement;
    uint64_t discount = 0;
    assert(itofin_flat_forward_new(ctx, settlement, 0.032, dc, &discount, &error) == 0);
    assert(itofin_bond_bps_from_curve(ctx, bond, discount, settlement, &value, &error) == 0);
    assert(value == 0.0);
    assert(itofin_bond_bps_from_yield(ctx, bond, 0.052, config, &value, &error) == 0);
    assert(value == 0.0);
    assert(itofin_bond_atm_rate_from_curve(ctx, bond, discount, settlement, &value, &error) == 0);
    assert(value == 0.0);
    value = -123;
    assert(itofin_bond_bps_from_yield(ctx, bond, NAN, config, &value, &error) != 0);
    assert(value == -123);
    assert(itofin_bond_bps_from_curve(ctx, bond, discount, maturity, &value, &error) != 0);
    assert(value == -123);
    assert(itofin_bond_atm_rate_from_curve(ctx, bond, discount, maturity, &value, &error) != 0);
    assert(value == -123);
    assert(itofin_handle_release(ctx, discount, &error) == 0);
    assert(itofin_bond_bps_from_curve(ctx, bond, discount, settlement, &value, &error) == ITOFIN_INVALID_HANDLE);
    assert(value == -123);
    assert(itofin_handle_release(ctx, dc, &error) == 0);
    assert(itofin_bond_risk(ctx, bond, 2, 0, config, &value, &error) == ITOFIN_INVALID_HANDLE);
    assert(value == -123);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
