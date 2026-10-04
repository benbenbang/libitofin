#include <ql/math/statistics/convergencestatistics.hpp>
#include <ql/math/statistics/generalstatistics.hpp>
#include <cmath>
#include <iomanip>
#include <iostream>
#include <string>

int main(int argc, char** argv) {
    using Statistics = QuantLib::ConvergenceStatistics<QuantLib::GeneralStatistics>;
    Statistics statistics;
    std::cout << std::setprecision(17);
    if (argc > 1 && std::string(argv[1]) == "zero-first") {
        bool rejected = false;
        try { statistics.add(7.0, 0.0); } catch (const std::exception&) { rejected = true; }
        std::cout << "{\"rejected\":" << (rejected ? "true" : "false")
                  << ",\"samples_after_error\":" << statistics.samples()
                  << ",\"table_size_after_error\":" << statistics.convergenceTable().size()
                  << ",\"checkpoint_mean_is_nan\":"
                  << (std::isnan(statistics.convergenceTable().at(0).second) ? "true" : "false") << "}\n";
        return 0;
    }
    std::size_t count;
    std::cin >> count;
    for (std::size_t i = 0; i < count; ++i) {
        double value, weight;
        std::cin >> value >> weight;
        statistics.add(value, weight);
    }
    std::cout << "{\"samples\":" << statistics.samples()
              << ",\"weight_sum\":" << statistics.weightSum() << ",\"mean\":";
    if (count) std::cout << statistics.mean(); else std::cout << "null";
    std::cout << ",\"table\":[";
    bool first = true;
    for (const auto& point : statistics.convergenceTable()) {
        if (!first) std::cout << ',';
        first = false;
        std::cout << "{\"samples\":" << point.first << ",\"mean\":" << point.second << '}';
    }
    std::cout << "]}\n";
}
