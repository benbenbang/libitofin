#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdint.h>
#include <stdio.h>
#include <string.h>

int main(void) {
    FILE *file = fopen("sdk/go/testdata/gap_fd_reference.csv", "r");
    assert(file != NULL);
    char line[2048];
    assert(fgets(line, sizeof(line), file) != NULL);
    int rows = 0, ordinary = 0;
    while (fgets(line, sizeof(line), file) != NULL) {
        char id[128], family[32], kind[16];
        double spot, r, q, sigma, days, trigger, second, tg, xg, damping;
        double npv, delta, gamma, theta, closed_form;
        assert(sscanf(line, "%127[^,],%31[^,],%15[^,],%lf,%lf,%lf,%lf,%lf,%lf,%lf,%lf,%lf,%lf,%lf,%lf,%lf,%lf,%lf",
                      id, family, kind, &spot, &r, &q, &sigma, &days, &trigger, &second,
                      &tg, &xg, &damping, &npv, &delta, &gamma, &theta, &closed_form) == 18);
        ++rows;
        assert(strcmp(family, "gap") == 0);
        if (spot <= 0 || sigma <= 0 || days <= 0 || trigger <= 0) continue;
        ++ordinary;
        ItofinContext *ctx = NULL;
        ItofinError error = {0, {0}};
        uint64_t settings = 0, dc = 0, process = 0, engine = 0, option = 0;
        int32_t today = 0;
        assert(itofin_context_new(&ctx, &error) == 0);
        assert(itofin_date_new(15, 6, 2026, &today, &error) == 0);
        assert(itofin_settings_new(ctx, &settings, &error) == 0);
        assert(itofin_settings_set_evaluation_date(ctx, settings, today, &error) == 0);
        assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
        assert(itofin_black_scholes_new(ctx, spot, r, q, sigma, today, dc, &process, &error) == 0);
        ItofinFdConfig cfg = {7, (uint64_t)tg, (uint64_t)xg, (uint64_t)damping, 0};
        assert(itofin_fd_black_scholes_engine_new(ctx, process, cfg, &engine, &error) == 0);
        const int32_t side = strcmp(kind, "call") == 0 ? 0 : 1;
        assert(itofin_option_gap_new(ctx, side, trigger, second, today + (int32_t)days, settings, &option, &error) == 0);
        assert(itofin_option_set_engine(ctx, option, engine, 2, 0, &error) == 0);
        const double expected[] = {npv, delta, gamma, theta};
        for (int32_t field = 0; field < 4; ++field) {
            double actual = 0;
            assert(itofin_option_value(ctx, option, field, &actual, &error) == 0);
            const double tolerance = field == 3 ? 1e-7 : 1e-8;
            if (!isfinite(actual) || !isfinite(expected[field]) || fabs(actual - expected[field]) > tolerance) {
                fprintf(stderr, "%s grid%.0f damping%.0f field%d: %.17g vs %.17g\n", id, tg, damping, (int)field, actual, expected[field]);
                assert(0);
            }
        }
        assert(itofin_context_free(ctx, &error) == 0);
    }
    assert(fclose(file) == 0);
    assert(rows == 240 && ordinary == 192);
    return 0;
}
