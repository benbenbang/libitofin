"""An immutable standalone Gaussian state process, not a fitted model."""

# itofin library
from itofin.processes import MfStateProcess

p = MfStateProcess(0.0, [0.5, 1.5], [0.2, 0.4, 0.1])
print(f"MF state variance: {p.variance(0.0, 0.0, 1.0):.10f}")
