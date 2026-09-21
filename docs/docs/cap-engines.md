# Normal and lattice cap/floor pricing

The development version adds `BachelierCapFloorEngine`, `TreeCapFloorEngine`, and
`CapHelper` in Rust, Python and Go. These additions await the next release.

Use the Bachelier engine for normal volatility, including negative rates and
strikes. Volatility is expressed in absolute rate units: `0.01` means 100 basis
points. Construct it from a normal optionlet surface or an observable flat quote.
The engine returns NPV and aggregate vega; a collar is long the cap and short the
floor. Live quotes, curves and evaluation dates invalidate cached valuations.

```python
from itofin.pricingengines import BachelierCapFloorEngine
from itofin.quotes import SimpleQuote

normal_vol = SimpleQuote(0.01)
engine = BachelierCapFloorEngine.with_flat_vol(
    discount_curve, normal_vol, day_counter, settings
)
cap.set_bachelier_engine(engine)
value = cap.npv()
vega = cap.results().additional_results["vega"]
```

`TreeCapFloorEngine(model, time_steps)` prices through a retained `HullWhite`
model. Its fixed-grid constructor accepts the complete mandatory time list,
sorts and deduplicates it, and inserts zero without subdivision. Missing coupon
reset or payment nodes return an error. A moving reference date can invalidate a
fixed grid; rebuild the grid from the helper's current mandatory times.

```python
from itofin.pricingengines import TreeCapFloorEngine

tree = TreeCapFloorEngine(model, 100)
cap.set_tree_engine(tree)
value = cap.npv()
```

The tree engine requires the first accrual start to be at or after the model
curve's reference date. Rust's `DiscretizedCapFloor` also supports known historical
fixings when initialized on a suitable lattice. Other short-rate models and
engine-level fallback curves are outside this concrete Hull-White API.

`CapHelper` accepts a retained `SimpleQuote`, an Ibor index, a discount curve, fixed
leg conventions, a first-swaplet flag, an error type and a volatility type. Normal
quotes use Bachelier prices; shifted-lognormal quotes preserve the existing Black
path. `mandatory_times()` supplies reset and payment times; `set_tree_engine()`
selects model pricing. `HullWhite.calibrate_caps()` fits these helpers on a tree
and optionally fixes mean reversion. Model queries rebuild the ATM strike and
schedule so curve and date changes are reflected.

Go uses `Session.NewBachelierCapFloorEngineFlat`, `Session.NewTreeCapFloorEngine`,
`Session.NewCapHelper`, and `HullWhite.CalibrateCaps`. Inputs must share a session.
Native engines and helpers retain their dependencies after input wrappers close;
closing the session ends all access.

Independent QuantLib 1.43 fixtures pin cap/floor/collar prices, normal vegas,
negative-rate and zero-volatility cases, and the calibrated Hull-White sigma.
Rust also checks lattice refinement against the analytic cap price.
