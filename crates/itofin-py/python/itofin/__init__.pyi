# Hand-written type stubs for the itofin extension module (issue #517).
#
# There is NO generator: pyo3-stub-gen does not model this crate's imperative
# sys.modules submodule registration (see lib.rs), so these .pyi files are
# maintained by hand. When a #[pymethods] signature in crates/itofin-py/src/*.rs
# changes, update the matching stub here. Submodule -> source map:
#   cashflows     <- src/cashflows.rs
#   time          <- src/time.rs
#   quotes        <- src/market.rs
#   termstructures<- src/curve.rs, src/vol.rs, src/helpers.rs, src/swaptionvol.rs
#   processes     <- src/market.rs, src/heston.rs
#   indexes       <- src/hullwhite.rs
#   instruments   <- src/option.rs, src/swap.rs, src/swaption.rs
#   models        <- src/heston.rs, src/hullwhite.rs, src/calibration.rs
#   pricingengines<- src/swaptionengine.rs
#   optimization  <- src/calibration.rs
#   results       <- src/results.rs
"""Python bindings for libitofin, a Rust port of QuantLib."""

from . import cashflows as cashflows
from . import indexes as indexes
from . import instruments as instruments
from . import models as models
from . import optimization as optimization
from . import pricingengines as pricingengines
from . import processes as processes
from . import quotes as quotes
from . import results as results
from . import termstructures as termstructures
from . import time as time

__version__: str

class ItofinError(Exception):
    """Error raised by the itofin API, carrying the located message.

    Every fallible core call surfaces as this exception, whose message is the
    located form "file:line: message".
    """

class Settings:
    """The explicit, non-global evaluation-date store (D5).

    There is no global singleton: the exact settings object passed to a
    construction is the one it reads, so instruments built against different
    Settings do not see each other's evaluation date.
    """

    def __init__(self) -> None:
        """Create settings with no evaluation date set."""
        ...

    def set_evaluation_date(self, date: time.Date) -> None:
        """Set the evaluation date, notifying observers if it changed.

        The new date is in place before the notification goes out, so an
        observer that recomputes on the update reads the date that triggered it.

        Args:
            date (time.Date): The new evaluation date. Observers are notified only when this
                differs from the date already set.
        """
        ...

    def set_include_todays_cash_flows(self, value: bool | None) -> None:
        """Set whether cash flows on today's date enter an NPV; None clears.

        The flag is three-valued, as in the core.

        Args:
            value (bool | None): True or False decides the question outright; None clears it,
                restoring the unset state in which each pricing site applies its
                own default. The argument is required, so clearing is always
                deliberate.
        """
        ...

    def include_todays_cash_flows(self) -> bool | None:
        """Return the current setting, or None while it is unset.

        Returns:
            bool | None: The three-valued flag last set, or None if it has never been set or
            was cleared.
        """
        ...
