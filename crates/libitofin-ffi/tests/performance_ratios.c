#include "itofin.h"
#include <assert.h>
#include <float.h>
#include <math.h>
#include <stddef.h>
#include <string.h>

int main(void) {
    const double returns[] = {.02, -.01, .03, -.02};
    const double positive[] = {.01, .02};
    const double flat[] = {.01, .01};
    const double nonfinite[] = {1, INFINITY};
    double output = 99;
    ItofinError error;
    memset(&error, 0, sizeof(error));
    assert(itofin_target_downside_deviation(returns, 4, 0, &output, &error) == 0);
    assert(fabs(output - sqrt(.0005 / 4)) < 1e-14);
    assert(itofin_sharpe_ratio(returns, 4, 0, 1, &output, &error) == 0);
    assert(fabs(output - .005 / sqrt(.0017 / 3)) < 1e-14);
    assert(itofin_sortino_ratio(returns, 4, 0, 12, &output, &error) == 0);
    assert(fabs(output - sqrt(12.0 / 5)) < 1e-14);
    output = 99;
    assert(itofin_sharpe_ratio(flat, 2, 0, 12, &output, &error) != 0);
    assert(output == 99 && error.code != 0);
    assert(itofin_sortino_ratio(positive, 2, 0, 12, &output, &error) != 0);
    assert(output == 99);
    assert(itofin_sharpe_ratio(NULL, 2, 0, 12, &output, &error) != 0);
    assert(itofin_sortino_ratio(NULL, 2, 0, 12, &output, &error) != 0);
    assert(itofin_target_downside_deviation(NULL, 2, 0, &output, &error) != 0);
    assert(itofin_target_downside_deviation(NULL, 0, 0, &output, &error) != 0);
    assert(itofin_sharpe_ratio(returns, 4, 0, 12, NULL, &error) != 0);
    assert(itofin_sortino_ratio(returns, 4, 0, 12, NULL, &error) != 0);
    assert(itofin_target_downside_deviation(returns, 4, 0, NULL, &error) != 0);
    assert(itofin_sharpe_ratio(nonfinite, 2, 0, 12, &output, &error) != 0);
    assert(itofin_sortino_ratio(returns, 4, 0, 0, &output, &error) != 0);
    assert(itofin_sharpe_ratio(returns, 4, NAN, 12, &output, &error) != 0);
    assert(itofin_target_downside_deviation(returns, 4, INFINITY, &output, &error) != 0);
    assert(output == 99);
    assert(itofin_target_downside_deviation(positive, 2, 0, &output, &error) == 0);
    assert(output == 0 && error.code == 0);
    assert(itofin_sortino_ratio(returns, 4, 0, 12, &output, NULL) == 0);
    {
        const double mixed[] = {1e200, -1};
        assert(itofin_target_downside_deviation(mixed, 2, 0, &output, &error) == 0);
        assert(fabs(output - sqrt(.5)) < 1e-14);
    }
    return 0;
}
