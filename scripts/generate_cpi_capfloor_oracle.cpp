#include <ql/cashflows/cpicoupon.hpp>
#include <ql/cashflows/cpicouponpricer.hpp>
#include <ql/experimental/inflation/cpicapfloorengines.hpp>
#include <ql/experimental/inflation/cpicapfloortermpricesurface.hpp>
#include <ql/indexes/inflation/ukrpi.hpp>
#include <ql/math/interpolations/bilinearinterpolation.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/settings.hpp>
#include <ql/termstructures/inflation/interpolatedzeroinflationcurve.hpp>
#include <ql/termstructures/yield/flatforward.hpp>
#include <ql/time/calendars/nullcalendar.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <fstream>
#include <iomanip>
#include <iostream>

using namespace QuantLib;

extern "C" int run_oracle(const char* output) {
    try {
        const Date today(15, June, 2025);
        Settings::instance().evaluationDate() = today;
        const Actual365Fixed dc;
        const std::vector<Date> dates{Date(1, January, 2025), Date(1, January, 2035)};
        auto inflation = ext::make_shared<InterpolatedZeroInflationCurve<Linear>>(
            today, dates, std::vector<Rate>{.025, .025}, Monthly, dc);
        RelinkableHandle<ZeroInflationTermStructure> inflationHandle(inflation);
        auto index = ext::make_shared<UKRPI>(inflationHandle);
        const std::vector<Date> fixingDates{Date(1, October, 2024), Date(1, November, 2024), Date(1, December, 2024),
            Date(1, January, 2025), Date(1, February, 2025), Date(1, March, 2025), Date(1, April, 2025), Date(1, May, 2025)};
        const std::vector<Real> fixings{99.5, 99.8, 100.0, 100.2, 100.4, 100.6, 100.8, 101.0};
        for (Size i = 0; i < fixings.size(); ++i) index->addFixing(fixingDates[i], fixings[i], true);
        auto rate = ext::make_shared<SimpleQuote>(.04);
        Handle<YieldTermStructure> yield(ext::make_shared<FlatForward>(today, Handle<Quote>(rate), dc));
        CPICoupon coupon(100.0, Date(15, July, 2025), 100000.0, Date(15, January, 2025),
            Date(15, July, 2025), index, Period(3, Months), CPI::Flat, dc, .02);
        coupon.setPricer(ext::make_shared<CPICouponPricer>());
        std::ofstream out(output);
        out << std::setprecision(17);
        out << "name\tvalue\n";
        out << "sanity_discount\t" << yield->discount(today + 365) << '\n';
        out << "sanity_index_forecast\t" << index->fixing(Date(1, July, 2025)) << '\n';
        out << "sanity_coupon_amount\t" << coupon.amount() << '\n';
        Matrix caps(2, 3), floors(2, 3);
        caps[0][0] = .08; caps[0][1] = .15; caps[0][2] = .24;
        caps[1][0] = .04; caps[1][1] = .10; caps[1][2] = .16;
        floors[0][0] = .02; floors[0][1] = .04; floors[0][2] = .06;
        floors[1][0] = .035; floors[1][1] = .065; floors[1][2] = .095;
        const std::vector<Rate> capStrikes{.03, .04}, floorStrikes{.01, .02};
        const std::vector<Period> tenors{Period(1, Years), Period(3, Years), Period(5, Years)};
        auto surface = ext::make_shared<InterpolatedCPICapFloorTermPriceSurface<Bilinear>>(
            100000.0, .025, Period(3, Months), NullCalendar(), Unadjusted, dc,
            index, CPI::Flat, yield, capStrikes, floorStrikes, tenors, caps, floors);
        auto engine = ext::make_shared<InterpolatingCPICapFloorEngine>(Handle<CPICapFloorTermPriceSurface>(surface));
        for (int year : {1, 2, 3, 5, 7}) {
            const Date maturity = today + Period(year, Years);
            for (Real strike : {.005, .01, .015, .02, .025, .03, .035, .04, .05}) {
                out << "cap_" << year << '_' << strike << '\t' << surface->capPrice(maturity, strike) << '\n';
                out << "floor_" << year << '_' << strike << '\t' << surface->floorPrice(maturity, strike) << '\n';
            }
        }
        auto optionValue = [&](Option::Type type, Real nominal, Real baseCPI, Date start, Period lag,
                               CPI::InterpolationType interpolation) {
            CPICapFloor option(type, nominal, start, baseCPI, today + Period(2, Years),
                NullCalendar(), Unadjusted, NullCalendar(), Unadjusted, .025, index, lag, interpolation);
            option.setPricingEngine(engine);
            return option.NPV();
        };
        for (auto interpolation : {CPI::Flat, CPI::Linear}) {
            const std::string label = interpolation == CPI::Flat ? "flat" : "linear";
            for (int lag : {3, 5}) {
                out << label << "_cap_lag" << lag << '\t' << optionValue(Option::Call, 100000.0, 100.0, today, Period(lag, Months), interpolation) << '\n';
                out << label << "_floor_lag" << lag << '\t' << optionValue(Option::Put, 100000.0, 100.0, today, Period(lag, Months), interpolation) << '\n';
            }
            out << label << "_nominal_changed\t" << optionValue(Option::Call, 17.0, 100.0, today, Period(3, Months), interpolation) << '\n';
            out << label << "_base_changed\t" << optionValue(Option::Call, 100000.0, 321.0, today, Period(3, Months), interpolation) << '\n';
            out << label << "_start_changed\t" << optionValue(Option::Call, 100000.0, 100.0, today - Period(1, Years), Period(3, Months), interpolation) << '\n';
        }
        const Date query = today + Period(2, Years);
        auto differentNominal = ext::make_shared<InterpolatedCPICapFloorTermPriceSurface<Bilinear>>(
            7.0, .025, Period(3, Months), NullCalendar(), Unadjusted, dc,
            index, CPI::Flat, yield, capStrikes, floorStrikes, tenors, caps, floors);
        out << "surface_nominal_changed_cap\t" << differentNominal->capPrice(query, .015) << '\n';
        const std::vector<Period> monthTenors{Period(12, Months), Period(36, Months), Period(60, Months)};
        auto months = ext::make_shared<InterpolatedCPICapFloorTermPriceSurface<Bilinear>>(
            100000.0, .025, Period(3, Months), NullCalendar(), Unadjusted, dc,
            index, CPI::Flat, yield, capStrikes, floorStrikes, monthTenors, caps, floors);
        out << "month_tenor_power_quirk_cap\t" << months->capPrice(query, .015) << '\n';
        out << "snapshot_cap_initial\t" << surface->capPrice(query, .015) << '\n';
        out << "snapshot_floor_initial\t" << surface->floorPrice(query, .035) << '\n';
        rate->setValue(.08);
        out << "snapshot_cap_discount_updated\t" << surface->capPrice(query, .015) << '\n';
        inflationHandle.linkTo(ext::make_shared<InterpolatedZeroInflationCurve<Linear>>(
            today, dates, std::vector<Rate>{.05, .05}, Monthly, dc));
        out << "snapshot_cap_inflation_updated\t" << surface->capPrice(query, .015) << '\n';
        out << "dynamic_atm_after_update\t" << surface->atmRate(query) << '\n';
        auto fresh = ext::make_shared<InterpolatedCPICapFloorTermPriceSurface<Bilinear>>(
            100000.0, .025, Period(3, Months), NullCalendar(), Unadjusted, dc,
            index, CPI::Flat, yield, capStrikes, floorStrikes, tenors, caps, floors);
        out << "fresh_cap_after_update\t" << fresh->capPrice(query, .015) << '\n';
        try {
            optionValue(Option::Call, 100000.0, 100.0, today, Period(1, Months), CPI::Flat);
            out << "negative_lag_difference_rejected\t0\n";
        } catch (const std::exception&) {
            out << "negative_lag_difference_rejected\t1\n";
        }
        Settings::instance().evaluationDate() = today + 1;
        out << "surface_reference_after_roll\t" << surface->referenceDate().serialNumber() << '\n';
        out << "surface_cap_after_roll\t" << surface->capPrice(query, .015) << '\n';
        Settings::instance().evaluationDate() = today + Period(2, Years);
        out << "at_maturity_npv\t" << optionValue(Option::Call, 100000.0, 100.0, today, Period(3, Months), CPI::Flat) << '\n';
        Settings::instance().evaluationDate() = today + Period(2, Years) + 1;
        out << "after_maturity_npv\t" << optionValue(Option::Call, 100000.0, 100.0, today, Period(3, Months), CPI::Flat) << '\n';
        return 0;
    } catch (const std::exception& e) {
        std::cerr << e.what() << '\n';
        return 1;
    }
}
