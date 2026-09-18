#include <ql/quantlib.hpp>
#include <ql/termstructures/globalbootstrap.hpp>
#include <ql/termstructures/globalbootstrapvars.hpp>
#include <iomanip>
#include <iostream>

using namespace QuantLib;

int main() {
    Settings::instance().evaluationDate() = Date(15, June, 2026);
    const Date reference(17, June, 2026), extraDate(17, August, 2026);
    auto shortQuote = ext::make_shared<SimpleQuote>(0.04);
    auto longQuote = ext::make_shared<SimpleQuote>(0.045);
    auto convexity = ext::make_shared<SimpleQuote>(0.01);
    auto shortHelper = ext::make_shared<DepositRateHelper>(
        Handle<Quote>(shortQuote), ext::make_shared<Euribor1M>());
    auto additional = ext::make_shared<DepositRateHelper>(
        Handle<Quote>(longQuote), ext::make_shared<Euribor3M>());
    auto future = ext::make_shared<FuturesRateHelper>(
        Handle<Quote>(ext::make_shared<SimpleQuote>(95.0)), reference,
        Date(17, September, 2026), Actual360(), Handle<Quote>(convexity, false), Futures::Custom);
    using Curve = PiecewiseYieldCurve<Discount, LogLinear, GlobalBootstrap>;
    Curve::bootstrap_type bootstrap(
        {additional}, [=] { return std::vector<Date>{extraDate}; },
        [=](const std::vector<Time>& times, const std::vector<Real>& data) {
            QL_REQUIRE(times.size() == 4, "unexpected grid");
            return Array{10000.0 * additional->quoteError(), data[2] - 0.99};
        }, 1.0e-12, nullptr, nullptr,
        ext::make_shared<SimpleQuoteVariables>(
            std::vector<ext::shared_ptr<SimpleQuote>>{convexity},
            std::vector<Real>{0.01}, std::vector<Real>{0.0}));
    Curve curve(reference, {shortHelper, future}, Actual365Fixed(), LogLinear(), bootstrap);
    std::cout << "long_quote,july_discount,august_discount,september_discount,convexity\n";
    std::cout << std::setprecision(17);
    for (double quote : {0.045, 0.042}) {
        longQuote->setValue(quote);
        std::cout << quote << ',' << curve.discount(Date(17, July, 2026)) << ','
                  << curve.discount(extraDate) << ',' << curve.discount(Date(17, September, 2026))
                  << ',' << convexity->value() << '\n';
    }
}
