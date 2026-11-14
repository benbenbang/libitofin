// Boost 1.92 parity values for the nu = 4 Student-t quantile near p = 0.5,
// where Boost's exact closed form cancels. Build and run from the repo root:
//
//   c++ -std=c++17 -I"$(brew --prefix boost)/include" \
//       crates/libitofin/tests/fixtures/studentt_quantile/boost_nu4.cpp -o boost_nu4
//   ./boost_nu4
#include <boost/math/distributions/students_t.hpp>
#include <cstdio>

int main() {
    boost::math::students_t dist(4.0);
    for (double p : {0.499, 0.4999, 0.49999, 0.5 + 1.0e-7}) {
        std::printf("%.17g,%.17g\n", p, quantile(dist, p));
    }
}
