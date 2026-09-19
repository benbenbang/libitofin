"""Generate independent vanilla MakeSwaption forecast-fixing pins with QuantLib 1.43."""

import json
from pathlib import Path

import QuantLib as ql

assert ql.__version__ == "1.43"
ql.Settings.instance().evaluationDate = ql.Date(9, 10, 2015)
curve = ql.YieldTermStructureHandle(
    ql.FlatForward(ql.Date(9, 10, 2015), 0.05, ql.Actual360())
)
index = ql.SwapIndex(
    "EuriborSwapIsdaFixA",
    ql.Period(5, ql.Years),
    2,
    ql.EURCurrency(),
    ql.TARGET(),
    ql.Period(1, ql.Years),
    ql.ModifiedFollowing,
    ql.Thirty360(ql.Thirty360.BondBasis),
    ql.Euribor6M(curve),
)
rows = [
    {"date": date.ISO(), "fixing": index.fixing(date)}
    for date in [ql.Date(10, 10, 2016), ql.Date(11, 10, 2016), ql.Date(11, 4, 2016)]
]
Path(__file__).with_suffix(".json").write_text(
    json.dumps({"quantlib": ql.__version__, "fixings": rows}, indent=2, sort_keys=True) + "\n"
)
