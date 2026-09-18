#include <ql/termstructures/inflation/interpolatedzeroinflationcurve.hpp>
#include <ql/termstructures/inflation/seasonality.hpp>
#include <ql/time/daycounters/actual365fixed.hpp>
#include <fstream>
#include <iostream>
#include <sstream>
#include <string>
#include <vector>

using namespace QuantLib;

std::vector<std::string> split(const std::string& text, char delimiter) {
    std::vector<std::string> fields;
    std::istringstream stream(text);
    for (std::string field; std::getline(stream, field, delimiter);)
        fields.push_back(field);
    return fields;
}

int main(int argc, char** argv) {
    if (argc != 2)
        return 2;
    std::ifstream input(argv[1]);
    if (!input)
        return 2;
    std::string line;
    std::getline(input, line);
    int failures = 0;
    while (std::getline(input, line)) {
        const auto fields = split(line, ',');
        std::vector<Rate> factors(std::stoul(fields.at(5)), 1.0);
        for (const auto& adjustment : split(fields.at(6), ';')) {
            const auto pair = split(adjustment, ':');
            factors.at(std::stoul(pair.at(0))) = std::stod(pair.at(1));
        }
        const Date base(1, Month(std::stoi(fields.at(4))), std::stoi(fields.at(3)));
        ZeroInflationCurve curve(Date(13, August, 2007),
                                 {base, Date(13, August, 2012)}, {0.02, 0.03},
                                 Frequency(std::stoi(fields.at(2))), Actual365Fixed());
        MultiplicativePriceSeasonality seasonality(
            Date(31, January, 2007), Frequency(std::stoi(fields.at(1))), factors);
        bool consistent;
        try {
            consistent = seasonality.isConsistent(curve);
        } catch (const Error& error) {
            if (std::string(error.what()).find("seasonality is inconsistent") == std::string::npos)
                throw;
            consistent = false;
        }
        const bool expected = fields.at(7) == "true";
        std::cout << fields.at(0) << ',' << std::boolalpha << consistent << '\n';
        failures += consistent != expected;
    }
    return failures ? 1 : 0;
}
