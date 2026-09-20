"""Generate independent QuantLib 1.43 overnight-basis fixtures."""
import sys

import QuantLib as q

q.Settings.instance().evaluationDate = today = q.Date(23, 10, 2025)
calendar = q.TARGET()
dc = q.Actual360()
empty = q.YieldTermStructureHandle()


def handle(curve):
    return q.YieldTermStructureHandle(curve)


def basis_helpers(overnight, basis, discount):
    ibor = q.Euribor3M()
    return [q.OvernightIborBasisSwapRateHelper(
        q.QuoteHandle(q.SimpleQuote(basis)), q.Period(year, q.Years), 2,
        calendar, q.ModifiedFollowing, False, overnight, ibor, discount,
    ) for year in range(1, 6)]


def curve(helpers):
    result = q.PiecewiseLogLinearDiscount(today, helpers, dc, q.IterativeBootstrap(1e-14))
    result.discount(result.maxDate())
    return result


def emit(mode, basis, fitted, helpers, overnight, discount):
    for year, helper in enumerate(helpers, 1):
        assert abs(helper.impliedQuote() / basis - 1) < 1e-12
        schedule = q.Schedule(helper.earliestDate(), helper.maturityDate(), q.Period(3, q.Months), calendar, q.ModifiedFollowing, q.ModifiedFollowing, q.DateGeneration.Forward, False)
        swap = q.Swap(q.OvernightLeg([1.0], schedule, overnight, spreads=[basis]), q.IborLeg([1.0], schedule, q.Euribor3M(handle(fitted))))
        swap.setPricingEngine(q.DiscountingSwapEngine(discount if discount else handle(fitted)))
        assert abs(swap.NPV()) < 1e-10
        print(f"{mode},{basis:.17g},{year},{helper.earliestDate().serialNumber()},"
              f"{helper.maturityDate().serialNumber()},{helper.pillarDate().serialNumber()},"
              f"{fitted.discount(helper.pillarDate()):.17g}")


for explicit in (False, True):
    overnight = q.Estr(handle(q.FlatForward(today, 0.021, dc)))
    discount = handle(q.FlatForward(today, 0.012, dc)) if explicit else empty
    helpers = basis_helpers(overnight, 0.002, discount)
    emit(int(explicit), 0.002, curve(helpers), helpers, overnight, discount)

for basis in (0.002, 0.003):
    prior = handle(q.FlatForward(today, 0.025, dc))
    previous = None
    for iteration in range(40):
        overnight_helpers = [q.OISRateHelper(
            2, q.Period(year, q.Years), q.QuoteHandle(q.SimpleQuote(0.02 + year * 0.001)),
            q.Estr(), prior,
        ) for year in range(1, 6)]
        overnight_curve = curve(overnight_helpers)
        overnight = q.Estr(handle(overnight_curve))
        helpers = basis_helpers(overnight, basis, empty)
        fitted = curve(helpers)
        values = [fitted.discount(h.pillarDate()) for h in helpers]
        values += [overnight_curve.discount(h.pillarDate()) for h in overnight_helpers]
        if previous and max(abs(a - b) for a, b in zip(values, previous)) < 2e-15:
            print(f"basis={basis}, iterations={iteration + 1}, joint residual={max(abs(a-b) for a,b in zip(values, previous)):.17g}", file=sys.stderr)
            break
        previous = values
        dates, discounts = zip(*fitted.nodes())
        prior = handle(q.DiscountCurve(dates, discounts, dc))
        prior.enableExtrapolation()
    else:
        raise RuntimeError("coupled oracle did not converge")
    for helper in overnight_helpers:
        assert abs(helper.impliedQuote() / helper.quote().value() - 1) < 1e-12
    emit(2, basis, fitted, helpers, overnight, empty)
