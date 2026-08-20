# Hand-written stubs for itofin.quotes; sync manually with src/market.rs (#517).

class SimpleQuote:
    """A mutable, observable market element (D1).

    Wraps a single value that pricing inputs observe; setting a new value
    notifies dependents so any cached valuation recomputes lazily.
    """

    def __init__(self, value: float) -> None:
        """Initialize the quote.

        Args:
            value (float): The initial market value.
        """
        ...

    def value(self) -> float:
        """Return the current value.

        Returns:
            float: The quote's current market value.
        """
        ...

    def set_value(self, value: float) -> None:
        """Set a new value and notify observers.

        Args:
            value (float): The new value; observers are notified when it actually
                changes, so dependent valuations recompute on next access.
        """
        ...
