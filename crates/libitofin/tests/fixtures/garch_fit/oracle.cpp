#include <ql/math/distributions/normaldistribution.hpp>
#include <ql/math/randomnumbers/inversecumulativerng.hpp>
#include <ql/math/randomnumbers/mt19937uniformrng.hpp>
#include <ql/models/volatility/garch.hpp>
#include <ql/time/date.hpp>

#include <cmath>
#include <exception>
#include <iostream>
#include <stdexcept>

extern "C" int garch_fit_oracle(double* returns, double* result) {
    try {
        using namespace QuantLib;
        using GaussianGenerator =
            InverseCumulativeRng<MersenneTwisterUniformRng, InverseCumulativeNormal>;

        Date date(7, July, 1962);
        TimeSeries<Volatility> series;
        Garch11 process(0.2, 0.3, 0.4);
        GaussianGenerator rng(MersenneTwisterUniformRng(48));
        Volatility last_return = 0.0;
        Real variance = 0.0;

        for (std::size_t i = 0; i < 50000; ++i, date += 1) {
            variance = process.forecast(last_return, variance);
            last_return = rng.next().value * std::sqrt(variance);
            series[date] = last_return;
            returns[i] = last_return;
        }

        Garch11 fitted(series);
        const auto calculated = fitted.calculate(series);
        const auto calculated_volatilities = calculated.values();
        const Real last_variance = calculated_volatilities[calculated_volatilities.size() - 2] *
                                   calculated_volatilities[calculated_volatilities.size() - 2];
        const Real next_volatility = calculated_volatilities.back();
        if (calculated.dates().back() != date ||
            std::fabs(fitted.forecast(last_return, last_variance) -
                      next_volatility * next_volatility) > 1e-12) {
            throw std::runtime_error("next variance does not match forecast");
        }
        result[0] = fitted.alpha();
        result[1] = fitted.beta();
        result[2] = fitted.omega();
        result[3] = fitted.logLikelihood();
        result[4] = next_volatility * next_volatility;
        return 0;
    } catch (const std::exception& error) {
        std::cerr << error.what() << '\n';
        return 1;
    }
}
