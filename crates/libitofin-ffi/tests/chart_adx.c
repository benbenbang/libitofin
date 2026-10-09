#include "itofin.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

int main(void) {
    const double high[] = {10, 12, 11, 14, 15, 13, 18, 16};
    const double low[] = {8, 9, 7, 10, 9, 9, 15, 13};
    const double close[] = {9, 11, 8, 13, 10, 12, 17, 14};
    const double expected[] = {
        0, 0, 200.0/7, 800.0/19, 800.0/43, 32.0/3, 8800.0/171, 8800.0/299,
        0, 0, 200.0/7, 200.0/19, 200.0/43, 8.0/3, 200.0/171, 6600.0/299,
        0, 0, 0, 60, 60, 60, 860.0/9, 100.0/7,
        0, 0, 0, 30, 45, 105.0/2, 2665.0/36, 22255.0/504
    };
    double output[32];
    size_t valid[4] = {77, 77, 77, 77};
    ItofinError error;
    memset(&error, 0, sizeof(error));
    assert(itofin_chart_adx(high, low, close, 8, 2, output, 32, valid, 4, &error) == 0);
    assert(valid[0] == 2 && valid[1] == 2 && valid[2] == 2 && valid[3] == 3);
    for (size_t i = 0; i < 32; ++i) { assert(fabs(output[i] - expected[i]) < 1e-12); output[i] = 99; }
    for (size_t i = 0; i < 4; ++i) { valid[i] = 77; }
    assert(itofin_chart_adx(high, low, close, 8, 2, output, 31, valid, 4, &error) != 0);
    assert(itofin_chart_adx(high, low, close, 8, 2, output, 32, valid, 3, &error) != 0);
    assert(itofin_chart_adx(NULL, low, close, 8, 2, output, 32, valid, 4, &error) != 0);
    assert(itofin_chart_adx(high, NULL, close, 8, 2, output, 32, valid, 4, &error) != 0);
    assert(itofin_chart_adx(high, low, NULL, 8, 2, output, 32, valid, 4, &error) != 0);
    assert(itofin_chart_adx(high, low, close, 8, 2, NULL, 32, valid, 4, &error) != 0);
    assert(itofin_chart_adx(high, low, close, 8, 2, output, 32, NULL, 4, &error) != 0);
    assert(itofin_chart_adx(high, low, close, 8, 0, output, 32, valid, 4, &error) != 0);
    {
        const double h[] = {DBL_MAX, -DBL_MAX};
        const double l[] = {0, -DBL_MAX};
        const double c[] = {0, -DBL_MAX};
        assert(itofin_chart_adx(h, l, c, 2, 14, output, 32, valid, 4, &error) != 0);
    }
    for (size_t i = 0; i < 32; ++i) { assert(output[i] == 99); }
    for (size_t i = 0; i < 4; ++i) { assert(valid[i] == 77); }
    assert(itofin_chart_adx(NULL, NULL, NULL, 0, 14, NULL, 0, valid, 4, NULL) == 0);
    for (size_t i = 0; i < 4; ++i) { assert(valid[i] == 0); }
    return 0;
}
