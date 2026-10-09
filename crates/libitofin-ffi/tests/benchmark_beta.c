#include "itofin.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

int main(void) {
    const double asset[] = {1, 3, 2, 10};
    const double benchmark[] = {-1, 0, 2, 9};
    const double weights[] = {1, 2, 3, 0};
    const double constant[] = {3, 3, 3, 3};
    const double invalid[] = {NAN, 1, 2, 3};
    const double negative[] = {-1, -3, -2, -10};
    double out = 91;
    ItofinError error;
    memset(&error, 0, sizeof(error));
    for (size_t i = 0; i < 10; ++i) {
        assert(itofin_benchmark_beta(asset, 4, benchmark, 4, weights, 4, &out, &error) == 0);
        assert(fabs(out - 1.0 / 53) < 2e-12 && error.code == 0);
    }
    assert(itofin_benchmark_beta(asset, 4, benchmark, 4, NULL, 0, &out, &error) == 0);
    assert(fabs(out - 53.0 / 61) < 2e-12);
    assert(itofin_benchmark_beta(asset, 4, asset, 4, NULL, 0, &out, &error) == 0);
    assert(out == 1);
    assert(itofin_benchmark_beta(negative, 4, asset, 4, NULL, 0, &out, &error) == 0);
    assert(out == -1);
    assert(itofin_benchmark_beta(constant, 4, asset, 4, NULL, 0, &out, &error) == 0);
    assert(out == 0);
    out = 91;
    assert(itofin_benchmark_beta(asset, 4, constant, 4, NULL, 0, &out, &error) != 0);
    assert(itofin_benchmark_beta(NULL, 4, benchmark, 4, NULL, 0, &out, &error) != 0);
    assert(itofin_benchmark_beta(asset, 4, NULL, 4, NULL, 0, &out, &error) != 0);
    assert(itofin_benchmark_beta(asset, 4, benchmark, 3, NULL, 0, &out, &error) != 0);
    assert(itofin_benchmark_beta(asset, 4, benchmark, 4, NULL, 4, &out, &error) != 0);
    assert(itofin_benchmark_beta(asset, 4, benchmark, 4, weights, 3, &out, &error) != 0);
    assert(itofin_benchmark_beta(asset, 4, benchmark, 4, invalid, 4, &out, &error) != 0);
    assert(itofin_benchmark_beta(invalid, 4, benchmark, 4, weights, 4, &out, &error) != 0);
    assert(itofin_benchmark_beta(asset, 4, benchmark, 4, weights, 4, NULL, &error) != 0);
    assert(itofin_benchmark_beta(NULL, 0, NULL, 0, NULL, 0, &out, &error) != 0);
    assert(out == 91);
    assert(itofin_benchmark_beta(asset, 4, benchmark, 4, weights, 4, &out, NULL) == 0);
    assert(fabs(out - 1.0 / 53) < 2e-12);
    return 0;
}
