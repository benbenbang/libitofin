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
    const double tr[] = {2, 5, 6, 5, 2, 1};
    const double expected[] = {0, 0, 13.0 / 3, 41.0 / 9, 100.0 / 27, 227.0 / 81};
    double output[6];
    size_t first_valid = 77;
    ItofinError error;
    memset(&error, 0, sizeof(error));
    assert(itofin_chart_true_range(high, low, close, 6, output, 6, &first_valid, &error) == 0);
    assert(first_valid == 0);
    for (size_t i = 0; i < 6; ++i) { assert(output[i] == tr[i]); }
    assert(itofin_chart_atr(high, low, close, 6, 3, output, 6, &first_valid, &error) == 0);
    assert(first_valid == 2);
    for (size_t i = 0; i < 6; ++i) { assert(fabs(output[i] - expected[i]) < 1e-12); }
    for (size_t i = 0; i < 6; ++i) { output[i] = 99; }
    first_valid = 77;
    assert(itofin_chart_atr(high, low, close, 6, 3, output, 5, &first_valid, &error) != 0);
    assert(first_valid == 77);
    assert(itofin_chart_true_range(NULL, low, close, 6, output, 6, &first_valid, &error) != 0);
    assert(itofin_chart_atr(high, NULL, close, 6, 3, output, 6, &first_valid, &error) != 0);
    assert(itofin_chart_atr(high, low, NULL, 6, 3, output, 6, &first_valid, &error) != 0);
    assert(itofin_chart_atr(high, low, close, 6, 3, NULL, 6, &first_valid, &error) != 0);
    assert(itofin_chart_atr(high, low, close, 6, 3, output, 6, NULL, &error) != 0);
    assert(first_valid == 77);
    for (size_t i = 0; i < 6; ++i) { assert(output[i] == 99); }
    assert(itofin_chart_atr(NULL, NULL, NULL, 0, 14, NULL, 0, &first_valid, &error) == 0);
    assert(first_valid == 0);
    first_valid = 77;
    assert(itofin_chart_atr(NULL, NULL, NULL, 0, 0, NULL, 0, &first_valid, &error) != 0);
    assert(first_valid == 77);
    {
        const double huge_high[] = {DBL_MAX};
        const double huge_low[] = {-DBL_MAX};
        const double zero[] = {0};
        assert(itofin_chart_true_range(huge_high, huge_low, zero, 1, output, 6, &first_valid, &error) != 0);
        assert(itofin_chart_atr(huge_high, huge_low, zero, 1, 14, output, 6, &first_valid, &error) != 0);
        assert(first_valid == 77 && output[0] == 99);
    }
    return 0;
}
