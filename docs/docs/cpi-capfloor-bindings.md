# CPI cap/floor quoted-price interpolation

`CpiCapFloor` is a single-payment option on cumulative inflation, not a cap or
floor on each coupon. It is available in Rust, C, Go and Python together with
`CpiCapFloorTermPriceSurface` and `InterpolatingCpiCapFloorEngine`.

## Supported contract

| Area | Behavior |
| --- | --- |
| Market input | Separate cap/floor annual-quoted strikes and price matrices |
| Matrix layout | Strike rows, maturity columns |
| Maturities | At least two increasing positive whole-year tenors |
| Interpolation | Bilinear in time and annual-quoted strike |
| Extrapolation | Always enabled, extending boundary cells; negative values are possible |
| Observation | Flat or Linear CPI interpolation; nonzero lag uses Months or Years |
| Pricing | Price-surface lookup, not a Black/Bachelier volatility model |
| Expiry | Evaluation date strictly after the unadjusted contract maturity |

Cap and floor prices must be positive and finite at input. Cap prices cannot
increase with strike; floor prices cannot decrease. Each input strike grid must
increase and have at least two strikes. The native strike-union convention
requires more than two combined strikes: floor strikes followed by cap strikes
strictly above the largest floor strike (with the native `1e-7` separation).
Overlapping quoted strikes retain their respective cap and floor quotes.

### Price units are important

**The engine returns the surface quote directly. It does not multiply by the
instrument nominal, base CPI or contract start date.** The surface nominal is
also descriptive metadata, not a multiplier in cap/floor parity completion.
These are inherited QuantLib engine conventions, not an assertion that changing
notional leaves an economic option's value unchanged. Supply quotes in the
required units and scale externally if appropriate for the application.

Missing cap or floor cells are completed using native put/call parity and the
nominal discount curve plus the zero-inflation index's ATM inflation rate.
This uses compounded annual quoted strikes, not a direct CPI-ratio strike.

## Immutable snapshot

The surface captures its reference date, parity-completed grids and time grid at
construction. **Rebuild it after evaluation-date, fixing or curve changes.** It
retains no unused live pricing graph and exposes no dynamic ATM selector.
An engine retains its surface; an instrument retains its engine, index, calendars
and explicit settings. Releasing the corresponding C/Go/Python wrapper does not
invalidate these retained dependencies.

This intentionally differs from native evaluation-date rolling: the pinned
QuantLib surface constructs its parity grids once, but moves its reference date
without rebuilding those grids. Our surface freezes both to avoid mixing stale
grids and a new time origin. Instrument expiry still follows its live settings.
Rebuild instruments when changing calendar conventions or holiday definitions.

## Engine timing

The instrument observation lag must be no shorter than the surface lag. Their
normalized month difference shifts the adjusted payment date backwards to obtain
the effective maturity. Flat observation queries the start of its inflation
period. Linear observation interpolates prices at that period's start and the
next period's start using the effective maturity's day weight.

The index availability lag must be covered: Flat allows equality, while Linear
requires a strictly longer observation lag. Deprecated `AsIndex` is not exposed.
Mixed settings, incompatible lag units, invalid dates, non-finite values and
unavailable calendar years return ordinary errors. C outputs are unchanged when
an operation fails. The usual C pointer-validity and Go session rules apply.

## Python example

```python
from itofin.instruments import CpiCapFloor, OptionType
from itofin.pricingengines import InterpolatingCpiCapFloorEngine
from itofin.termstructures import CpiCapFloorTermPriceSurface

# index, nominal discount curve, calendars, day counter and explicit settings
# are existing objects. Matrices are quoted prices, not volatilities.
surface = CpiCapFloorTermPriceSurface(
    nominal=100000.0, base_rate=0.025, observation_lag=lag,
    calendar=calendar, convention=convention, day_counter=day_counter,
    index=index, interpolation=interpolation, discount=discount,
    cap_strikes=[0.03, 0.04], floor_strikes=[0.01, 0.02],
    maturity_years=[1, 3, 5],
    cap_prices=[[0.08, 0.15, 0.24], [0.04, 0.10, 0.16]],
    floor_prices=[[0.02, 0.04, 0.06], [0.035, 0.065, 0.095]],
    settings=settings,
)
engine = InterpolatingCpiCapFloorEngine(surface)
option = CpiCapFloor(
    OptionType.Call, 100000.0, start, 100.0, maturity,
    calendar, convention, calendar, convention, 0.025,
    index, lag, interpolation, settings, engine,
)
quote_value = option.npv()
```

Go uses `Session.NewCpiCapFloorTermPriceSurface(CpiPriceSurfaceConfig{...})`,
`NewInterpolatingCpiCapFloorEngine`, and `NewCpiCapFloor(CpiCapFloorConfig{...})`.
Close objects and sessions explicitly. Calls from different sessions are rejected.
C uses `itofin_cpi_price_surface_*` and `itofin_cpi_capfloor_*` with typed handles.

## Numerical evidence and limits

The tracked native fixture is `sdk/go/testdata/cpi-capfloor-native.tsv` with a
matching Python fixture. Its generator is
`scripts/generate_cpi_capfloor_oracle.py` and the adjacent C++ source. Provenance
is in `sdk/go/testdata/cpi-capfloor-native.provenance.json`.

The harness combines the protected QuantLib 1.43 wheel's exported native symbols
with surface-template code compiled from source revision
`9863b578af0caa4cecabf697196533e84a8308b6`. Three independent ABI sanity values
match the wheel's Python queries. This is **mixed source/binary provenance**,
not a claim that the wheel was built from that revision.

Fixtures include quoted/parity cells, off-grid interpolation, extrapolation,
Flat/Linear timing, differing lags, ignored nominal/base/start metadata,
immutable old grids versus rebuilt market data, and raw-maturity expiry.
Native month-tenor exponent behavior and native moving-reference behavior are
recorded as unsupported/divergent, not silently repaired or counted as parity.
There is no CPI volatility stripping, calibration, coupon cap/floor engine,
`AsIndex`, nonyear-tenor support or full QuantLib class parity in this slice.
