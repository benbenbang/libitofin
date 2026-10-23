"""Standalone forward-measure process, not a calibrated Hull-White model."""

# itofin library
from itofin.processes import HullWhiteForwardProcess
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter

curve = FlatForward(Date(23,10,2026),.03,DayCounter.actual365_fixed())
process = HullWhiteForwardProcess(curve,.1,.02)
process.set_forward_measure_time(5.)
print(process.expectation(1.,.03,.8), process.variance(1.,.03,.8))
