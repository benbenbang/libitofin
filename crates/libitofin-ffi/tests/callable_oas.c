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
    ItofinCallableOASConfig oas = {dc, 2, 1, 0, 0};
    ItofinAnalyticsSolverConfig solver = {0, NAN, 0, NAN};
    double price = 991.0, root = 991.0;
    assert(itofin_callable_bond_clean_price_oas(ctx, bond, 0.015, engine, oas, &price, &error) == 0);
    assert(itofin_callable_bond_oas(ctx, bond, price, engine, oas, solver, &root, &error) == 0);
    near(root, 0.015, 2e-10);
    solver.flags = 2;
    solver.max_evaluations = 1;
    root = 991.0;
    assert(itofin_callable_bond_oas(ctx, bond, price, engine, oas, solver, &root, &error) != 0);
    assert(root == 991.0);
    oas.settlement_flags = 1;
    assert(itofin_callable_bond_clean_price_oas(ctx, bond, 0.015, engine, oas, &root, &error) != 0);
    assert(root == 991.0);
    oas.settlement_flags = 0;
    assert(itofin_callable_bond_clean_price_oas(ctx, bond, NAN, engine, oas, &root, &error) != 0);
    assert(itofin_callable_bond_clean_price_oas(ctx, bond, 0.015, engine, oas, NULL, &error) != 0);
    assert(root == 991.0);
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
    assert(itofin_callable_bond_clean_price_oas(ctx, bond, 0.015, other_engine, oas, &root, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_callable_bond_clean_price_oas(ctx, plain, 0.015, engine, oas, &root, &error) == ITOFIN_INVALID_ARGUMENT);
    assert(itofin_callable_bond_clean_price_oas(ctx, bond, 0.015, model, oas, &root, &error) == ITOFIN_INVALID_HANDLE);
    assert(root == 991.0);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
