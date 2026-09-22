#include <ql/quantlib.hpp>
#include <iostream>
#include <iomanip>
using namespace QuantLib;
extern "C" void bma_oracle() {
 Settings::instance().evaluationDate()=Date(23,October,2025);
 Date today=Settings::instance().evaluationDate();
 auto bma=ext::make_shared<BMAIndex>();
 auto cal=JointCalendar(bma->fixingCalendar(),USDLibor(3*Months).fixingCalendar(),JoinHolidays);
 auto settlement=cal.advance(today,2,Days);
 Handle<YieldTermStructure> risk(ext::make_shared<FlatForward>(settlement,0.04,Actual360()));
 auto ibor=ext::make_shared<USDLibor>(3*Months,risk);
 bma->addFixing(Date(22,October,2025),.03);
 std::vector<int> years{1,2,3,4,5,7,10,15,20,30};
 std::vector<double> fractions{.6756,.68,.6825,.685,.6881,.695,.7044,.7169,.7269,.7381};
 std::vector<ext::shared_ptr<RateHelper>> helpers;
 for(size_t i=0;i<years.size();++i) helpers.push_back(ext::make_shared<BMASwapRateHelper>(Handle<Quote>(ext::make_shared<SimpleQuote>(fractions[i])),Period(years[i],Years),2,cal,3*Months,Following,ActualActual(ActualActual::ISDA),bma,ibor));
 auto curve=ext::make_shared<PiecewiseYieldCurve<Discount,LogLinear>>(today,helpers,Actual360());
 auto fitted=ext::make_shared<BMAIndex>(Handle<YieldTermStructure>(curve));
 std::cout<<std::setprecision(17)<<"{\"quantlib\":\""<<QL_VERSION<<"\",\"today\":"<<today.serialNumber()<<",\"settlement\":"<<settlement.serialNumber()<<",\"nodes\":[";
 for(size_t i=0;i<years.size();++i) {
 auto maturity=settlement+Period(years[i],Years);
 Schedule bs=MakeSchedule().from(settlement).to(maturity).withTenor(3*Months).withCalendar(fitted->fixingCalendar()).withConvention(Following).backwards();
 Schedule ls=MakeSchedule().from(settlement).to(maturity).withTenor(3*Months).withCalendar(ibor->fixingCalendar()).withConvention(ibor->businessDayConvention()).endOfMonth(ibor->endOfMonth()).backwards();
 BMASwap swap(Swap::Payer,100,ls,.75,0,ibor,ibor->dayCounter(),bs,fitted,ActualActual(ActualActual::ISDA));
 swap.setPricingEngine(ext::make_shared<DiscountingSwapEngine>(risk));
 if(i) std::cout<<",";
 std::cout<<"{\"years\":"<<years[i]<<",\"fraction\":"<<fractions[i]<<",\"pillar\":"<<helpers[i]->pillarDate().serialNumber()<<",\"discount\":"<<curve->discount(helpers[i]->pillarDate())<<",\"fair_fraction\":"<<swap.fairLiborFraction()<<",\"fair_spread\":"<<swap.fairLiborSpread()<<",\"npv\":"<<swap.NPV()<<",\"libor_npv\":"<<swap.liborLegNPV()<<",\"bma_npv\":"<<swap.bmaLegNPV()<<",\"libor_bps\":"<<swap.liborLegBPS()<<",\"bma_bps\":"<<swap.bmaLegBPS()<<"}";
 }
 AverageBMACoupon coupon(Date(27,January,2026),100,settlement,Date(27,January,2026),fitted,1.2,.001,Date(),Date(),Actual360());
 std::cout<<"],\"coupon\":{\"rate\":"<<coupon.rate()<<",\"amount\":"<<coupon.amount()<<",\"fixing_dates\":[";
 auto dates=coupon.fixingDates();for(size_t i=0;i<dates.size();++i){if(i)std::cout<<",";std::cout<<dates[i].serialNumber();}
 std::cout<<"]},\"holiday_fixings\":[";
 dates=bma->fixingSchedule(Date(20,December,2024),Date(8,January,2025)).dates();for(size_t i=0;i<dates.size();++i){if(i)std::cout<<",";std::cout<<dates[i].serialNumber();}
 std::cout<<"]}";
}
