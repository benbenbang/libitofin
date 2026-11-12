#include <ql/math/randomnumbers/xoshiro256starstaruniformrng.hpp>
#include <ql/math/randomnumbers/zigguratgaussianrng.hpp>
#include <cstdint>
#include <cstring>
#include <iomanip>
#include <iostream>

using namespace QuantLib;

int main() {
    ZigguratGaussianRng<Xoshiro256StarStarUniformRng> rng(Xoshiro256StarStarUniformRng(42));
    std::cout << "index\tvalue\tbits\n";
    for (int k = 0; k <= 100; ++k) {
        Real x = rng.next().value;
        std::uint64_t bits;
        std::memcpy(&bits, &x, sizeof bits);
        std::cout << k << '\t' << std::setprecision(17) << x << "\t0x" << std::hex
                  << std::setw(16) << std::setfill('0') << bits << std::dec << std::setfill(' ')
                  << '\n';
    }
}
