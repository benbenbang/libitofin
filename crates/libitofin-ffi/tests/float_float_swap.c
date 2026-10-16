#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <string.h>

int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error;
    uint64_t settings, dc, dc365, currency, calendar, weekends;
    uint64_t curves[3], indexes[2], schedules[2], swap;
    const double rates[3] = {0.025, 0.034, 0.02};
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, 46024, &error) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc365, &error) == 0);
    assert(itofin_currency_new(ctx, 0, &currency, &error) == 0);
    assert(itofin_calendar_new(ctx, 1, &calendar, &error) == 0);
    assert(itofin_calendar_new(ctx, 2, &weekends, &error) == 0);
    for (int i = 0; i < 3; ++i) {
        assert(itofin_flat_forward_new(ctx, 46024, rates[i], dc, &curves[i], &error) == 0);
    }
    for (int i = 0; i < 2; ++i) {
        ItofinIborConfig index;
        ItofinScheduleConfig schedule;
        memset(&index, 0, sizeof index);
        index.tenor_length = i == 0 ? 3 : 6;
        index.tenor_unit = 2;
        index.currency = currency;
        index.fixing_calendar = calendar;
        index.convention = 2;
        index.day_counter = dc;
        index.forwarding = curves[i];
        index.settings = settings;
        const uint8_t *name = (const uint8_t *)(i == 0 ? "CFloatPay" : "CFloatReceive");
        assert(itofin_ibor_new(ctx, name, strlen((const char *)name), &index, false, &indexes[i], &error) == 0);
        memset(&schedule, 0, sizeof schedule);
        schedule.start = 46039;
        schedule.end = 46404;
        schedule.frequency = i == 0 ? 2 : 1;
        schedule.calendar = weekends;
        schedule.convention = 2;
        schedule.termination_convention = -1;
        assert(itofin_schedule_new(ctx, schedule, &schedules[i], &error) == 0);
    }
    ItofinFloatFloatSwapConfig config;
    memset(&config, 0, sizeof config);
    config.nominal = 1000000;
    config.pay_schedule = schedules[0];
    config.receive_schedule = schedules[1];
    config.pay_index = indexes[0];
    config.receive_index = indexes[1];
    config.pay_spread = 0.001;
    config.receive_spread = -0.002;
    config.pay_day_counter = dc;
    config.receive_day_counter = dc365;
    config.settings = settings;
    assert(itofin_float_float_swap_new(ctx, &config, &swap, &error) == 0);
    assert(itofin_float_float_swap_set_engine(ctx, swap, curves[2], settings, &error) == 0);
    double value = 999;
    assert(itofin_float_float_swap_value(ctx, swap, 0, &value, &error) == 0);
    assert(fabs(value - 5693.9219440826419) <= 2e-9);
    assert(itofin_float_float_swap_action(ctx, swap, 1, &error) == 0);
    assert(itofin_float_float_swap_value(ctx, swap, 99, &value, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(fabs(value - 5693.9219440826419) <= 2e-9);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
