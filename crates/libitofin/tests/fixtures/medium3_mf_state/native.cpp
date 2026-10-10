#include <ql/processes/mfstateprocess.hpp>
#include <iomanip>
#include <iostream>

int main() {
    using namespace QuantLib;
    std::cout << "a\tt0\tdt\tdiffusion\tvariance\tstddev\tempty\n";
    std::cout << std::setprecision(17);
    for (double a : {0.0, 0.07, -0.07}) {
        MfStateProcess p(a, Array{0.5, 1.5}, Array{0.2, 0.4, 0.1});
        for (double t : {0.0, 0.5, 1.5, 2.0}) {
            for (double dt : {0.0, 0.25, 1.0, 2.0}) {
                std::cout << a << '\t' << t << '\t' << dt << '\t'
                          << p.diffusion(t, 7.0) << '\t' << p.variance(t, 7.0, dt)
                          << '\t' << p.stdDeviation(t, 7.0, dt) << "\t0\n";
            }
        }
        MfStateProcess empty(a, Array{}, Array{0.2});
        std::cout << a << "\t0.5\t2\t" << empty.diffusion(0.5, 7.0) << '\t'
                  << empty.variance(0.5, 7.0, 2.0) << '\t'
                  << empty.stdDeviation(0.5, 7.0, 2.0) << "\t1\n";
    }
}
