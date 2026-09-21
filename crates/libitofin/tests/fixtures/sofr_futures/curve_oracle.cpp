#include <ql/quantlib.hpp>
#include <iomanip>
#include <iostream>
using namespace QuantLib;
struct Row { int year; Month month; Frequency frequency; double price; };
int main() {
    std::cout << "case,year,month,frequency,quote,start,end,discount,implied\n" << std::setprecision(17);
    for (bool june : {false, true}) {
        IndexManager::instance().clearHistories();
        Date today = june ? Date(27, June, 2024) : Date(26, October, 2018);
        Settings::instance().evaluationDate() = today;
        auto index = ext::make_shared<Sofr>();
        if (june) {
            for (int day : {18,20,21,24,25,26,27}) index->addFixing(Date(day, June, 2024), .02);
        } else {
            const std::pair<int, double> fixings[] = {
                {1,.0222},{2,.022},{3,.022},{4,.0218},{5,.0216},{9,.0215},
                {10,.0215},{11,.0217},{12,.0218},{15,.0221},{16,.0218},
                {17,.0218},{18,.0219},{19,.0219},{22,.0218},{23,.0217},{24,.0218},{25,.0219}};
            for (auto [day, rate] : fixings) index->addFixing(Date(day, October, 2018), rate);
        }
        std::vector<Row> rows = june ? std::vector<Row>{
            {2024,June,Quarterly,97.220},{2024,September,Quarterly,97.170},
            {2024,December,Quarterly,97.160},{2025,March,Quarterly,97.165},
            {2025,June,Quarterly,97.175}} : std::vector<Row>{
            {2018,October,Monthly,97.8175},{2018,November,Monthly,97.770},
            {2018,December,Monthly,97.685},{2019,January,Monthly,97.595},
            {2019,February,Monthly,97.590},{2019,March,Monthly,97.525},
            {2019,March,Quarterly,97.440},{2019,June,Quarterly,97.295},
            {2019,September,Quarterly,97.220},{2019,December,Quarterly,97.170},
            {2020,March,Quarterly,97.160},{2020,June,Quarterly,97.165},
            {2020,September,Quarterly,97.175}};
        std::vector<ext::shared_ptr<RateHelper>> helpers;
        for (auto row : rows) helpers.push_back(ext::make_shared<SofrFutureRateHelper>(row.price,row.month,row.year,row.frequency));
        auto curve = ext::make_shared<PiecewiseYieldCurve<Discount,Linear>>(today,helpers,Actual365Fixed());
        for (Size n = 0; n < rows.size(); ++n) {
            auto row = rows[n]; auto h = helpers[n];
            std::cout << (june ? "juneteenth" : "bootstrap") << ',' << row.year << ',' << int(row.month) << ','
                      << int(row.frequency) << ',' << row.price << ',' << h->earliestDate().serialNumber() << ','
                      << h->maturityDate().serialNumber() << ',' << curve->discount(h->maturityDate()) << ',' << h->impliedQuote() << '\n';
        }
    }
}
