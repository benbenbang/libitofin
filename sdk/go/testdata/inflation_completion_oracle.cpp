#include <ql/currencies/europe.hpp>
#include <ql/indexes/inflation/ukrpi.hpp>
#include <ql/math/interpolations/linearinterpolation.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/settings.hpp>
#include <ql/termstructures/inflation/inflationhelpers.hpp>
#include <ql/termstructures/inflation/piecewiseyoyinflationcurve.hpp>
#include <ql/termstructures/inflation/piecewisezeroinflationcurve.hpp>
#include <ql/termstructures/yield/flatforward.hpp>
#include <ql/time/calendars/unitedkingdom.hpp>
#include <ql/time/daycounters/actual360.hpp>
#include <ql/time/daycounters/thirty360.hpp>
#include <ql/version.hpp>
#include <iomanip>
#include <iostream>

using namespace QuantLib;

int main() {
    const Date today(13, August, 2007), base(1, July, 2007);
    Settings::instance().evaluationDate() = today;
    const auto zero = ext::make_shared<UKRPI>();
    const auto yoy = ext::make_shared<YYUKRPI>();
    const std::vector<Real> history{204.4, 205.4, 206.2, 207.3};
    for (Size i = 0; i < history.size(); ++i)
        zero->addFixing(Date(1, Month(4 + i), 2007), history[i]);
    const auto dc = Thirty360(Thirty360::BondBasis);
    const auto calendar = UnitedKingdom();
    const Handle<YieldTermStructure> nominal(
        ext::make_shared<FlatForward>(today, 0.05, Actual360()));
    const Handle<Quote> quote(ext::make_shared<SimpleQuote>(0.0295));
    std::cout << std::setprecision(17) << "QuantLib," << QL_VERSION << '\n';
    for (int i = 0; i < 4; ++i) {
        const Date maturity(i == 2 ? 20 : 13, August, 2008);
        const auto interpolation = i == 0 ? CPI::Flat : CPI::Linear;
        const auto pillar = i == 3 ? Pillar::MaturityDate : Pillar::LastRelevantDate;
        const ZeroCouponInflationSwapHelper zh(
            quote, Period(2, Months), maturity, calendar, ModifiedFollowing,
            dc, zero, interpolation, pillar);
        const YearOnYearInflationSwapHelper yh(
            quote, Period(2, Months), maturity, calendar, ModifiedFollowing,
            dc, yoy, interpolation, nominal, pillar);
        std::cout << "zero-helper," << i << ',' << io::iso_date(zh.pillarDate())
                  << ',' << io::iso_date(zh.latestDate()) << '\n';
        std::cout << "yoy-helper," << i << ',' << io::iso_date(yh.pillarDate())
                  << ',' << io::iso_date(yh.latestDate()) << '\n';
    }
    std::vector<ext::shared_ptr<BootstrapHelper<ZeroInflationTermStructure>>> zh;
    std::vector<ext::shared_ptr<BootstrapHelper<YoYInflationTermStructure>>> yh;
    for (int year : {2008, 2009}) {
        zh.push_back(ext::make_shared<ZeroCouponInflationSwapHelper>(
            quote, Period(2, Months), Date(13, August, year), calendar,
            ModifiedFollowing, dc, zero, CPI::Flat));
        yh.push_back(ext::make_shared<YearOnYearInflationSwapHelper>(
            quote, Period(2, Months), Date(13, August, year), calendar,
            ModifiedFollowing, dc, yoy, CPI::Flat, nominal));
    }
    const PiecewiseZeroInflationCurve<Linear> zc(today, base, Monthly, dc, zh);
    const PiecewiseYoYInflationCurve<Linear> yc(today, base, 0.0295, Monthly, dc, yh);
    const auto zn = zc.nodes();
    const auto yn = yc.nodes();
    for (Size i = 0; i < zn.size(); ++i)
        std::cout << "zero-node," << io::iso_date(zn[i].first) << ','
                  << zc.times()[i] << ',' << zn[i].second << '\n';
    for (Size i = 0; i < yn.size(); ++i)
        std::cout << "yoy-node," << io::iso_date(yn[i].first) << ','
                  << yc.times()[i] << ',' << yn[i].second << '\n';
}
