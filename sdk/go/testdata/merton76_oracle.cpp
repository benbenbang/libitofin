#include <ql/processes/merton76process.hpp>
#include <ql/pricingengines/vanilla/jumpdiffusionengine.hpp>
#include <ql/instruments/vanillaoption.hpp>
#include <ql/exercise.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/termstructures/yield/flatforward.hpp>
#include <ql/termstructures/volatility/equityfx/blackconstantvol.hpp>
#include <ql/time/daycounters/actual360.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <ql/time/calendars/nullcalendar.hpp>
#include <ql/settings.hpp>
#include <cstdio>
extern "C" int merton(int call, int clock, double spot, double strike, double q, double r, double vol, double intensity, double mean, double jumpvol, int days, double accuracy, unsigned long limit, double *out, char *error) {
    using namespace QuantLib;
    try {
        Date today(2,October,2026);
        Settings::instance().evaluationDate()=today;
        DayCounter dc=Actual360();
        DayCounter ratedc=clock?DayCounter(Actual365Fixed()):dc;
        DayCounter divdc=clock?DayCounter(Actual365Fixed()):dc;
        Date volref=clock?today-30:today;
        auto quote=[](double v){return Handle<Quote>(ext::make_shared<SimpleQuote>(v));};
        auto process=ext::make_shared<Merton76Process>(quote(spot),Handle<YieldTermStructure>(ext::make_shared<FlatForward>(today,q,divdc)),Handle<YieldTermStructure>(ext::make_shared<FlatForward>(today,r,ratedc)),Handle<BlackVolTermStructure>(ext::make_shared<BlackConstantVol>(volref,NullCalendar(),vol,dc)),quote(intensity),quote(mean),quote(jumpvol));
        VanillaOption option(ext::make_shared<PlainVanillaPayoff>(call?Option::Call:Option::Put,strike),ext::make_shared<EuropeanExercise>(today+days));
        option.setPricingEngine(ext::make_shared<JumpDiffusionEngine>(process,accuracy,limit));
        out[0]=option.NPV(); out[1]=option.delta(); out[2]=option.gamma(); out[3]=option.theta(); out[4]=option.vega(); out[5]=option.rho(); out[6]=option.dividendRho();
        return 0;
    } catch(const std::exception& e) {std::snprintf(error,4096,"%s",e.what());return 1;}
}
