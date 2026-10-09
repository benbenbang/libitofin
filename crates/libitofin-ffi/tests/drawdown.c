#include "itofin.h"
#include <assert.h>
#include <math.h>

int main(void) {
    double values[] = {100, 120, 90, 130};
    ItofinDrawdownResult result = {91, 92, 93};
    ItofinError error = {0, {0}};
    assert(itofin_maximum_drawdown(values, 4, &result, &error) == 0);
    assert(result.drawdown == 0.25 && result.peak_index == 1 && result.trough_index == 2);
    values[3] = NAN;
    assert(itofin_maximum_drawdown(values, 4, &result, &error) != 0);
    assert(result.drawdown == 0.25 && result.peak_index == 1 && result.trough_index == 2);
    assert(itofin_maximum_drawdown(NULL, 4, &result, &error) != 0);
    assert(itofin_maximum_drawdown(NULL, 0, &result, &error) != 0);
    assert(itofin_maximum_drawdown(values, 4, NULL, &error) != 0);
    values[0] = 1;
    assert(itofin_maximum_drawdown(values, 1, &result, NULL) == 0);
    assert(result.drawdown == 0 && result.peak_index == 0 && result.trough_index == 0);
    values[0] = 0;
    assert(itofin_maximum_drawdown(values, 1, &result, &error) != 0);
    assert(result.drawdown == 0 && result.peak_index == 0 && result.trough_index == 0);
    return 0;
}
