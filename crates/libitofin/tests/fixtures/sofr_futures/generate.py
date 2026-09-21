"""QuantLib 1.43 oracle for sofrfutures.cpp and holiday-clipped daily accrual."""
import csv
import pathlib

import QuantLib as ql

ROOT = pathlib.Path(__file__).parent
with (ROOT / "accrual.csv").open("w", newline="") as output:
    writer = csv.writer(output, lineterminator="\n")
    writer.writerow(["today", "start", "end", "averaging", "today_fixing", "price"])
    for averaging in (ql.RateAveraging.Simple, ql.RateAveraging.Compound):
        for today in (ql.Date(17, 6, 2024), ql.Date(21, 6, 2024), ql.Date(22, 6, 2024)):
            for today_fixing in (False, True):
                ql.IndexManager.instance().clearHistories()
                ql.Settings.instance().evaluationDate = today
                curve = ql.FlatForward(ql.Date(17, 6, 2024), .035, ql.Actual365Fixed())
                index = ql.Sofr(ql.YieldTermStructureHandle(curve))
                for day, rate in ((18, .02), (20, .025), (21, .03), (24, .04)):
                    date = ql.Date(day, 6, 2024)
                    if date < today or (today_fixing and date == today):
                        index.addFixing(date, rate)
                start, end = ql.Date(19, 6, 2024), ql.Date(30, 6, 2024)
                future = ql.OvernightIndexFuture(index, start, end, ql.QuoteHandle(), averaging)
                writer.writerow([today.serialNumber(), start.serialNumber(), end.serialNumber(),
                                 averaging, int(today_fixing), format(future.NPV(), ".17g")])
