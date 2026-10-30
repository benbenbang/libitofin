"""Callback cycle collection versus native retention (#1028, #1310)."""

import gc
import subprocess
import sys
import textwrap
import threading
import weakref
from pathlib import Path

import pytest

from itofin import Settings
from itofin.indexes import Euribor
from itofin.instruments import OptionType, SwapType, VanillaSwap, ZeroCouponBond
from itofin.models import HullWhite
from itofin.pricingengines import DiscountingBondEngine
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward, PiecewiseYieldCurve, SwapRateHelper
from itofin.time import BusinessDayConvention, Calendar, Date, DayCounter, Frequency, Period, Schedule
from test_global_bootstrap import build, strip


class Holder:
    """A weak-referenceable capture container."""

    def __init__(self, quotes):
        self.items: list = []
        self.quotes = quotes


def cycle(capture, hook=lambda: None):
    """Build a callback curve plus a native consumer; the callback captures capture(curve, consumer)."""
    settings, quotes, helpers = strip()
    holder = Holder(quotes)

    def penalties(*args):
        hook()
        assert holder.items
        return []

    curve = build(helpers, additional_penalties=penalties)
    consumer = Euribor(Period(1, "Months"), curve, settings)
    holder.items = capture(curve, consumer)
    consumer.fixing(Date(15, 6, 2026), True)
    return [weakref.ref(item) for item in (curve, holder, penalties)]


@pytest.mark.xfail(strict=True, reason="parked: T3 #1311")
def test_unreachable_transitive_cycle_through_consumer_is_collected():
    """callback -> consumer -> native curve -> callback is collected once unreachable."""
    refs = cycle(lambda curve, consumer: [consumer])
    gc.collect()
    assert [ref() for ref in refs] == [None, None, None]


@pytest.mark.xfail(strict=True, reason="parked: T3 #1311")
def test_unreachable_cycle_through_wrapper_and_consumer_is_collected():
    """A cycle holding both the wrapper and a consumer of the same curve is collected."""
    refs = cycle(lambda curve, consumer: [curve, consumer])
    gc.collect()
    assert [ref() for ref in refs] == [None, None, None]


def swap_over(index, settings):
    """A one-month swap forecasting off index, discounted on a flat curve."""
    schedule = Schedule(
        Date(17, 7, 2026),
        Date(17, 8, 2026),
        Frequency.Monthly,
        Calendar.target(),
        BusinessDayConvention.ModifiedFollowing,
    )
    swap = VanillaSwap(
        SwapType.Payer, 100.0, schedule, 0.03, DayCounter.actual360(),
        schedule, index, 0.0, DayCounter.actual360(), settings,
    )
    swap.set_engine(FlatForward(Date(15, 6, 2026), 0.03, DayCounter.actual365_fixed()), settings)
    return swap


def test_native_only_consumer_retains_callback():
    """A swap retains the index natively; no Python wrapper of the index or curve survives."""
    settings, quotes, helpers = strip()
    calls = []

    def penalties(*args):
        calls.append(True)
        return []

    callback_ref = weakref.ref(penalties)
    curve = build(helpers, additional_penalties=penalties)
    swap = swap_over(Euribor(Period(1, "Months"), curve, settings), settings)
    first = swap.npv()
    del curve, penalties
    gc.collect()
    assert callback_ref() is not None
    count = len(calls)
    quotes[0].set_value(0.05)
    assert abs(swap.npv() - first) > 1.0e-6
    assert len(calls) > count
    del swap
    gc.collect()
    assert callback_ref() is None


def test_last_consumer_releases_callback():
    """Callbacks stay alive until the last of several native consumers is dropped."""
    settings, _, helpers = strip()

    def penalties(*args):
        return []

    callback_ref = weakref.ref(penalties)
    curve = build(helpers, additional_penalties=penalties)
    first = Euribor(Period(1, "Months"), curve, settings)
    second = Euribor(Period(3, "Months"), curve, settings)
    del curve, penalties
    gc.collect()
    del first
    gc.collect()
    assert callback_ref() is not None
    assert second.fixing(Date(15, 6, 2026), True) > 0
    del second
    gc.collect()
    assert callback_ref() is None


def ibor_index(curve, settings):
    """An IborIndex forwarding on curve."""
    index = Euribor(Period(1, "Months"), curve, settings)
    return lambda: index.fixing(Date(15, 6, 2026), True)


