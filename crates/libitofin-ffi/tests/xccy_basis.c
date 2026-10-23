#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <string.h>

int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error;
    uint64_t settings, dc, calendar, currencies[2], indexes[2], curves[4], schedule, swap;
    const double rates[4] = {0.031, 0.043, 0.027, 0.036};
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, 46027, &error) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
    assert(itofin_calendar_new(ctx, 1, &calendar, &error) == 0);
    for (int i = 0; i < 4; ++i) {
        assert(itofin_flat_forward_new(ctx, 46027, rates[i], dc, &curves[i], &error) == 0);
    }
    for (int i = 0; i < 2; ++i) {
        assert(itofin_currency_new(ctx, i, &currencies[i], &error) == 0);
        ItofinIborConfig a;
        memset(&a, 0, sizeof a);
        a.tenor_length = 3; a.tenor_unit = 2; a.currency = currencies[i];
        a.fixing_calendar = calendar; a.convention = 2; a.day_counter = dc;
        a.forwarding = curves[i]; a.settings = settings;
        const uint8_t *name = (const uint8_t *)(i == 0 ? "CXccyBase" : "CXccyQuote");
        assert(itofin_ibor_new(ctx, name, strlen((const char *)name), &a, false, &indexes[i], &error) == 0);
    }
    ItofinScheduleConfig sch;
    memset(&sch, 0, sizeof sch);
    sch.start = 46030; sch.end = 46395; sch.frequency = 2;
    sch.calendar = calendar; sch.convention = 2; sch.termination_convention = -1;
    assert(itofin_schedule_new(ctx, sch, &schedule, &error) == 0);
    ItofinXccyBasisSwapConfig a;
    memset(&a, 0, sizeof a);
    a.base_schedule = schedule; a.quote_schedule = schedule;
    a.base_index = indexes[0]; a.quote_index = indexes[1];
    a.base_curve = curves[2]; a.quote_curve = curves[3];
    a.base_day_counter = dc; a.quote_day_counter = dc;
    a.base_currency = currencies[0]; a.quote_currency = currencies[1];
    a.base_notional = 1000000; a.quote_notional = 1270000;
    a.quote_spread = 0.0017; a.spot = 1.27; a.settings = settings;
    assert(itofin_xccy_basis_swap_new(ctx, &a, &swap, &error) == 0);
    const double expected[] = {4015.3856141086876, 11035.711320720658, 125.82851287923141, 4674.150858899704, 5936.1715908026254, -0.0030176680825117009, 1.27};
    double value = 987;
    for (int query = 0; query < 7; ++query) {
        assert(itofin_xccy_basis_swap_value(ctx, swap, query, &value, &error) == 0);
        assert(fabs(value - expected[query]) <= (query == 5 ? 2e-13 : 2e-9));
    }
    a.include_settlement_flows = 1;
    uint64_t sentinel = 987;
    assert(itofin_xccy_basis_swap_new(ctx, &a, &sentinel, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(sentinel == 987);
    assert(itofin_xccy_basis_swap_value(ctx, swap, 7, &value, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(value == 1.27);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
