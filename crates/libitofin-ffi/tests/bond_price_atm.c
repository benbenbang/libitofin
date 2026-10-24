#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int32_t iso_date(const char *text, ItofinError *error) {
    int year = 0, month = 0, day = 0;
    int32_t serial = 0;
    assert(sscanf(text, "%d-%d-%d", &year, &month, &day) == 3);
    assert(itofin_date_new(day, month, year, &serial, error) == 0);
    return serial;
}

int main(void) {
    FILE *input = fopen("sdk/go/testdata/bond_price_atm/bond_atm_from_price_reference.csv", "r");
    assert(input != NULL);
    char line[2048];
    assert(fgets(line, sizeof(line), input) != NULL);
    int checked = 0, skipped_amortizing = 0;
    while (fgets(line, sizeof(line), input) != NULL) {
        char *fields[15];
        int count = 0;
        for (char *part = strtok(line, ",\r\n"); part != NULL; part = strtok(NULL, ",\r\n")) {
            assert(count < 15);
            fields[count++] = part;
        }
        assert(count == 15);
        if (strcmp(fields[1], "amortizing") == 0) {
            ++skipped_amortizing;
            continue;
        }
        ItofinContext *ctx = NULL;
        ItofinError error = {0, {0}};
        uint64_t settings = 0, calendar = 0, dc = 0, curve_dc = 0, curve = 0, bond = 0;
        assert(itofin_context_new(&ctx, &error) == 0);
        assert(itofin_settings_new(ctx, &settings, &error) == 0);
        assert(itofin_calendar_new(ctx, 1, &calendar, &error) == 0);
        assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
        assert(itofin_day_counter_new(ctx, 1, &curve_dc, &error) == 0);
        int32_t start = iso_date(fields[3], &error), end = iso_date(fields[4], &error);
        int32_t reference = iso_date(fields[5], &error), settlement = iso_date(fields[6], &error);
        double face = strtod(fields[2], NULL), discount_rate = strtod(fields[8], NULL);
        assert(itofin_flat_forward_new(ctx, reference, discount_rate, curve_dc, &curve, &error) == 0);
        int zero = strcmp(fields[1], "zero") == 0;
        ItofinBondConfig base = {5, face, zero ? 100.0 : 102.0, settings, start, 2, calendar};
        if (zero) {
            assert(itofin_zero_coupon_bond_new(ctx, base, end, &bond, &error) == 0);
        } else {
            uint64_t schedule = 0;
            ItofinScheduleConfig sc = {start, end, 1, calendar, 2, 0, -1};
            assert(itofin_schedule_new(ctx, sc, &schedule, &error) == 0);
            int32_t ex_days = (int32_t)strtol(fields[7], NULL, 10);
            ItofinFixedBondConfig config = {base, schedule, dc, 0, calendar, ex_days, 0, 2, (uint8_t)(ex_days != 0), 0};
            double rates[] = {0.047, 0.058};
            assert(itofin_fixed_rate_bond_new(ctx, config, rates, 2, &bond, &error) == 0);
            assert(itofin_handle_release(ctx, schedule, &error) == 0);
        }
        double value = -123.0, expected = strtod(fields[13], NULL), independent = strtod(fields[14], NULL);
        int32_t kind = strcmp(fields[9], "clean") == 0 ? 0 : 1;
        double price = strtod(fields[10], NULL);
        assert(itofin_bond_real(ctx, bond, 5, settlement, &value, &error) == 0);
        assert(value == strtod(fields[11], NULL));
        assert(itofin_bond_real(ctx, bond, 4, settlement, &value, &error) == 0);
        assert(isfinite(value) && fabs(value - strtod(fields[12], NULL)) <= 2e-12);
        assert(itofin_bond_atm_rate_from_price(ctx, bond, curve, price, kind, settlement, &value, &error) == 0);
        assert(isfinite(value) && isfinite(expected) && isfinite(independent));
        assert(fabs(value - expected) <= 2e-13 && fabs(value - independent) <= 2e-13);
        assert(itofin_handle_release(ctx, settings, &error) == 0);
        assert(itofin_handle_release(ctx, calendar, &error) == 0);
        assert(itofin_handle_release(ctx, dc, &error) == 0);
        assert(itofin_handle_release(ctx, curve_dc, &error) == 0);
        assert(itofin_bond_atm_rate_from_price(ctx, bond, curve, price, kind, settlement, &value, &error) == 0);
        assert(isfinite(value) && fabs(value - expected) <= 2e-13);
        value = -123.0;
        assert(itofin_bond_atm_rate_from_price(ctx, bond, curve, NAN, kind, settlement, &value, &error) != 0);
        assert(value == -123.0);
        assert(itofin_bond_atm_rate_from_price(ctx, bond, curve, price, 2, settlement, &value, &error) == ITOFIN_INVALID_ARGUMENT);
        assert(value == -123.0);
        assert(itofin_bond_atm_rate_from_price(ctx, bond, curve, price, kind, settlement, NULL, &error) == ITOFIN_INVALID_ARGUMENT);
        assert(itofin_handle_release(ctx, curve, &error) == 0);
        assert(itofin_bond_atm_rate_from_price(ctx, bond, curve, price, kind, settlement, &value, &error) == ITOFIN_INVALID_HANDLE);
        assert(value == -123.0);
        assert(itofin_context_free(ctx, &error) == 0);
        ++checked;
    }
    assert(fclose(input) == 0);
    assert(checked == 182 && skipped_amortizing == 60);
    return 0;
}
