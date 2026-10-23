#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <string.h>

int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError e;
    uint64_t settings, dc, dc365, calendar, currency, forward, discount, index;
    uint64_t fixed_schedule, floating_schedule, bond, swap;
    assert(itofin_context_new(&ctx, &e) == 0);
    assert(itofin_settings_new(ctx, &settings, &e) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, 46024, &e) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, &e) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc365, &e) == 0);
    assert(itofin_calendar_new(ctx, 1, &calendar, &e) == 0);
    assert(itofin_currency_new(ctx, 0, &currency, &e) == 0);
    assert(itofin_flat_forward_new(ctx, 46024, .029, dc, &forward, &e) == 0);
    assert(itofin_flat_forward_new(ctx, 46024, .036, dc365, &discount, &e) == 0);
    ItofinIborConfig ibor;
    memset(&ibor, 0, sizeof ibor);
    ibor.tenor_length = 6;
    ibor.tenor_unit = 2;
    ibor.currency = currency;
    ibor.fixing_calendar = calendar;
    ibor.convention = 2;
    ibor.day_counter = dc;
    ibor.forwarding = forward;
    ibor.settings = settings;
    const uint8_t *name = (const uint8_t *)"AssetBinding";
    assert(itofin_ibor_new(ctx, name, strlen((const char *)name), &ibor, false, &index, &e) == 0);
    ItofinScheduleConfig schedule;
    memset(&schedule, 0, sizeof schedule);
    schedule.start = 46027;
    schedule.end = 47123;
    schedule.frequency = 0;
    schedule.calendar = calendar;
    schedule.convention = 2;
    schedule.termination_convention = -1;
    assert(itofin_schedule_new(ctx, schedule, &fixed_schedule, &e) == 0);
    schedule.frequency = 1;
    assert(itofin_schedule_new(ctx, schedule, &floating_schedule, &e) == 0);
    ItofinFixedBondConfig fixed;
    memset(&fixed, 0, sizeof fixed);
    fixed.bond.face_amount = 100;
    fixed.bond.redemption = 100;
    fixed.bond.settings = settings;
    fixed.bond.payment_convention = 2;
    fixed.bond.calendar = calendar;
    fixed.schedule = fixed_schedule;
    fixed.day_counter = dc;
    double coupon = .043;
    assert(itofin_fixed_rate_bond_new(ctx, fixed, &coupon, 1, &bond, &e) == 0);
    ItofinAssetSwapConfig config;
    memset(&config, 0, sizeof config);
    config.bond = bond;
    config.index = index;
    config.floating_schedule = floating_schedule;
    config.clean_price = 97.25;
    config.spread = .0017;
    config.pay_bond_coupon = 1;
    assert(itofin_asset_swap_new(ctx, &config, &swap, &e) == 0);
    assert(itofin_asset_swap_set_engine(ctx, swap, discount, settings, &e) == 0);
    const double expected[6] = {-6.093607531139909, -101.9099097126986, 95.8163021815587, .02858905060041601, .023014480205408566, 103.34541083795396};
    for (int i = 0; i < 6; ++i) {
        double value = 999;
        assert(itofin_asset_swap_value(ctx, swap, i, &value, &e) == 0);
        assert(fabs(value - expected[i]) <= 2e-10);
    }
    double sentinel = 12345;
    assert(itofin_asset_swap_value(ctx, swap, 99, &sentinel, &e) == ITOFIN_INVALID_ARGUMENT);
    assert(sentinel == 12345);
    assert(itofin_asset_swap_action(ctx, swap, 99, &e) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_asset_swap_new(ctx, NULL, &swap, &e) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_asset_swap_value(ctx, swap, 0, NULL, &e) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_context_free(ctx, &e) == 0);
    return 0;
}
