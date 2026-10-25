#include <ql/cashflows/cashflows.hpp>
#include <ql/cashflows/cpicoupon.hpp>
#include <ql/cashflows/cpicouponpricer.hpp>
#include <ql/cashflows/fixedratecoupon.hpp>
#include <ql/indexes/inflation/ukrpi.hpp>
#include <ql/indexes/indexmanager.hpp>
#include <ql/settings.hpp>
#include <ql/termstructures/inflation/interpolatedzeroinflationcurve.hpp>
#include <ql/termstructures/yield/flatforward.hpp>
#include <ql/time/calendars/nullcalendar.hpp>
#include <ql/time/calendars/weekendsonly.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <fstream>
#include <iomanip>
#include <iostream>

using namespace QuantLib;

extern "C" int run_oracle(const char* flowOutput, const char* npvOutput) {
    try {
        const Date today(18, August, 2025);
        const Actual365Fixed dc;
        Settings::instance().evaluationDate() = today;
        Settings::instance().includeTodaysCashFlows() = ext::nullopt;
        Settings::instance().includeReferenceDateEvents() = false;
        std::ofstream flows(flowOutput), npvs(npvOutput);
        flows << std::setprecision(17);
        npvs << std::setprecision(17);
        flows << "case\tinterpolation\tlag_months\tgrowth_only\tdefault_base\tirregular\tstage\tflow\tdate\tamount\tbase_kind\tbase_fixing\n";
        npvs << "case\tstage\tsettlement\tinclude\tnpv\n";
        const auto discount = ext::make_shared<FlatForward>(Date(1, January, 2024), -.015, dc);
        Size caseId = 0;
        for (auto interpolation : {CPI::Flat, CPI::Linear})
        for (int lag : {1, 3})
        for (bool growth : {false, true})
        for (bool defaultBase : {false, true})
        for (bool irregular : {false, true}) {
            IndexManager::instance().clearHistories();
            const std::vector<Date> curveDates{Date(1, January, 2025), Date(1, January, 2035)};
            RelinkableHandle<ZeroInflationTermStructure> inflation;
            inflation.linkTo(ext::make_shared<InterpolatedZeroInflationCurve<Linear>>(
                today, curveDates, std::vector<Rate>{-.0075, -.0075}, Monthly, dc));
            auto index = ext::make_shared<UKRPI>(inflation);
            for (int year = 2022; year <= 2025; ++year)
            for (int month = 1; month <= 12 && (year < 2025 || month <= 7); ++month) {
                Real value = 208.0 + (year - 2022) * 9.1 + month * .73 + (month % 3) * .17;
                index->addFixing(Date(1, Month(month), year), value, true);
            }
            const Period observationLag(lag, Months);
            const Date start = irregular ? Date(17, February, 2024) : Date(31, January, 2024);
            const std::vector<Date> dates{start, Date(31, July, 2024), Date(31, January, 2025),
                Date(31, July, 2025), Date(31, January, 2026)};
            Schedule schedule(dates, NullCalendar(), Unadjusted, Unadjusted, Period(6, Months),
                DateGeneration::Backward, true, std::vector<bool>{!irregular, true, true, true});
            CPILeg builder(schedule, index, Null<Real>(), observationLag);
            builder.withNotionals(std::vector<Real>{8123.75, 7400.0, 7100.0, 6900.0, 6500.0})
                .withFixedRates(std::vector<Rate>{-.0125, 0.0, .0375})
                .withPaymentDayCounter(dc).withPaymentCalendar(WeekendsOnly())
                .withPaymentAdjustment(Following).withObservationInterpolation(interpolation)
                .withSubtractInflationNominal(growth)
                .withExCouponPeriod(Period(7, Days), NullCalendar(), Unadjusted);
            const Date explicitBase(31, January, 2024);
            if (!defaultBase) builder.withBaseDate(explicitBase);
            const Date base = defaultBase ? start - observationLag : explicitBase;
            Leg leg = builder;
            for (int stage = 0; stage < 3; ++stage) {
                if (stage == 1)
                    index->addFixing(Date(1, base.month(), base.year()), 180.25, true);
                if (stage == 2)
                    inflation.linkTo(ext::make_shared<InterpolatedZeroInflationCurve<Linear>>(
                        today, curveDates, std::vector<Rate>{.065, .065}, Monthly, dc));
                for (Size flow = 0; flow < leg.size(); ++flow) {
                    int kind = 0;
                    Real baseFixing = 0.0;
                    if (auto coupon = ext::dynamic_pointer_cast<CPICoupon>(leg[flow])) {
                        kind = 1;
                        baseFixing = CPI::laggedFixing(index, coupon->baseDate() + observationLag,
                            observationLag, interpolation);
                    } else if (auto principal = ext::dynamic_pointer_cast<CPICashFlow>(leg[flow])) {
                        kind = 2;
                        baseFixing = principal->baseFixing();
                    }
                    flows << caseId << '\t' << (interpolation == CPI::Flat ? "flat" : "linear")
                        << '\t' << lag << '\t' << growth << '\t' << defaultBase << '\t' << irregular
                        << '\t' << stage << '\t' << flow << '\t' << leg[flow]->date().serialNumber()
                        << '\t' << leg[flow]->amount() << '\t' << kind << '\t' << baseFixing << '\n';
                }
                for (Date settlement : {Date(31, July, 2025), today})
                for (bool include : {false, true})
                    npvs << caseId << '\t' << stage << '\t' << settlement.serialNumber() << '\t' << include
                        << '\t' << CashFlows::npv(leg, *discount, include, settlement, today) << '\n';
            }
            ++caseId;
        }
        return 0;
    } catch (const std::exception& e) {
        std::cerr << e.what() << '\n';
        return 1;
    }
}
