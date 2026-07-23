# Hand-written type stubs for the itofin extension module (issue #517).
#
# There is NO generator: pyo3-stub-gen does not model this crate's imperative
# sys.modules submodule registration (see lib.rs), so these .pyi files are
# maintained by hand. When a #[pymethods] signature in crates/itofin-py/src/*.rs
# changes, update the matching stub here. Submodule -> source map:
#   time          <- src/time.rs
#   quotes        <- src/market.rs
#   termstructures<- src/curve.rs
#   processes     <- src/market.rs, src/heston.rs
#   indexes       <- src/hullwhite.rs
#   instruments   <- src/option.rs, src/swap.rs, src/swaption.rs
#   models        <- src/heston.rs, src/hullwhite.rs, src/calibration.rs
#   optimization  <- src/calibration.rs
"""Python bindings for libitofin, a Rust port of QuantLib."""

from . import indexes as indexes
from . import instruments as instruments
from . import models as models
from . import optimization as optimization
from . import processes as processes
from . import quotes as quotes
from . import termstructures as termstructures
from . import time as time

__version__: str

class ItofinError(Exception):
    """Error raised by the itofin API, carrying the located message."""

class Settings:
    """The explicit, non-global evaluation-date store (D5)."""

    def __init__(self) -> None: ...
    def set_evaluation_date(self, date: time.Date) -> None:
        """Set the evaluation date, notifying observers if it changed."""
        ...