def swap_engine(curve, settings):
    """A swap forecasting on a flat curve and discounting on curve."""
    flat = FlatForward(Date(15, 6, 2026), 0.03, DayCounter.actual365_fixed())
    swap = swap_over(Euribor(Period(1, "Months"), flat, settings), settings)
    swap.set_engine(curve, settings)
    return swap.npv


def rate_helper_discount(curve, settings):
    """A swap helper discounting on curve, bootstrapped into another curve."""
    helper = SwapRateHelper(
        SimpleQuote(0.03), Period(1, "Years"), Calendar.target(), Frequency.Annual,
        BusinessDayConvention.ModifiedFollowing, DayCounter.thirty360_bond_basis(),
        Euribor(Period(6, "Months"), None, settings), discount=curve,
    )
    outer = PiecewiseYieldCurve(Date(17, 6, 2026), [helper], DayCounter.actual365_fixed())
    outer.discount(0.5)
    return lambda: (helper, outer.discount(0.5))


def process_model(curve, settings):
    """A Hull-White model on curve."""
    model = HullWhite(curve, 0.1, 0.01)
    return lambda: model.discount_bond_option(OptionType.Call, 0.99, 0.05, 0.2)


def bond_engine(curve, settings):
    """A zero-coupon bond priced by a discounting engine on curve."""
    bond = ZeroCouponBond(2, Calendar.target(), 100.0, Date(17, 8, 2026), settings)
    bond.set_engine(DiscountingBondEngine(curve, settings))
    return bond.clean_price


@pytest.mark.parametrize("consumer", [ibor_index, swap_engine, rate_helper_discount, process_model, bond_engine])
def test_consumer_keeps_direct_cycle_callbacks_alive(consumer):
    """A live native consumer blocks clearing a direct cycle, and its callbacks still run."""
    calls = []

    def make():
        settings, quotes, helpers = strip()
        holder = []

        def penalties(*args):
            assert holder[0] is not None
            calls.append(True)
            return []

        curve = build(helpers, additional_penalties=penalties)
        curve.enable_extrapolation()
        holder.append(curve)
        return consumer(curve, settings), quotes, [weakref.ref(curve), weakref.ref(penalties)]

    query, quotes, refs = make()
    query()
    gc.collect()
    assert all(ref() is not None for ref in refs)
    count = len(calls)
    quotes[0].set_value(0.05)
    query()
    assert len(calls) > count
    del query
    gc.collect()
    assert [ref() for ref in refs] == [None, None]


def test_collection_inside_callback_keeps_running_curve():
    """gc.collect() from inside a callback cannot clear the curve being bootstrapped."""
    refs = cycle(lambda curve, consumer: [curve], hook=gc.collect)
    holder = refs[1]()
    assert isinstance(holder, Holder)
    holder.quotes[0].set_value(0.05)
    assert holder.items[0].discount(0.1) > 0
    del holder
    gc.collect()
    assert [ref() for ref in refs] == [None, None, None]


def test_foreign_thread_collection_is_conservative():
    """A gc pass on another thread skips unsendable traversal; the owner thread collects later."""
    refs = cycle(lambda curve, consumer: [curve])
    thread = threading.Thread(target=gc.collect)
    thread.start()
    thread.join()
    gc.collect()
    assert [ref() for ref in refs] == [None, None, None]


SHUTDOWN = """
import threading
from itofin.indexes import Euribor
from itofin.time import Date, Period
from test_global_bootstrap import build, strip
from test_global_bootstrap_callbacks_gc import cycle, swap_over

def keep(curve, consumer):
    return [curve, consumer]

alive = [cycle(keep), cycle(lambda c, i: [i])]
settings, quotes, helpers = strip()
retained = Euribor(Period(1, "Months"), build(helpers, additional_penalties=lambda *a: []), settings)
retained.fixing(Date(15, 6, 2026), True)
swap = swap_over(retained, settings)
thread = threading.Thread(target=lambda: alive.append(cycle(keep)))
thread.start()
thread.join()
print("ok", flush=True)
"""


def test_interpreter_shutdown_with_live_cycles_is_clean():
    """Finalization with live cycles, retained consumers and a foreign-thread cycle exits cleanly."""
    result = subprocess.run(
        [sys.executable, "-c", textwrap.dedent(SHUTDOWN)],
        cwd=Path(__file__).parent,
        capture_output=True,
        text=True,
        timeout=60,
    )
    assert result.returncode == 0, result.stderr
    assert result.stdout.strip() == "ok"
    assert result.stderr == "", result.stderr
