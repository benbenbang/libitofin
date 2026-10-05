#include <ql/pricingengines/forward/replicatingvarianceswapengine.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/termstructures/yield/flatforward.hpp>
#include <ql/termstructures/volatility/equityfx/blackconstantvol.hpp>
#include <ql/termstructures/volatility/equityfx/blackvariancesurface.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <ql/time/calendars/nullcalendar.hpp>
#include <ql/settings.hpp>
#include <iomanip>
#include <iostream>
#include <string>

using namespace QuantLib;

struct Market {
    ext::shared_ptr<SimpleQuote> spot, rate, dividend, vol;
    ext::shared_ptr<GeneralizedBlackScholesProcess> process;
};

void result(VarianceSwap& swap, const Market& market, const Date& maturity) {
    const Real variance = swap.variance();
    const Real npv = swap.NPV();
    const Real discount = market.process->riskFreeRate()->discount(maturity);
    const auto weights = swap.result<ReplicatingVarianceSwapEngine::weights_type>("optionWeights");
    const auto exercise = ext::make_shared<EuropeanExercise>(maturity);
    const auto engine = ext::make_shared<AnalyticEuropeanEngine>(market.process);
    std::cout << "{\"variance\":" << variance << ",\"npv\":" << npv
              << ",\"discount_factor\":" << discount << ",\"weights\":[";
    for (Size i = 0; i < weights.size(); ++i) {
        if (i != 0) std::cout << ',';
        EuropeanOption option(weights[i].first, exercise);
        option.setPricingEngine(engine);
        const Real price = option.NPV();
        std::cout << "{\"option_type\":\""
                  << (weights[i].first->optionType() == Option::Call ? "call" : "put")
                  << "\",\"strike\":" << weights[i].first->strike()
                  << ",\"weight\":" << weights[i].second
                  << ",\"option_price\":" << price
                  << ",\"weighted_price\":" << price * weights[i].second << '}';
    }
    std::cout << "]}";
}

int main() {
    try {
        Real spot, rate, dividend, vol, strike, notional, dk;
        Integer position, days;
        Size calls, puts, smileSize, updates;
        if (!(std::cin >> spot >> rate >> dividend >> vol >> strike >> notional
                      >> position >> days >> dk >> calls >> puts >> smileSize)) return 2;
        std::vector<Real> callStrikes(calls), putStrikes(puts), smileStrikes(smileSize);
        Matrix smileVols(smileSize, 1);
        for (auto& value : callStrikes) std::cin >> value;
        for (auto& value : putStrikes) std::cin >> value;
        for (Size i = 0; i < smileSize; ++i) std::cin >> smileStrikes[i] >> smileVols[i][0];
        const Date today(5, October, 2026), maturity = today + days;
        Settings::instance().evaluationDate() = today;
        const DayCounter dayCounter = Actual365Fixed();
        Market market{ext::make_shared<SimpleQuote>(spot), ext::make_shared<SimpleQuote>(rate),
                      ext::make_shared<SimpleQuote>(dividend), ext::make_shared<SimpleQuote>(vol), {}};
        const auto riskFree = ext::make_shared<FlatForward>(today, Handle<Quote>(market.rate), dayCounter);
        const auto dividends = ext::make_shared<FlatForward>(today, Handle<Quote>(market.dividend), dayCounter);
        ext::shared_ptr<BlackVolTermStructure> volatility;
        if (smileSize == 0) {
            volatility = ext::make_shared<BlackConstantVol>(today, NullCalendar(), Handle<Quote>(market.vol), dayCounter);
        } else {
            volatility = ext::make_shared<BlackVarianceSurface>(today, NullCalendar(),
                std::vector<Date>{maturity}, smileStrikes, smileVols, dayCounter);
        }
        market.process = ext::make_shared<BlackScholesMertonProcess>(Handle<Quote>(market.spot),
            Handle<YieldTermStructure>(dividends), Handle<YieldTermStructure>(riskFree),
            Handle<BlackVolTermStructure>(volatility));
        VarianceSwap swap(position == 1 ? Position::Long : Position::Short, strike, notional, today, maturity);
        swap.setPricingEngine(ext::make_shared<ReplicatingVarianceSwapEngine>(market.process, dk, callStrikes, putStrikes));
        std::cout << std::setprecision(17) << "{\"initial\":";
        result(swap, market, maturity);
        std::cout << ",\"updates\":[";
        std::cin >> updates;
        for (Size i = 0; i < updates; ++i) {
            std::string name;
            std::cin >> name >> spot >> rate >> dividend >> vol;
            market.spot->setValue(spot); market.rate->setValue(rate);
            market.dividend->setValue(dividend); market.vol->setValue(vol);
            if (i != 0) std::cout << ',';
            std::cout << "{\"name\":\"" << name << "\",\"cached_variance\":" << swap.variance()
                      << ",\"cached_npv\":" << swap.NPV() << ",\"recalculated\":";
            swap.recalculate();
            result(swap, market, maturity);
            std::cout << '}';
        }
        std::cout << "]}\n";
        return 0;
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
