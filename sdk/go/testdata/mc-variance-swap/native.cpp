#include <ql/pricingengines/forward/mcvarianceswapengine.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/termstructures/yield/flatforward.hpp>
#include <ql/termstructures/volatility/equityfx/blackconstantvol.hpp>
#include <ql/termstructures/volatility/equityfx/blackvariancecurve.hpp>
#include <ql/time/calendars/nullcalendar.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <iomanip>
#include <iostream>

class BoundedLocalVol : public QuantLib::LocalVolTermStructure {
  public:
    explicit BoundedLocalVol(const QuantLib::Date& today)
    : LocalVolTermStructure(today, QuantLib::NullCalendar(), QuantLib::Following, QuantLib::Actual365Fixed()) {}
    QuantLib::Date maxDate() const override { return QuantLib::Date::maxDate(); }
    QuantLib::Real minStrike() const override { return 0; }
    QuantLib::Real maxStrike() const override { return QL_MAX_REAL; }
  protected:
    QuantLib::Volatility localVolImpl(QuantLib::Time, QuantLib::Real state) const override {
        return .15 + .1*state/(state+100);
    }
};

int main() {
    using namespace QuantLib;
    Real spot, rate, dividend, vol, strike, notional, tolerance;
    int curve, days, position;
    Size steps, perYear, samples, maxSamples;
    BigNatural seed;
    std::cin >> curve >> spot >> rate >> dividend >> vol >> strike >> notional
             >> position >> days >> steps >> perYear >> samples >> tolerance
             >> maxSamples >> seed;
    const Date today(6, October, 2026);
    Settings::instance().evaluationDate() = today;
    const Actual365Fixed dc;
    auto spotQuote = ext::make_shared<SimpleQuote>(spot);
    auto rateQuote = ext::make_shared<SimpleQuote>(rate);
    auto dividendQuote = ext::make_shared<SimpleQuote>(dividend);
    auto volQuote = ext::make_shared<SimpleQuote>(vol);
    Handle<Quote> spotHandle(spotQuote);
    Handle<YieldTermStructure> riskFree(ext::make_shared<FlatForward>(today, Handle<Quote>(rateQuote), dc));
    Handle<YieldTermStructure> dividends(ext::make_shared<FlatForward>(today, Handle<Quote>(dividendQuote), dc));
    Handle<BlackVolTermStructure> black;
    if (curve == 1) {
        black = Handle<BlackVolTermStructure>(ext::make_shared<BlackVarianceCurve>(
            today, std::vector<Date>{today+36, today+90}, std::vector<Volatility>{0.1, 0.2}, dc));
    } else {
        black = Handle<BlackVolTermStructure>(ext::make_shared<BlackConstantVol>(today, NullCalendar(), Handle<Quote>(volQuote), dc));
    }
    ext::shared_ptr<GeneralizedBlackScholesProcess> process;
    if (curve == 2) {
        Handle<LocalVolTermStructure> local(ext::make_shared<BoundedLocalVol>(today));
        process = ext::make_shared<GeneralizedBlackScholesProcess>(spotHandle, dividends, riskFree, black, local);
    } else {
        process = ext::make_shared<BlackScholesMertonProcess>(spotHandle, dividends, riskFree, black);
    }
    MakeMCVarianceSwapEngine<PseudoRandom> factory(process);
    if (steps) factory.withSteps(steps); else factory.withStepsPerYear(perYear);
    if (samples) factory.withSamples(samples); else factory.withAbsoluteTolerance(tolerance).withMaxSamples(maxSamples);
    factory.withSeed(seed);
    ext::shared_ptr<PricingEngine> pricingEngine = factory;
    auto engine = ext::dynamic_pointer_cast<MCVarianceSwapEngine<PseudoRandom>>(pricingEngine);
    VarianceSwap swap(position == 1 ? Position::Long : Position::Short, strike, notional, today, today+days);
    swap.setPricingEngine(pricingEngine);
    auto emit = [&]() {
        Real npv = swap.NPV();
        Size count = steps ? steps : std::max<Size>(static_cast<Size>(perYear*process->time(today+days)), 1);
        TimeGrid grid(process->time(today+days), count);
        std::cout << "{\"variance\":" << swap.variance()
                  << ",\"npv\":" << npv
                  << ",\"variance_error\":" << engine->sampleAccumulator().errorEstimate()
                  << ",\"error_estimate\":" << swap.errorEstimate()
                  << ",\"samples\":" << engine->sampleAccumulator().samples()
                  << ",\"path_steps\":" << count
                  << ",\"integration_intervals\":" << static_cast<Size>(grid.back()/grid.dt(0)) << "}";
    };
    std::cout << std::setprecision(17) << "{\"initial\":";
    emit();
    Size updates;
    std::cin >> updates;
    std::cout << ",\"updates\":[";
    for (Size i=0; i<updates; ++i) {
        std::cin >> spot >> rate >> dividend >> vol;
        spotQuote->setValue(spot); rateQuote->setValue(rate);
        dividendQuote->setValue(dividend); volQuote->setValue(vol);
        swap.recalculate();
        if (i) std::cout << ",";
        emit();
    }
    std::cout << "]}\n";
}
