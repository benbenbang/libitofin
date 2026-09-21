#include <ql/indexes/ibor/euribor.hpp>
#include <ql/indexes/ibor/estr.hpp>
#include <ql/indexes/inflation/ukrpi.hpp>
#include <ql/math/interpolations/linearinterpolation.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/settings.hpp>
#include <ql/termstructures/yield/ratehelpers.hpp>
#include <ql/termstructures/yield/oisratehelper.hpp>
#include <ql/termstructures/yield/piecewiseyieldcurve.hpp>
#include <ql/termstructures/inflation/inflationhelpers.hpp>
#include <ql/termstructures/inflation/piecewiseyoyinflationcurve.hpp>
#include <ql/termstructures/inflation/piecewisezeroinflationcurve.hpp>
#include <ql/termstructures/yield/flatforward.hpp>
#include <ql/time/calendars/target.hpp>
#include <ql/time/calendars/unitedkingdom.hpp>
#include <ql/time/daycounters/actual360.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <ql/time/daycounters/thirty360.hpp>
#include <ql/version.hpp>
#include <iomanip>
#include <iostream>
using namespace QuantLib;

int main() {
    std::cout << std::setprecision(17) << "{\"quantlib\":\"" << QL_VERSION << "\",\"yield\":[";
    const Date today(15, June, 2026);
    Settings::instance().evaluationDate() = today;
    const auto ibor = ext::make_shared<Euribor3M>();
    const auto overnight = ext::make_shared<Estr>();
    for (int kind = 0; kind < 3; ++kind) {
        const auto quote = ext::make_shared<SimpleQuote>(0.03);
        const Handle<Quote> q(quote);
        const Date custom = kind == 0 ? Date(15, October, 2026) : Date(15, March, 2027);
        ext::shared_ptr<RateHelper> helper;
        if (kind == 0)
            helper = ext::make_shared<FraRateHelper>(q, Period(3, Months), ibor, Pillar::CustomDate, custom, true);
        else if (kind == 1)
            helper = ext::make_shared<SwapRateHelper>(q, Period(1, Years), TARGET(), Annual,
                ModifiedFollowing, Thirty360(Thirty360::BondBasis), ibor, Handle<Quote>(), Period(0, Days),
                Handle<YieldTermStructure>(), Null<Natural>(), Pillar::CustomDate, custom);
        else
            helper = ext::make_shared<OISRateHelper>(2, Period(1, Years), q, overnight,
                Handle<YieldTermStructure>(), false, 2, Following, Annual, Calendar(), Period(0, Days),
                0.0, Pillar::CustomDate, custom);
        PiecewiseYieldCurve<Discount, LogLinear> curve(today, {helper}, Actual365Fixed());
        const Real discount = curve.discount(helper->latestRelevantDate(), true);
        quote->setValue(0.031);
        const Real updated = curve.discount(helper->latestRelevantDate(), true);
        if (kind) std::cout << ',';
        std::cout << "{\"kind\":" << kind << ",\"earliest\":\"" << io::iso_date(helper->earliestDate())
                  << "\",\"maturity\":\"" << io::iso_date(helper->maturityDate())
                  << "\",\"latest_relevant\":\"" << io::iso_date(helper->latestRelevantDate())
                  << "\",\"pillar\":\"" << io::iso_date(helper->pillarDate())
                  << "\",\"latest\":\"" << io::iso_date(helper->latestDate())
                  << "\",\"discount\":" << discount << ",\"updated_discount\":" << updated << '}';
    }
    std::cout << "],\"inflation\":[";
    const Date inflationToday(13, August, 2007), base(1, July, 2007);
    Settings::instance().evaluationDate() = inflationToday;
    const auto zero = ext::make_shared<UKRPI>();
    const auto yoy = ext::make_shared<YYUKRPI>();
    for (Size i = 0; i < 4; ++i)
        zero->addFixing(Date(1, Month(4+i), 2007), std::vector<Real>{204.4,205.4,206.2,207.3}[i]);
    const auto dc = Thirty360(Thirty360::BondBasis);
    const Handle<YieldTermStructure> nominal(ext::make_shared<FlatForward>(inflationToday, 0.05, Actual360()));
    for (int flat = 0; flat < 2; ++flat) {
        const auto interpolation = flat ? CPI::Flat : CPI::Linear;
        const Handle<Quote> quote(ext::make_shared<SimpleQuote>(0.0295));
        const Date maturity(13, August, 2008), custom(15, June, 2008);
        const auto zh = ext::make_shared<ZeroCouponInflationSwapHelper>(quote, Period(2, Months), maturity,
            UnitedKingdom(), ModifiedFollowing, dc, zero, interpolation, Pillar::CustomDate, custom);
        const auto yh = ext::make_shared<YearOnYearInflationSwapHelper>(quote, Period(2, Months), maturity,
            UnitedKingdom(), ModifiedFollowing, dc, yoy, interpolation, nominal, Pillar::CustomDate, custom);
        PiecewiseZeroInflationCurve<Linear> zc(inflationToday, base, Monthly, dc, {zh});
        PiecewiseYoYInflationCurve<Linear> yc(inflationToday, base, 0.0295, Monthly, dc, {yh});
        if (flat) std::cout << ',';
        std::cout << "{\"flat\":" << (flat ? "true" : "false")
                  << ",\"pillar\":\"" << io::iso_date(zh->pillarDate())
                  << "\",\"latest\":\"" << io::iso_date(zh->latestDate())
                  << "\",\"yoy_pillar\":\"" << io::iso_date(yh->pillarDate())
                  << "\",\"yoy_latest\":\"" << io::iso_date(yh->latestDate())
                  << "\",\"zero_rate\":" << zc.nodes().back().second
                  << ",\"yoy_rate\":" << yc.nodes().back().second << '}';
    }
    std::cout << "]}\n";
}
