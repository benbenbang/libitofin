#include "itofin.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

int main(void) {
    const double high[] = {12, 16, 11, 15, 14, 14, 20, 18, 13};
    const double low[] = {10, 14, 9, 13, 12, 14, 16, 15, 11};
    const double close[] = {11, 15, 10, 14, 13, 14, 20, 15, 11};
    const double expected[] = {0, 0, -600.0/7, -200.0/7, -100.0/3, -100.0/3, 0, -250.0/3, -100};
    double output[10];
    size_t first_valid = 77;
    ItofinError error;
    memset(&error, 0, sizeof(error));
    output[9] = 123;
    assert(itofin_chart_williams_r(high, low, close, 9, 3, output, 10, &first_valid, &error) == 0);
    assert(first_valid == 2 && output[9] == 123);
    for (size_t i = 0; i < 9; ++i) { assert(fabs(output[i] - expected[i]) < 1e-12); }
    for (size_t i = 0; i < 10; ++i) { output[i] = 99; }
    first_valid = 77;
    assert(itofin_chart_williams_r(high, low, close, 9, 3, output, 8, &first_valid, &error) != 0);
    assert(itofin_chart_williams_r(high, low, close, 9, 0, output, 10, &first_valid, &error) != 0);
    assert(itofin_chart_williams_r(NULL, low, close, 9, 3, output, 10, &first_valid, &error) != 0);
    assert(itofin_chart_williams_r(high, NULL, close, 9, 3, output, 10, &first_valid, &error) != 0);
    assert(itofin_chart_williams_r(high, low, NULL, 9, 3, output, 10, &first_valid, &error) != 0);
    assert(itofin_chart_williams_r(high, low, close, 9, 3, NULL, 10, &first_valid, &error) != 0);
    assert(itofin_chart_williams_r(high, low, close, 9, 3, output, 10, NULL, &error) != 0);
    assert(first_valid == 77);
    for (size_t i = 0; i < 10; ++i) { assert(output[i] == 99); }
    {
        const double huge_high[] = {DBL_MAX};
        const double huge_low[] = {-DBL_MAX};
        const double zero[] = {0};
        const double bad[] = {NAN};
        const double unordered[] = {2};
        assert(itofin_chart_williams_r(huge_high, huge_low, zero, 1, 14, output, 10, &first_valid, &error) != 0);
        assert(itofin_chart_williams_r(bad, zero, zero, 1, 14, output, 10, &first_valid, &error) != 0);
        assert(itofin_chart_williams_r(zero, zero, unordered, 1, 14, output, 10, &first_valid, &error) != 0);
        assert(first_valid == 77 && output[0] == 99);
        const double flats[] = {-DBL_MAX, DBL_MAX};
        assert(itofin_chart_williams_r(flats, flats, flats, 2, 14, output, 10, &first_valid, &error) != 0);
        assert(first_valid == 77 && output[0] == 99);
        assert(itofin_chart_williams_r(flats, flats, flats, 2, 1, output, 10, &first_valid, NULL) == 0);
        assert(first_valid == 0 && output[0] == -50 && output[1] == -50);
    }
    output[0] = 99;
    first_valid = 77;
    assert(itofin_chart_williams_r(high, low, close, 9, 3, output, 10,
        &first_valid, (ItofinError *)((unsigned char *)&error + 1)) != 0);
    assert(first_valid == 77 && output[0] == 99);
    first_valid = 77;
    assert(itofin_chart_williams_r(NULL, NULL, NULL, 0, 0, NULL, 0, &first_valid, &error) != 0);
    assert(first_valid == 77);
    assert(itofin_chart_williams_r(NULL, NULL, NULL, 0, 14, NULL, 0, &first_valid, &error) == 0);
    assert(first_valid == 0);
    return 0;
}
