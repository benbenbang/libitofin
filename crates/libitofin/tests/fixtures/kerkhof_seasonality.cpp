#include <ql/termstructures/inflation/interpolatedzeroinflationcurve.hpp>
#include <ql/termstructures/inflation/seasonality.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <ql/time/daycounters/thirty360.hpp>
#include <ql/version.hpp>
#include <iomanip>
#include <iostream>
#include <limits>
#include <vector>

using namespace QuantLib;

int main() {
    std::cerr << "QuantLib " << QL_VERSION << '\n';
    const std::vector<Rate> factors = {
        1.20, 1.004, 0.997, 1.006, 0.995, 1.003,
        0.991, 1.008, 0.998, 1.005, 0.996, 1.002};
    std::cout << std::setprecision(17);
    std::cout << "anchor,base,date,frequency,daycounter,factor,corrected,multiplicative,consistent,curve\n";
    for (Month anchorMonth : {January, July, December}) {
        const Date anchor = Date::endOfMonth(Date(1, anchorMonth, 2007));
        auto seasonality = ext::make_shared<KerkhofSeasonality>(anchor, factors);
        MultiplicativePriceSeasonality multiplicative(anchor, Monthly, factors);
        for (Frequency frequency : {Monthly, Quarterly}) {
            for (bool thirty360 : {false, true}) {
                const DayCounter dc = thirty360
                    ? DayCounter(Thirty360(Thirty360::BondBasis)) : DayCounter(Actual365Fixed());
                ZeroInflationCurve curve(Date(13, August, 2007),
                    {Date(1, July, 2007), Date(1, January, 2015)}, {0.02, 0.035}, frequency, dc);
                for (Date date : {Date(1, January, 2006), Date(31, July, 2007),
                                  Date(31, December, 2007), Date(29, February, 2008),
                                  Date(15, August, 2008), Date(31, July, 2010)}) {
                    const Rate corrected = seasonality->correctZeroRate(date, 0.027, curve);
                    const Rate other = multiplicative.correctZeroRate(date, 0.027, curve);
                    curve.setSeasonality(seasonality);
                    const Rate curveRate = date < curve.baseDate()
                        ? std::numeric_limits<Rate>::quiet_NaN() : curve.zeroRate(date, true);
                    std::cout << anchor.serialNumber() << ',' << curve.baseDate().serialNumber()
                              << ',' << date.serialNumber() << ',' << int(frequency)
                              << ',' << (thirty360 ? "30/360" : "A365")
                              << ',' << seasonality->seasonalityFactor(date)
                              << ',' << corrected << ',' << other
                              << ',' << seasonality->isConsistent(curve) << ',' << curveRate << '\n';
                }
                try {
                    seasonality->correctYoYRate(Date(15, August, 2008), 0.027, curve);
                    return 1;
                } catch (const Error& error) {
                    if (std::string(error.what()).find("not defined on YoY rates") == std::string::npos)
                        throw;
                }
            }
        }
    }
    for (Size count : {Size(0), Size(11), Size(13), Size(24)}) {
        try {
            KerkhofSeasonality invalid(Date(1, January, 2007), std::vector<Rate>(count, 1.0));
            invalid.seasonalityFactor(Date(1, March, 2008));
            return 1;
        } catch (const Error&) {
            std::cerr << "Rejected " << count << " factors\n";
        }
    }
}
