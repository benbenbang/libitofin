# Hand-written stubs for itofin.results; sync manually with src/results.rs (#517).

# itofin library
from itofin.time import Date

class Results:
    """A read-only snapshot of one instrument valuation.

    Handed back by every instrument facade's results(), which forces the
    calculation and then copies the four fields out. It holds a copy, not a
    view: once taken, an input change reprices the instrument's live accessors
    and leaves the snapshot reporting the valuation it was taken from.

    Every field is optional because the core stores it that way - an engine
    fills what it computes and leaves the rest unset. The analytic European
    engine, for one, provides a value but neither an error estimate nor a
    valuation date.

    additional_results is REAL-ONLY: the core keeps the engine's extra outputs
    behind a type-erased handle whose only sanctioned downcast is to a real, so
    tags holding anything else are OMITTED from the dict rather than guessed
    at."""

    @property
    def npv(self) -> float | None:
        """The net present value.

        Returns:
            float | None: The value, or None when the engine provided none.
        """
        ...
    @property
    def error_estimate(self) -> float | None:
        """The standard error on the value.

        Returns:
            float | None: The standard error, or None on the engines that do
                not produce one, which is every analytic engine here.
        """
        ...
    @property
    def valuation_date(self) -> Date | None:
        """The date the value refers to.

        Returns:
            Date | None: The valuation date, or None when the engine did not
                say.
        """
        ...
    @property
    def additional_results(self) -> dict[str, float]:
        """The engine's extra named outputs, restricted to the real-valued tags.

        Returns:
            dict[str, float]: The real-valued tags; see the class docs for why
                the others are omitted.
        """
        ...
