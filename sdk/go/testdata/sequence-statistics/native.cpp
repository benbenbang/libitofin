#include <ql/math/statistics/sequencestatistics.hpp>
#include <cmath>
#include <iomanip>
#include <iostream>

void emit(const std::vector<double>& values) {
    std::cout << '[';
    for (std::size_t i = 0; i < values.size(); ++i) {
        if (i) std::cout << ',';
        if (std::isfinite(values[i])) std::cout << values[i];
        else if (std::isnan(values[i])) std::cout << "\"nan\"";
        else std::cout << (values[i] > 0 ? "\"inf\"" : "\"-inf\"");
    }
    std::cout << ']';
}

void emit(const QuantLib::Matrix& values) {
    emit(std::vector<double>(values.begin(), values.end()));
}

int main() {
    std::size_t rows, columns;
    std::cin >> rows >> columns;
    QuantLib::SequenceStatistics statistics(columns);
    for (std::size_t row = 0; row < rows; ++row) {
        double weight;
        std::cin >> weight;
        std::vector<double> values(columns);
        for (double& value : values) std::cin >> value;
        statistics.add(values, weight);
    }
    std::cout << std::setprecision(17) << "{\"mean\":";
    emit(statistics.mean());
    std::cout << ",\"variance\":";
    emit(statistics.variance());
    std::cout << ",\"standard_deviation\":";
    emit(statistics.standardDeviation());
    std::cout << ",\"error_estimate\":";
    emit(statistics.errorEstimate());
    std::cout << ",\"minimum\":";
    emit(statistics.min());
    std::cout << ",\"maximum\":";
    emit(statistics.max());
    std::cout << ",\"covariance\":";
    emit(statistics.covariance());
    std::cout << ",\"correlation\":";
    emit(statistics.correlation());
    std::cout << "}\n";
}
