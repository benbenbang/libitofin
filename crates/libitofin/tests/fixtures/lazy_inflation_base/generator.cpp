#include <ql/indexes/inflation/ukrpi.hpp>
#include <ql/termstructures/inflation/piecewisezeroinflationcurve.hpp>
#include <ql/termstructures/inflation/inflationhelpers.hpp>
#include <ql/time/calendars/unitedkingdom.hpp>
#include <ql/time/daycounters/thirty360.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/version.hpp>
#include <cmath>
#include <iomanip>
#include <iostream>

using namespace QuantLib;

int main() {
    if (std::string(QL_VERSION) != "1.43-dev")
        return 2;
    Settings::instance().evaluationDate() = Date(13, August, 2007);
    const Date reference(13, August, 2007);
    const DayCounter dc = Thirty360(Thirty360::BondBasis);
    const std::vector<Date> maturities = {
        Date(13, August, 2008), Date(13, August, 2009), Date(13, August, 2010),
        Date(15, August, 2011), Date(13, August, 2012), Date(13, August, 2014),
        Date(13, August, 2017), Date(13, August, 2019), Date(15, August, 2022),
        Date(14, August, 2027), Date(13, August, 2032), Date(15, August, 2037),
        Date(13, August, 2047), Date(13, August, 2057)};
    const std::vector<Rate> rates = {2.93, 2.95, 2.965, 2.98, 3.0, 3.06, 3.175,
        3.243, 3.293, 3.338, 3.348, 3.348, 3.308, 3.228};
    auto index = ext::make_shared<UKRPI>();
    std::vector<ext::shared_ptr<SimpleQuote>> quotes;
    std::vector<ext::shared_ptr<BootstrapHelper<ZeroInflationTermStructure>>> helpers;
    for (const auto& maturity : maturities) {
        auto quote = ext::make_shared<SimpleQuote>();
        quotes.push_back(quote);
        helpers.push_back(ext::make_shared<ZeroCouponInflationSwapHelper>(
            Handle<Quote>(quote), Period(3, Months), maturity, UnitedKingdom(),
            ModifiedFollowing, dc, index, CPI::Flat));
    }
    auto lazy = ext::make_shared<PiecewiseZeroInflationCurve<Linear>>(
        reference, [index]() { return index->lastFixingDate(); }, Monthly, dc, helpers);
    for (Size i = 0; i < rates.size(); ++i)
        quotes[i]->setValue(rates[i] / 100.0);
    const std::vector<Real> fixings = {189.9, 189.9, 189.6, 190.5, 191.6, 192.0,
        192.2, 192.2, 192.6, 193.1, 193.3, 193.6, 194.1, 193.4, 194.2, 195.0,
        196.5, 197.7, 198.5, 198.5, 199.2, 200.1, 200.4, 201.1, 202.7, 201.6,
        203.1, 204.4, 205.4, 206.2, 207.3};
    for (Size i = 0; i < fixings.size(); ++i)
        index->addFixing(Date(1, January, 2005) + Period(Integer(i), Months), fixings[i]);
    auto fixed = ext::make_shared<PiecewiseZeroInflationCurve<Linear>>(
        reference, index->lastFixingDate(), Monthly, dc, helpers);
    if (lazy->baseDate() != fixed->baseDate() || lazy->nodes() != fixed->nodes())
        return 3;
    lazy->update();
    auto forecastIndex = index->clone(Handle<ZeroInflationTermStructure>(lazy));
    std::cout << "phase,node,date,rate,forecast,base_date,persistent_node_date\n" << std::setprecision(17);
    for (int phase = 0; phase < 5; ++phase) {
        if (phase == 1) {
            index->clearFixings();
            for (Size i = 0; i < fixings.size(); ++i)
                index->addFixing(Date(1, January, 2005) + Period(Integer(i), Months),
                                 i + 1 == fixings.size() ? 208.1 : fixings[i]);
        }
        if (phase == 2) {
            Settings::instance().evaluationDate() = Date(13, September, 2007);
            index->addFixing(Date(1, August, 2007), 208.4);
        }
        if (phase == 3)
            Settings::instance().evaluationDate() = Date(13, October, 2007);
        if (phase == 4)
            quotes[0]->setValue(rates[0] / 100.0 + 0.0005);
        const auto persistentNodes = lazy->nodes();
        const auto persistentForecast = forecastIndex->fixing(Date(1, August, 2012));
        auto fresh = ext::make_shared<PiecewiseZeroInflationCurve<Linear>>(
            reference, index->lastFixingDate(), Monthly, dc, helpers);
        const auto nodes = fresh->nodes();
        const auto forecast = index->clone(Handle<ZeroInflationTermStructure>(fresh))
                                  ->fixing(Date(1, August, 2012));
        if (std::fabs(forecast - persistentForecast) >= 1e-7)
            return 4;
        for (Size i = 0; i < nodes.size(); ++i) {
            if (std::fabs(nodes[i].second - persistentNodes[i].second) >= 1e-12)
                return 5;
            std::cout << phase << ',' << i << ',' << nodes[i].first.serialNumber()
                      << ',' << nodes[i].second << ',' << forecast << ','
                      << lazy->baseDate().serialNumber() << ','
                      << persistentNodes[i].first.serialNumber() << '\n';
        }
    }
}
