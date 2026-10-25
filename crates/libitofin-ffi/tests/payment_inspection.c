#include "itofin.h"
#include <assert.h>
#include <math.h>
#include <stdio.h>
#include <stdlib.h>
#include <string.h>

static int32_t iso(const char *value, ItofinError *error) {
    int year = 0, month = 0, day = 0;
    int32_t date = 0;
    assert(sscanf(value, "%d-%d-%d", &year, &month, &day) == 3);
    assert(itofin_date_new(day, month, year, &date, error) == 0);
    return date;
}
static void check_payment(ItofinPaymentResult value, const char *date, const char *amount, ItofinError *error) {
    if (strcmp(date, "-") == 0) {
        assert(value.present == 0 && value.date == 0 && value.amount == 0.0);
    } else {
        double wanted = strtod(amount, NULL);
        assert(value.present == 1 && value.date == iso(date, error));
        assert(isfinite(value.amount) && fabs(value.amount - wanted) <= 3e-12);
    }
}
int main(void) {
    ItofinContext *ctx = NULL;
    ItofinError error = {0, {0}};
    uint64_t settings = 0, cal = 0, dc = 0, schedule = 0, bond = 0, leg = 0;
    assert(itofin_context_new(&ctx, &error) == 0);
    assert(itofin_settings_new(ctx, &settings, &error) == 0);
    assert(itofin_calendar_new(ctx, 1, &cal, &error) == 0);
    assert(itofin_day_counter_new(ctx, 0, &dc, &error) == 0);
    int32_t start = iso("2026-03-24", &error), end = iso("2028-03-24", &error), today = iso("2026-09-24", &error);
    assert(itofin_settings_set_evaluation_date(ctx, settings, today, &error) == 0);
    ItofinScheduleConfig dates = {start, end, 1, cal, 2, 1, -1};
    assert(itofin_schedule_new(ctx, dates, &schedule, &error) == 0);
    ItofinBondConfig base = {2, 375.0, 102.0, settings, start, 2, cal};
    ItofinFixedBondConfig config = {base, schedule, dc, 0, cal, 7, 0, 2, 1, 0};
    double rates[] = {.047, .058};
    assert(itofin_fixed_rate_bond_new(ctx, config, rates, 2, &bond, &error) == 0);
    assert(itofin_bond_handle(ctx, bond, 1, &leg, &error) == 0);
    int32_t span = 0;
    assert(itofin_leg_span_date(ctx, leg, 0, &span, &error) == 0 && span == start);
    assert(itofin_leg_span_date(ctx, leg, 1, &span, &error) == 0 && span == end);
    assert(itofin_bond_inspection(ctx, bond, 0, 0, &span, &error) == 0 && span == start);
    FILE *input = fopen("sdk/go/testdata/payment_inspection/bond-native.tsv", "r");
    assert(input != NULL);
    char line[2048];
    assert(fgets(line, sizeof(line), input) != NULL);
    int checked = 0;
    while (fgets(line, sizeof(line), input) != NULL) {
        char *fields[8];
        int count = 0;
        for (char *part = strtok(line, "\t\r\n"); part != NULL; part = strtok(NULL, "\t\r\n")) {
            assert(count < 8);
            fields[count++] = part;
        }
        assert(count == 8);
        int32_t settlement = iso(fields[0], &error);
        int32_t setting = strcmp(fields[1], "unset") == 0 ? -1 : (strcmp(fields[1], "true") == 0);
        int32_t include = strcmp(fields[2], "true") == 0;
        assert(itofin_settings_set_include_todays_cash_flows(ctx, settings, setting, &error) == 0);
        assert(itofin_bond_inspection(ctx, bond, 1, settlement, &span, &error) == 0);
        assert(span == (strcmp(fields[7], "true") == 0));
        for (int32_t direction = 0; direction <= 1; ++direction) {
            ItofinPaymentQuery query = {settings, settlement, include, direction, 1};
            ItofinPaymentResult value = {9, -123, -456.0};
            assert(itofin_leg_payment(ctx, leg, query, &value, &error) == 0);
            int column = direction == 0 ? 3 : 5;
            check_payment(value, fields[column], fields[column + 1], &error);
            if (include == 0) {
                assert(itofin_bond_payment(ctx, bond, direction, 1, settlement, &value, &error) == 0);
                check_payment(value, fields[column], fields[column + 1], &error);
            }
            query.with_amount = 0;
            assert(itofin_leg_payment(ctx, leg, query, &value, &error) == 0);
            assert(value.amount == 0.0);
            query.direction = 2;
            value.present = 9; value.date = -123; value.amount = -456.0;
            assert(itofin_leg_payment(ctx, leg, query, &value, &error) == ITOFIN_INVALID_ARGUMENT);
            assert(value.present == 9 && value.date == -123 && value.amount == -456.0);
        }
        ++checked;
    }
    assert(fclose(input) == 0 && checked == 60);
    assert(itofin_handle_release(ctx, bond, &error) == 0);
    assert(itofin_handle_release(ctx, settings, &error) == 0);
    assert(itofin_leg_span_date(ctx, leg, 0, &span, &error) == 0 && span == start);
    assert(itofin_context_free(ctx, &error) == 0);
    return 0;
}
