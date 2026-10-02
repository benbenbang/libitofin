"""Price a European Bates option and observe a live spot update."""

# itofin library
from itofin import Settings
from itofin.instruments import OptionType, VanillaOption
from itofin.models import BatesModel
from itofin.pricingengines import BatesEngine
from itofin.processes import BatesProcess
from itofin.quotes import SimpleQuote
from itofin.termstructures import FlatForward
from itofin.time import Date, DayCounter

today = Date(2, 10, 2026)
expiry = Date(2, 10, 2027)
settings = Settings()
settings.set_evaluation_date(today)
dc = DayCounter.actual365_fixed()
spot = SimpleQuote(100.0)
rate = SimpleQuote(0.03)
dividend = SimpleQuote(0.01)
process = BatesProcess(
    spot,
    FlatForward.from_quote(today, rate, dc),
    FlatForward.from_quote(today, dividend, dc),
    0.04, 1.5, 0.04, 0.3, -0.5, 0.7, -0.12, 0.18,
)
model = BatesModel(process)
engine = BatesEngine(model, integration_order=144)
option = VanillaOption(OptionType.Call, 100.0, expiry, settings)
option.set_bates_engine(engine)
print("Bates NPV:", option.npv())
spot.set_value(105.0)
print("After spot update:", option.npv())
print("Calibration parameter order:", model.params())
