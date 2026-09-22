#include <ql/quantlib.hpp>
#include <iomanip>
#include <iostream>
using namespace QuantLib;
extern "C" void optionlet_oracle() {
    std::cout << std::setprecision(17);
    Settings::instance().evaluationDate() = Date(28, October, 2013);
    auto curve = Handle<YieldTermStructure>(ext::make_shared<FlatForward>(
        0, TARGET(), 0.04, Actual365Fixed()));
    auto index = ext::make_shared<Euribor6M>(curve);
    std::vector<Period> tenors = {1*Years, 2*Years, 3*Years, 5*Years, 7*Years, 10*Years};
    std::vector<Real> strikes = {.01, .02, .04, .06, .1};
    Matrix normal(tenors.size(), strikes.size());
    for (Size i=0; i<tenors.size(); ++i)
        for (Size j=0; j<strikes.size(); ++j)
            normal[i][j] = .008 + .0002*i + .002*strikes[j];
    auto surface = ext::make_shared<CapFloorTermVolSurface>(
        0, TARGET(), Following, tenors, strikes, normal, Actual365Fixed());
    auto strip = ext::make_shared<OptionletStripper1>(surface, index, Null<Rate>(), 1e-10,
        100, Handle<YieldTermStructure>(), Normal);
    StrippedOptionletAdapter adapter(strip);
    adapter.enableExtrapolation();
    std::cout << "normal_smile " << adapter.smileSection(4.0)->volatility(.035) << '\n';
    Matrix flat(tenors.size(), strikes.size(), .18);
    surface = ext::make_shared<CapFloorTermVolSurface>(
        0, TARGET(), Following, tenors, strikes, flat, Actual365Fixed());
    strip = ext::make_shared<OptionletStripper1>(surface, index, Null<Rate>(), 1e-10);
    auto atm = Handle<CapFloorTermVolCurve>(ext::make_shared<CapFloorTermVolCurve>(
        0, TARGET(), Following, std::vector<Period>{1*Years,3*Years,5*Years},
        std::vector<Volatility>{.20,.20,.20}, Actual365Fixed()));
    OptionletStripper2 corrected(strip, atm);
    auto levels = corrected.atmCapFloorStrikes();
    auto prices = corrected.atmCapFloorPrices();
    auto spreads = corrected.spreadsVol();
    for (Size i=0; i<levels.size(); ++i)
        std::cout << "atm " << i << ' ' << levels[i] << ' ' << prices[i] << ' ' << spreads[i] << '\n';
}
