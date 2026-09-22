#include <ql/methods/montecarlo/lsmbasissystem.hpp>
#include <iomanip>
#include <iostream>

int main() {
    std::cout << std::setprecision(17);
    for (int family = 0; family != 7; ++family) {
        auto basis = QuantLib::LsmBasisSystem::pathBasisSystem(
            3, static_cast<QuantLib::LsmBasisSystem::PolynomialType>(family));
        for (double x : {0.2, 0.5, 0.9}) {
            std::cout << family << ',' << x;
            for (const auto& f : basis) std::cout << ',' << f(x);
            std::cout << '\n';
        }
    }
}
