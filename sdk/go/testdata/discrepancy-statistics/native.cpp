#include <ql/math/randomnumbers/mt19937uniformrng.hpp>
#include <ql/math/randomnumbers/sobolrsg.hpp>
#include <ql/math/statistics/discrepancystatistics.hpp>
#include <iomanip>
#include <iostream>
#include <string>
#include <vector>

int main(int argc, char** argv) {
    std::cout << std::setprecision(17);
    if (argc > 1 && std::string(argv[1]) == "dimension-one") {
        bool rejected = false;
        try { QuantLib::DiscrepancyStatistics statistics(1); }
        catch (const std::exception&) { rejected = true; }
        std::cout << "{\"rejected\":" << (rejected ? "true" : "false") << "}\n";
        return 0;
    }
    std::string mode;
    std::size_t dimension, count;
    unsigned long seed;
    std::cin >> mode >> dimension >> count >> seed;
    std::vector<std::vector<double>> samples;
    if (mode == "sobol") {
        QuantLib::SobolRsg rng(dimension, seed, QuantLib::SobolRsg::Jaeckel);
        for (std::size_t i = 0; i < count; ++i) samples.push_back(rng.nextSequence().value);
    } else if (mode == "mt19937") {
        QuantLib::MersenneTwisterUniformRng rng(seed);
        for (std::size_t i = 0; i < count; ++i) {
            std::vector<double> point(dimension);
            for (double& coordinate : point) coordinate = rng.next().value;
            samples.push_back(point);
        }
    } else {
        for (std::size_t i = 0; i < count; ++i) {
            std::vector<double> point(dimension);
            for (double& coordinate : point) std::cin >> coordinate;
            samples.push_back(point);
        }
    }
    QuantLib::DiscrepancyStatistics statistics(dimension);
    for (const auto& point : samples) statistics.add(point);
    std::cout << "{\"discrepancy\":" << statistics.discrepancy() << ",\"samples\":[";
    for (std::size_t i = 0; i < count; ++i) {
        if (i) std::cout << ',';
        std::cout << '[';
        for (std::size_t j = 0; j < dimension; ++j) {
            if (j) std::cout << ',';
            std::cout << samples[i][j];
        }
        std::cout << ']';
    }
    std::cout << "]}\n";
}
