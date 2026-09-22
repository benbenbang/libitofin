#include <ql/pricingengines/vanilla/coshestonengine.hpp>
#include <ql/pricingengines/vanilla/exponentialfittinghestonengine.hpp>
#include <ql/termstructures/yield/flatforward.hpp>
#include <ql/quotes/simplequote.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <ql/time/period.hpp>
#include <ql/version.hpp>
#include <stdexcept>
#include <string>
#include <iomanip>
#include <iostream>
using namespace QuantLib;
int main() {
    if (std::string(QL_VERSION).rfind("1.43",0) != 0) throw std::runtime_error("QuantLib 1.43 required");
    std::cout << std::setprecision(17);
    Date ref(7, February, 2017);
    Settings::instance().evaluationDate() = ref;
    Handle<YieldTermStructure> r(ext::make_shared<FlatForward>(ref,0.15,Actual365Fixed()));
    Handle<YieldTermStructure> q(ext::make_shared<FlatForward>(ref,0.075,Actual365Fixed()));
    Handle<Quote> s(ext::make_shared<SimpleQuote>(100));
    auto model = ext::make_shared<HestonModel>(ext::make_shared<HestonProcess>(r,q,s,0.1,4.0,0.25,0.4,-0.75));
    COSHestonEngine cos(model);
    for (double t=0.01;t<41;t*=2) {
        std::cout << "c," << t << ',' << cos.c1(t) << ',' << cos.c2(t) << ',' << cos.c3(t) << ',' << cos.c4(t) << '\n';
        for (double u : {0.01,0.1,1.0,3.0,10.0,50.0}) {
            auto f = cos.chF(u,t);
            std::cout << "f," << t << ',' << u << ',' << f.real() << ',' << f.imag() << '\n';
        }
    }
    AnalyticHestonEngine::ComplexLogFormula cvs[] = {AnalyticHestonEngine::OptimalCV,AnalyticHestonEngine::AndersenPiterbarg,AnalyticHestonEngine::AndersenPiterbargOptCV,AnalyticHestonEngine::AsymptoticChF,AnalyticHestonEngine::AngledContour,AnalyticHestonEngine::AngledContourNoCV};
    for (Size i=0;i<6;++i) for (double alpha : {-0.5,-0.3,0.0,-1.0}) {
        if (i==3 && alpha != -0.5) continue;
        for (double scaling : {double(Null<Real>()),0.75}) {
            auto priceModel = i==3 ? ext::make_shared<HestonModel>(ext::make_shared<HestonProcess>(r,q,s,0.01,0.5,0.01,2.0,0.0)) : model;
            VanillaOption option(ext::make_shared<PlainVanillaPayoff>(Option::Call,120),ext::make_shared<EuropeanExercise>(ref+365));
            option.setPricingEngine(ext::make_shared<ExponentialFittingHestonEngine>(priceModel,cvs[i],scaling,alpha));
            std::cout << "p," << i << ',' << alpha << ',' << (scaling==Null<Real>() ? -1 : scaling) << ',' << option.NPV() << '\n';
        }
    }
}
