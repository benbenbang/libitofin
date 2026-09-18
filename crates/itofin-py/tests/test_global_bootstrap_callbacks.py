"""Callback failures, confinement, and native ownership boundaries."""

import gc
import threading
import weakref

import pytest

from itofin import ItofinError
from itofin.indexes import Euribor
from itofin.quotes import SimpleQuote
from itofin.termstructures import FuturesRateHelper, FuturesType, SimpleQuoteVariables
from itofin.time import Date, DayCounter, Period
from test_global_bootstrap import build, strip


@pytest.mark.parametrize("kind", ["dates", "penalties"])
def test_callback_exception_is_contextual_and_failure_retries(kind):
    """An exception remains visible on repeated reads and can recover without rebuilding."""
    _, quotes, helpers = strip()
    state = {"fail": True, "calls": 0}

    def callback(*args):
        state["calls"] += 1
        if state["fail"]:
            raise ValueError("intentional callback failure")
        return []

    curve = build(helpers, **{f"additional_{kind}": callback})
    fallback = Date(17, 6, 2026) if kind == "dates" else helpers[-1].latest_relevant_date()
    assert curve.max_date() == fallback
    for _ in range(2):
        with pytest.raises(ItofinError, match=f"additional_{kind} callback failed: ValueError: intentional"):
            curve.discount(0.1)
    state["fail"] = False
    first = curve.discount(0.1)
    count = state["calls"]
    assert curve.discount(0.1) == first
    assert state["calls"] == count
    quotes[0].set_value(0.05)
    assert curve.discount(0.1) != first
    assert state["calls"] > count


@pytest.mark.parametrize(
    "callback,message",
    [
        (lambda *args: [float("nan")], "finite residuals"),
        (lambda *args: [float("inf")], "finite residuals"),
        (lambda *args: ["invalid"], "additional_penalties callback failed"),
    ],
)
def test_invalid_penalty_results_are_errors(callback, message):
    """Reject non-numeric and non-finite residuals before they can become a fit."""
    _, _, helpers = strip()
    curve = build(helpers, additional_penalties=callback)
    with pytest.raises(ItofinError, match=message):
        curve.discount(0.1)


def test_invalid_dates_and_changing_residual_count():
    """Callback extraction and residual dimension errors are typed failures."""
    _, _, helpers = strip()
    curve = build(helpers, additional_dates=lambda: ["invalid"])
    with pytest.raises(ItofinError, match="additional_dates callback failed"):
        curve.discount(0.1)
    counter = []

    def penalties(*args):
        counter.append(True)
        return [0.1] if len(counter) == 1 else [0.1, 0.2]

    curve = build(helpers, additional_penalties=penalties)
    with pytest.raises(ItofinError, match="residual count changed"):
        curve.discount(0.1)


def test_callback_quote_mutation_is_rejected_and_guard_recovers():
    """Mutating market inputs during fitting cannot recursively invalidate the curve."""
    _, quotes, helpers = strip()
    state = [True]

    def penalties(*args):
        if state[0]:
            quotes[0].set_value(0.06)
        return []

    curve = build(helpers, additional_penalties=penalties)
    with pytest.raises(ItofinError, match="mutation is not allowed"):
        curve.discount(0.1)
    assert quotes[0].value() == 0.04
    state[0] = False
    quotes[0].set_value(0.05)
    assert curve.discount(0.1) > 0


def test_reentrant_weak_curve_queries_and_callback_collection():
    """Trial-curve reads work while weak captures avoid native ownership cycles."""
    _, _, helpers = strip()
    holder = []
    observations = []

    def penalties(times, data):
        observations.append(holder[0]().discount(times[-1]))
        assert abs(observations[-1] - data[-1]) < 1.0e-14
        return []

    callback_ref = weakref.ref(penalties)
    curve = build(helpers, additional_penalties=penalties)
    holder.append(weakref.ref(curve))
    curve_ref = weakref.ref(curve)
    curve.discount(0.1)
    assert observations
    del curve, penalties
    gc.collect()
    assert curve_ref() is None
    assert callback_ref() is None


def test_native_consumer_retains_callback_after_wrapper_drops():
    """A native index remains usable and owns callbacks until its own release."""
    settings, quotes, helpers = strip()
    calls = []

    def penalties(*args):
        calls.append(True)
        return []

    callback_ref = weakref.ref(penalties)
    curve = build(helpers, additional_penalties=penalties)
    consumer = Euribor(Period(1, "Months"), curve, settings)
    curve_ref = weakref.ref(curve)
    first = consumer.fixing(Date(15, 6, 2026), True)
    del curve, penalties
    gc.collect()
    assert curve_ref() is None
    assert callback_ref() is not None
    count = len(calls)
    quotes[0].set_value(0.05)
    assert abs(consumer.fixing(Date(15, 6, 2026), True) - first) > 1.0e-3
    assert len(calls) > count
    del consumer
    gc.collect()
    assert callback_ref() is None


def test_strong_capture_requires_user_to_break_ownership_cycle():
    """Document the mixed native/Python GC boundary without unsafe __clear__."""
    _, _, helpers = strip()
    holder = []

    def penalties(*args):
        assert holder[0] is not None
        return []

    curve = build(helpers, additional_penalties=penalties)
    holder.append(curve)
    curve_ref, callback_ref = weakref.ref(curve), weakref.ref(penalties)
    del curve, penalties
    gc.collect()
    assert curve_ref() is not None
    holder.clear()
    gc.collect()
    assert curve_ref() is None
    assert callback_ref() is None


def test_callback_curve_is_thread_confined():
    """Cross-thread queries are rejected before executing Python callbacks."""
    _, _, helpers = strip()
    calls, errors = [], []
    curve = build(helpers, additional_penalties=lambda *args: calls.append(True) or [])

    def query():
        try:
            curve.discount(0.1)
        except BaseException as error:
            errors.append(str(error))

    thread = threading.Thread(target=query)
    thread.start()
    thread.join()
    assert len(errors) == 1
    assert "unsendable" in errors[0]
    assert not calls
    assert curve.discount(0.1) > 0


def test_observing_variable_handle_fails_without_recursing():
    """Incorrect additional-variable wiring returns an error instead of overflowing."""
    _, _, helpers = strip()
    convexity = SimpleQuote(0.01)
    future = FuturesRateHelper.from_end_date(
        SimpleQuote(95),
        Date(17, 6, 2026),
        Date(17, 9, 2026),
        DayCounter.actual360(),
        convexity,
        FuturesType.Custom,
    )
    curve = build(
        [helpers[0], future],
        additional_helpers=[helpers[1]],
        additional_penalties=lambda *args: [1.0e4 * helpers[1].quote_error()],
        additional_variables=SimpleQuoteVariables([convexity], [0.01], [0.0]),
    )
    for _ in range(2):
        with pytest.raises(ItofinError, match="unregistered quote handles"):
            curve.discount(0.1)
