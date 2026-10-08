#include "itofin.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

typedef int32_t (*volume_function)(const double *, const double *, size_t,
                                  double *, size_t, size_t *, ItofinError *);

int main(void) {
    const double price[] = {10, 12, 11, 11, 9};
    const double volume[] = {0, 2, 1, 0, 3};
    const double expected_vwap[] = {0, 12, 35.0 / 3, 35.0 / 3, 31.0 / 3};
    const double expected_obv[] = {0, 2, 1, 1, -2};
    volume_function functions[] = {itofin_chart_vwap, itofin_chart_obv};
    const double *expected[] = {expected_vwap, expected_obv};
    ItofinError error;
    memset(&error, 0, sizeof(error));
    for (size_t method = 0; method < 2; ++method) {
        double output[] = {99, 99, 99, 99, 99};
        size_t first_valid = 77;
        assert(functions[method](price, volume, 5, output, 5, &first_valid, &error) == 0);
        assert(first_valid == (method == 0 ? 1 : 0));
        for (size_t i = 0; i < 5; ++i) {
            assert(fabs(output[i] - expected[method][i]) < 1e-12);
        }
        first_valid = 77;
        output[0] = 99;
        assert(functions[method](price, volume, 5, output, 4, &first_valid, &error) != 0);
        assert(first_valid == 77 && output[0] == 99);
        assert(functions[method](NULL, volume, 5, output, 5, &first_valid, &error) != 0);
        assert(first_valid == 77 && output[0] == 99);
        assert(functions[method](price, NULL, 5, output, 5, &first_valid, &error) != 0);
        assert(functions[method](price, volume, 5, NULL, 5, &first_valid, &error) != 0);
        assert(functions[method](price, volume, 5, output, 5, NULL, &error) != 0);
        assert(first_valid == 77 && output[0] == 99);
        assert(functions[method](NULL, NULL, 0, NULL, 0, &first_valid, &error) == 0);
        assert(first_valid == 0);
    }
    {
        const double close[] = {0, 1, 2};
        const double excessive[] = {0, DBL_MAX, DBL_MAX};
        double output[] = {99, 99, 99};
        size_t first_valid = 77;
        for (size_t method = 0; method < 2; ++method) {
            assert(functions[method](close, excessive, 3, output, 3, &first_valid, &error) != 0);
            assert(first_valid == 77 && output[2] == 99);
        }
    }
    {
        const double prices[] = {DBL_MAX, 0};
        const double volumes[] = {1, DBL_MAX};
        double output[2];
        size_t first_valid;
        assert(itofin_chart_vwap(prices, volumes, 2, output, 2, &first_valid, &error) == 0);
        assert(first_valid == 0 && fabs(output[1] - 1) < 1e-14);
    }
    return 0;
}
