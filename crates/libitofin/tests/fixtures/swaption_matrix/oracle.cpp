#include <ql/quantlib.hpp>
#include <iomanip>
#include <iostream>

using namespace QuantLib;

int main() {
    const Date today(15, June, 2026);
    Settings::instance().evaluationDate() = today;
    const Calendar calendar = TARGET();
    const DayCounter dc = Actual365Fixed();
    const std::vector<Period> options = {1*Months, 6*Months, 1*Years, 5*Years, 10*Years, 30*Years};
    const std::vector<Period> swaps = {1*Years, 5*Years, 10*Years, 30*Years};
    const Real data[6][4] = {
        {0.1300, 0.1560, 0.1390, 0.1220}, {0.1440, 0.1580, 0.1460, 0.1260},
        {0.1600, 0.1590, 0.1470, 0.1290}, {0.1640, 0.1470, 0.1370, 0.1220},
        {0.1400, 0.1300, 0.1250, 0.1100}, {0.1130, 0.1090, 0.1070, 0.0930}
    };
    Matrix matrix(6, 4);
    std::vector<std::vector<Handle<Quote>>> quotes(6, std::vector<Handle<Quote>>(4));
    RelinkableHandle<Quote> first(ext::make_shared<SimpleQuote>(data[0][0]));
    std::vector<Date> dates;
    for (Size i = 0; i < 6; ++i) {
        dates.push_back(calendar.advance(today, options[i], ModifiedFollowing));
        for (Size j = 0; j < 4; ++j) {
            matrix[i][j] = data[i][j];
            quotes[i][j] = Handle<Quote>(ext::make_shared<SimpleQuote>(data[i][j]));
        }
    }
    quotes[0][0] = first;
    std::vector<ext::shared_ptr<SwaptionVolatilityMatrix>> surfaces = {
        ext::make_shared<SwaptionVolatilityMatrix>(calendar, ModifiedFollowing, options, swaps, quotes, dc),
        ext::make_shared<SwaptionVolatilityMatrix>(today, calendar, ModifiedFollowing, options, swaps, quotes, dc),
        ext::make_shared<SwaptionVolatilityMatrix>(calendar, ModifiedFollowing, options, swaps, matrix, dc),
        ext::make_shared<SwaptionVolatilityMatrix>(today, calendar, ModifiedFollowing, options, swaps, matrix, dc),
        ext::make_shared<SwaptionVolatilityMatrix>(today, calendar, ModifiedFollowing, dates, swaps, matrix, dc)
    };
    Handle<YieldTermStructure> curve(ext::make_shared<FlatForward>(today, 0.05, dc));
    std::cout << std::setprecision(17);
    std::cout << "kind,form,option,swap,vol,npv,recovered\n";
    for (Size form = 0; form < surfaces.size(); ++form) {
        auto surface = surfaces[form];
        auto engine = ext::make_shared<BlackSwaptionEngine>(curve, Handle<SwaptionVolatilityStructure>(surface));
        for (Size i = 0; i < 6; ++i) {
            for (Size j = 0; j < 4; ++j) {
                auto index = ext::make_shared<EuriborSwapIsdaFixA>(swaps[j], curve);
                Swaption swaption = MakeSwaption(index, options[i]).withPricingEngine(engine);
                Real npv = swaption.NPV();
                Real recovered = swaption.impliedVolatility(npv, curve, data[i][j]*0.98, 1e-6, 100, 1e-6, 4.0);
                std::cout << "node," << form << ',' << i << ',' << j << ','
                          << surface->volatility(dates[i], swaps[j], 0.05) << ',' << npv << ',' << recovered << '\n';
            }
        }
    }
    for (Size form = 0; form < surfaces.size(); ++form) {
        auto surface = surfaces[form];
        Real initial = surface->volatility(dates[0], swaps[0], 0.02);
        Settings::instance().evaluationDate() = today - 1*Years;
        Real moved = surface->volatility(dates[0], swaps[0], 0.02);
        Settings::instance().evaluationDate() = today;
        auto old = ext::dynamic_pointer_cast<SimpleQuote>(first.currentLink());
        old->setValue(0.2);
        Real bumped = surface->volatility(dates[0], swaps[0], 0.02);
        first.linkTo(ext::make_shared<SimpleQuote>(0.3));
        Real relinked = surface->volatility(dates[0], swaps[0], 0.02);
        first.linkTo(ext::make_shared<SimpleQuote>(0.13));
        std::cout << "observe," << form << ',' << initial << ',' << moved << ',' << bumped << ',' << relinked << ",0\n";
    }
}
