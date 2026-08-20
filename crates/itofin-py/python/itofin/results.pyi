# Hand-written stubs for itofin.results; sync manually with src/results.rs (#517).

from itofin.time import Date

class Results:
    """A read-only snapshot of one instrument valuation, handed back by every
    instrument facade's results(). It holds a copy, not a view: once taken, an
    input change reprices the instrument's live accessors and leaves the
    snapshot reporting the valuation it was taken from.

    Every field is optional because the core stores it that way - an engine
    fills what it computes and leaves the rest unset. The analytic European
    engine, for one, provides a value but neither an error estimate nor a
    valuation date.

    additional_results is REAL-ONLY: the core keeps the engine's extra outputs
    behind a type-erased handle whose only sanctioned downcast is to a real, so
    tags holding anything else are OMITTED from the dict rather than guessed
    at."""

    @property
    def npv(self) -> float | None: ...
    @property
    def error_estimate(self) -> float | None: ...
    @property
    def valuation_date(self) -> Date | None: ...
    @property
    def additional_results(self) -> dict[str, float]: ...
