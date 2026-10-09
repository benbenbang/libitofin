#include "itofin.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

int main(void) {
    const double high[] = {12, 16, 11, 15, 14, 14};
    const double low[] = {10, 14, 9, 13, 12, 14};
    const double close[] = {11, 15, 10, 14, 13, 14};
    const double expected[] = {0, 0, 12, 13, 13, 13.5, 0, 0, 153.0/8, 325.0/16, 581.0/32, 1077.0/64, 0, 0, 39.0/8, 91.0/16, 251.0/32, 651.0/64};
    double output[18];
    size_t first_valid = 77;
    ItofinError error;
    memset(&error, 0, sizeof(error));
    assert(itofin_chart_keltner_channels(high, low, close, 6, 3, 2, 1.5, output, 18, &first_valid, &error) == 0);
    assert(first_valid == 2);
    for (size_t i = 0; i < 18; ++i) { assert(fabs(output[i] - expected[i]) < 1e-12); }
    for (size_t i = 0; i < 18; ++i) { output[i] = 99; }
    first_valid = 77;
    assert(itofin_chart_keltner_channels(high, low, close, 6, 3, 2, 1.5, output, 17, &first_valid, &error) != 0);
    assert(itofin_chart_keltner_channels(NULL, low, close, 6, 3, 2, 1.5, output, 18, &first_valid, &error) != 0);
    assert(itofin_chart_keltner_channels(high, NULL, close, 6, 3, 2, 1.5, output, 18, &first_valid, &error) != 0);
    assert(itofin_chart_keltner_channels(high, low, NULL, 6, 3, 2, 1.5, output, 18, &first_valid, &error) != 0);
    assert(itofin_chart_keltner_channels(high, low, close, 6, 3, 2, 1.5, NULL, 18, &first_valid, &error) != 0);
    assert(itofin_chart_keltner_channels(high, low, close, 6, 3, 2, 1.5, output, 18, NULL, &error) != 0);
    assert(itofin_chart_keltner_channels(high, low, close, 6, 0, 2, 1.5, output, 18, &first_valid, &error) != 0);
    assert(itofin_chart_keltner_channels(high, low, close, 6, 3, 0, 1.5, output, 18, &first_valid, &error) != 0);
    assert(itofin_chart_keltner_channels(high, low, close, 6, 3, 2, INFINITY, output, 18, &first_valid, &error) != 0);
    assert(first_valid == 77);
    for (size_t i = 0; i < 18; ++i) { assert(output[i] == 99); }
    {
        const double high_max[] = {DBL_MAX};
        const double low_max[] = {-DBL_MAX};
        const double zero[] = {0};
        assert(itofin_chart_keltner_channels(high_max, low_max, zero, 1, 20, 10, 0, output, 18, &first_valid, &error) != 0);
        assert(itofin_chart_keltner_channels(high_max, zero, zero, 1, 1, 1, 2, output, 18, &first_valid, &error) != 0);
        assert(itofin_chart_keltner_channels(high_max, zero, high_max, 1, 1, 1, 1, output, 18, &first_valid, &error) != 0);
        assert(itofin_chart_keltner_channels(zero, low_max, low_max, 1, 1, 1, 1, output, 18, &first_valid, &error) != 0);
        assert(first_valid == 77 && output[0] == 99);
    }
    {
        const double late_high[] = {2, DBL_MAX};
        const double late_low[] = {0, 0};
        const double late_close[] = {1, DBL_MAX};
        assert(itofin_chart_keltner_channels(late_high, late_low, late_close, 2, 1, 1, 1, output, 18, &first_valid, &error) != 0);
        assert(first_valid == 77);
        for (size_t i = 0; i < 18; ++i) { assert(output[i] == 99); }
    }
    assert(itofin_chart_keltner_channels(NULL, NULL, NULL, 0, 20, 10, 2, NULL, 0, &first_valid, &error) == 0);
    assert(first_valid == 0);
    return 0;
}
