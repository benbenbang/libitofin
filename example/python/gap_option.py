"""A signed European gap option through the existing FD engine, not analytic Black."""

# itofin library
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.pricingengines import FdBlackScholesVanillaEngine
from itofin.processes import BlackScholesProcess
from itofin.time import Date, DayCounter

today = Date(15, 6, 2026)
settings = Settings()
settings.set_evaluation_date(today)
process = BlackScholesProcess(100.0, 0.05, 0.02, 0.2, today, DayCounter.actual360())
engine = FdBlackScholesVanillaEngine(process, t_grid=400, x_grid=400, damping_steps=2)
option = VanillaOption.gap(OptionType.Call, 100.0, 160.0, today + 360, settings)
print(f"Signed gap NPV (FD): {option.price_fd(engine):.8f}")
