#include <ql/processes/geometricbrownianprocess.hpp>
#include <iomanip>
#include <iostream>

int main() {
    using namespace QuantLib;
    Real initial, mu, sigma, t, x, dt, dw;
    std::cin >> initial >> mu >> sigma >> t >> x >> dt >> dw;
    GeometricBrownianMotionProcess process(initial, mu, sigma);
    std::cout << std::setprecision(17)
              << "{\"x0\":" << process.x0()
              << ",\"drift\":" << process.drift(t, x)
              << ",\"diffusion\":" << process.diffusion(t, x)
              << ",\"expectation\":" << process.expectation(t, x, dt)
              << ",\"variance\":" << process.variance(t, x, dt)
              << ",\"std_deviation\":" << process.stdDeviation(t, x, dt)
              << ",\"evolve\":" << process.evolve(t, x, dt, dw) << "}\n";
}
