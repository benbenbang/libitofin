#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stddef.h>

static void near(double got, double want, double tolerance) {
    assert(isfinite(got));
    assert(fabs(got - want) <= tolerance);
}
int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, cal = 0, dc = 0, schedule = 0, curve = 0, model = 0, engine = 0, bond = 0;
    int32_t start = 0, maturity = 0, exercise = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_date_new(7, 7, 2026, &start, &error) == 0);
    assert(itofin_date_new(7, 7, 2031, &maturity, &error) == 0);
    assert(itofin_date_new(7, 7, 2028, &exercise, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, settings, start, &error) == 0);
    assert(itofin_calendar_new(ctx, 1, &cal, &error) == 0);
    assert(itofin_day_counter_new(ctx, 1, &dc, &error) == 0);
    ItofinScheduleConfig sc = {start, maturity, 1, cal, 2, 0, 2};
    assert(itofin_schedule_new(ctx, sc, &schedule, &error) == 0);
    assert(itofin_flat_forward_new(ctx, start, 0.03, dc, &curve, &error) == 0);
    assert(itofin_hullwhite_new(ctx, curve, 0.1, 0.01, &model, &error) == 0);
    uint64_t untouched = 991;
    assert(itofin_tree_callable_bond_engine_new(ctx, model, 0, settings, &untouched, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(untouched == 991);
    assert(itofin_tree_callable_bond_engine_new(ctx, model, 100, settings, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_tree_callable_bond_engine_new(ctx, model, 100, settings, &engine, &error) == 0);
    ItofinBondConfig common = {2, 1000.0, 100.0, settings, start, 2, 0};
    ItofinFixedBondConfig config = {common, schedule, dc, 0, 0, 0, 0, 2, 0, 0};
    ItofinCallability call = {100.0, 0, 0, exercise};
    const double coupons[1] = {0.05};
    assert(itofin_callable_fixed_rate_bond_new(ctx, config, coupons, 1, &call, 1, &bond, &error) == 0);
    assert(itofin_callable_bond_set_tree_engine(ctx, bond, engine, &error) == 0);
    double value = 0.0;
    assert(itofin_bond_real(ctx, bond, 0, 0, &value, &error) == 0);
    near(value, 1037.6283096323507, 1e-10);
    assert(itofin_bond_real(ctx, bond, 1, 0, &value, &error) == 0);
    near(value, 1037.7988926896915, 1e-10);
    assert(itofin_bond_real(ctx, bond, 2, 0, &value, &error) == 0);
    near(value, 103.75249200869519, 1e-11);
    assert(itofin_bond_real(ctx, bond, 3, 0, &value, &error) == 0);
    near(value, 103.77988926896914, 1e-11);
    uint64_t other_settings = 0, other_engine = 0, plain = 0, discount_engine = 0;
    assert(itofin_settings_new(ctx, &other_settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, other_settings, start, &error) == 0);
    assert(itofin_tree_callable_bond_engine_new(ctx, model, 100, other_settings, &other_engine, &error) == 0);
    assert(itofin_callable_bond_set_tree_engine(ctx, bond, other_engine, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_callable_bond_set_tree_engine(ctx, bond, model, &error) == ITOFIN_INVALID_HANDLE);
    assert(itofin_discounting_bond_engine_new(ctx, curve, settings, -1, &discount_engine, &error) == 0);
    assert(itofin_bond_set_engine(ctx, bond, discount_engine, &error) == ITOFIN_INVALID_ARGUMENT);
    common.calendar = cal;
    assert(itofin_zero_coupon_bond_new(ctx, common, maturity, &plain, &error) == 0);
    assert(itofin_callable_bond_set_tree_engine(ctx, plain, engine, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_handle_release(ctx, engine, &error) == 0);
    assert(itofin_handle_release(ctx, model, &error) == 0);
    assert(itofin_handle_release(ctx, curve, &error) == 0);
    assert(itofin_bond_action(ctx, bond, 0, &error) == 0);
    assert(itofin_bond_real(ctx, bond, 2, 0, &value, &error) == 0);
    near(value, 103.75249200869519, 1e-11);
    uint64_t zero = 0, zero_settings = 0, target = 0, zero_curve = 0, zero_model = 0, zero_engine = 0;
    assert(itofin_date_new(12, 10, 2026, &start, &error) == 0);
    assert(itofin_date_new(12, 10, 2031, &maturity, &error) == 0);
    assert(itofin_date_new(17, 6, 2028, &exercise, &error) == 0);
    assert(itofin_settings_new(ctx, &zero_settings, &error) == 0);
    assert(itofin_settings_set_evaluation_date(ctx, zero_settings, start, &error) == 0);
    assert(itofin_calendar_new(ctx, 0, &target, &error) == 0);
    assert(itofin_flat_forward_new(ctx, start, -0.012, dc, &zero_curve, &error) == 0);
    assert(itofin_hullwhite_new(ctx, zero_curve, 0.073, 0.014, &zero_model, &error) == 0);
    assert(itofin_tree_callable_bond_engine_new(ctx, zero_model, 120, zero_settings, &zero_engine, &error) == 0);
    common.face_amount = 1234567.89;
    common.redemption = 107.5;
    common.settings = zero_settings;
    common.calendar = target;
    common.payment_convention = 1;
    assert(itofin_date_new(9, 10, 2026, &common.issue_date, &error) == 0);
    call.price = 98.75;
    call.date = exercise;
    assert(itofin_callable_zero_coupon_bond_new(ctx, common, maturity, dc, &call, 1, &zero, &error) == 0);
    assert(itofin_callable_bond_set_tree_engine(ctx, zero, zero_engine, &error) == 0);
    assert(itofin_bond_real(ctx, zero, 0, 0, &value, &error) == 0);
    near(value, 1243845.739551052, 2e-8);
    assert(itofin_bond_real(ctx, zero, 1, 0, &value, &error) == 0);
    near(value, 1243763.9551227633, 2e-8);
    assert(itofin_bond_real(ctx, zero, 2, 0, &value, &error) == 0);
    near(value, 100.74488128172224, 2e-12);
    assert(itofin_bond_real(ctx, zero, 3, 0, &value, &error) == 0);
    near(value, 100.74488128172224, 2e-12);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
