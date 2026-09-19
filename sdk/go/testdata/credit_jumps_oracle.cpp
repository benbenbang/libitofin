#include <ql/termstructures/credit/hazardratestructure.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/time/calendars/nullcalendar.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <ql/settings.hpp>
#include <cmath>
#include <iomanip>
#include <iostream>

using namespace QuantLib;

class JumpCurve : public HazardRateStructure {
  public:
    JumpCurve(Date reference, const std::vector<Handle<Quote>>& jumps,
              const std::vector<Date>& dates)
    : HazardRateStructure(reference, NullCalendar(), Actual365Fixed(), jumps, dates) {}
    explicit JumpCurve(const std::vector<Handle<Quote>>& jumps)
    : HazardRateStructure(0, NullCalendar(), Actual365Fixed(), jumps) {}
    Date maxDate() const override { return Date::maxDate(); }
  protected:
    Real hazardRateImpl(Time) const override { return 0.02; }
    Real survivalProbabilityImpl(Time t) const override { return std::exp(-0.02 * t); }
};

void row(const char* scenario, const JumpCurve& curve, Time t) {
    std::cout << scenario << ',' << t << ',' << curve.survivalProbability(t)
              << ',' << curve.defaultProbability(t) << ',' << curve.defaultDensity(t)
              << ',' << curve.hazardRate(t) << '\n';
}

int main() {
    Date reference(15, June, 2026);
    Settings::instance().evaluationDate() = reference;
    auto first = ext::make_shared<SimpleQuote>(0.9);
    std::vector<Handle<Quote>> jumps{Handle<Quote>(first),
        Handle<Quote>(ext::make_shared<SimpleQuote>(0.8))};
    JumpCurve explicitDates(reference, jumps, {reference + 100, reference + 200});
    std::cout << std::setprecision(17);
    std::cout << "scenario,time,survival,default,density,hazard\n";
    for (Integer days : {0, 99, 100, 101, 200, 201, 730})
        row("explicit", explicitDates, days / 365.0);
    first->setValue(0.85);
    row("quote_changed", explicitDates, 1.0);
    first->setValue(0.9);
    JumpCurve moving(jumps);
    row("moving_initial", moving, 1.0);
    Settings::instance().evaluationDate() = Date(2, January, 2027);
    for (Time t : {0.0, 363.0 / 365.0, 1.0})
        row("moving_shifted", moving, t);
}
