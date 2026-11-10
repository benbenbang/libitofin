#include <ql/quantlib.hpp>
#include <iomanip>
#include <iostream>
using namespace QuantLib;

static void run(const char* name, VolatilityType type, Real displacement,
                const Matrix& vols, const std::vector<Volatility>& atmVols) {
    auto curve = Handle<YieldTermStructure>(ext::make_shared<FlatForward>(
        0, TARGET(), 0.04, Actual365Fixed()));
    auto index = ext::make_shared<Euribor6M>(curve);
    std::vector<Period> tenors = {1*Years, 2*Years, 3*Years, 5*Years, 7*Years, 10*Years};
    std::vector<Real> strikes = {.01, .02, .04, .06, .1};
    auto surface = ext::make_shared<CapFloorTermVolSurface>(
        0, TARGET(), Following, tenors, strikes, vols, Actual365Fixed());
    auto strip = ext::make_shared<OptionletStripper1>(surface, index, Null<Rate>(), 1e-10,
        100, Handle<YieldTermStructure>(), type, displacement);
    auto atm = Handle<CapFloorTermVolCurve>(ext::make_shared<CapFloorTermVolCurve>(
        0, TARGET(), Following, std::vector<Period>{1*Years, 3*Years, 5*Years},
        atmVols, Actual365Fixed()));
    OptionletStripper2 corrected(strip, atm);
    auto levels = corrected.atmCapFloorStrikes();
    auto prices = corrected.atmCapFloorPrices();
    auto spreads = corrected.spreadsVol();
    for (Size i=0; i<levels.size(); ++i)
        std::cout << name << ' ' << i << ' ' << levels[i] << ' ' << prices[i] << ' '
                  << spreads[i] << '\n';
}

int main() {
    std::cout << std::setprecision(17);
    Settings::instance().evaluationDate() = Date(28, October, 2013);
    Matrix normal(6, 5);
    std::vector<Real> strikes = {.01, .02, .04, .06, .1};
    for (Size i=0; i<6; ++i)
        for (Size j=0; j<5; ++j)
            normal[i][j] = .008 + .0002*i + .002*strikes[j];
    Matrix flat(6, 5, .18);
    Matrix steep(6, 5);
    for (Size i=0; i<6; ++i)
        for (Size j=0; j<5; ++j)
            steep[i][j] = .05 + .05*i;
    run("lognormal", ShiftedLognormal, 0.0, flat, {.20, .20, .20});
    run("lognormal_below", ShiftedLognormal, 0.0, flat, {.17, .17, .17});
    run("displaced", ShiftedLognormal, 0.01, flat, {.20, .20, .20});
    run("normal", Normal, 0.0, normal, {.0085, .0095, .0100});
    try {
        run("steep", ShiftedLognormal, 0.0, steep, {.01, .01, .01});
    } catch (std::exception& e) {
        std::cout << "steep error " << e.what() << '\n';
    }
}
