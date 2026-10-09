# Chart indicators

## Dated OHLC prices

Use `interval_prices` to validate OHLC bars and return them in date order.
Duplicate dates keep the last input bar. Prices must be finite, with open and
close inside the low-to-high range; negative prices are valid.

=== "Python"

    ```python
    from itofin import chart
    from itofin.time import Date

    dates = [Date(2, 1, 2024), Date(1, 1, 2024)]
    bars = chart.interval_prices(dates, [11, 10], [12, 11], [9, 8], [10, 9])
    assert bars[0].date == dates[1]
    assert bars[0].close == 9
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    // dates is a []itofin.Date in input order.
    bars, err := itofin.IntervalPrices(dates,
        []float64{11, 10}, []float64{12, 11},
        []float64{9, 8}, []float64{10, 9})
    if err != nil { panic(err) }
    // bars[0] is the earliest date.
    ```

The five input arrays must have equal lengths. The output contains one bar per
distinct date and leaves the input arrays unchanged.

## Close-price volatility

The simple local estimator uses the absolute log return between consecutive
positive closes, divided by the square root of the interval's year fraction.
The first bar is warmup. Pass one fraction per close for varying intervals;
the fraction at index zero is unused. The constant-fraction helper applies one
positive year fraction to every interval.

The constant estimator uses the **previous** `window` valid volatility values,
excluding the current value. It follows QuantLib's formula
`sqrt(sum(u²)/window - sum(u)²/(window*(window+1)))`, not a rolling standard
deviation. The output retains the input's alignment and warmup slots.

=== "Python"

    ```python
    from itofin import chart

    close = [100.0, 110.0, 99.0]
    local = chart.simple_local_volatility_constant_fraction(close, 1 / 252)
    constant = chart.constant_volatility(local, window=1)
    assert local.to_list()[0] is None
    assert constant.first_valid == 2
    assert abs(constant.to_list()[2] - 1.0698541148988145) < 1e-12
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    close := []float64{100, 110, 99}
    local, err := itofin.SimpleLocalVolatilityConstantFraction(close, 1.0/252)
    if err != nil { panic(err) }
    constant, err := itofin.ConstantVolatility(local, 1)
    if err != nil { panic(err) }
    _ = constant.NullableValues()
    ```

## Pointwise OHLC volatility

`ohlc_point_volatility` returns four annualized QuantLib estimators for each
bar: `simple_sigma`, `parkinson_sigma`, `garman_klass_sigma4`, and
`garman_klass_sigma5`. Simple Sigma uses the current close/open return, unlike
the consecutive-close estimator above. All four series start at index zero;
there is no warmup bar. Supply one positive year fraction per bar, or use the
constant-fraction helper. OHLC prices must be positive and finite, with open
and close inside the low-to-high range. Input order is preserved.

With `u = ln(high/open)`, `d = ln(low/open)`, and `c = ln(close/open)`, the four
point variances are `c²`, `(u-d)²/(4 ln 2)`,
`0.511(u-d)² - 0.019(c(u+d)-2ud) - 0.383c²`, and
`0.5(u-d)² - (2 ln 2-1)c²`. Each reported volatility is
`sqrt(abs(point variance)/year fraction)`.

=== "Python"

    ```python
    from itofin import chart

    estimates = chart.ohlc_point_volatility_constant_fraction(
        open=[100.0], high=[110.0], low=[90.0], close=[105.0],
        year_fraction=1 / 252,
    )
    assert estimates.parkinson_sigma.first_valid == 0
    assert abs(estimates.parkinson_sigma.values[0] - 1.9131168640323526) < 1e-12
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    estimates, err := itofin.OHLCPointVolatilityConstantFraction(
        []float64{100}, []float64{110}, []float64{90}, []float64{105}, 1.0/252,
    )
    if err != nil { panic(err) }
    _ = estimates.ParkinsonSigma.NullableValues()
    ```

## Overnight OHLC volatility

`ohlc_overnight_volatility` returns annualized Garman-Klass Sigma1, Sigma3,
and Sigma6 estimates. Each result combines the current bar with the previous
bar's close, so index zero is a missing prefix and `first_valid` is one for a
nonempty series. Indexed year fractions belong to the interval ending at each
bar; the fraction at index zero is unused. The constant-fraction helper uses
one positive year fraction for every interval.

Let `g = ln(open[i]/close[i-1])`, `f` be the overnight fraction with `0 < f < 1`,
and `p` be the current bar's Simple, Parkinson, or Sigma4 point variance from
above. The corresponding coefficients `a` are `0.5`, `0.17`, and `0.012`.
Each estimate is `sqrt((a*g²/f + (1-a)*p/(1-f))/year_fraction[i])`.
An invalid or negative combined variance returns an error; unlike the
pointwise estimators, the overnight formula does not take its absolute value.
All OHLC prices must be positive and finite, with open and close inside the
low-to-high range.

=== "Python"

    ```python
    from itofin import chart

    estimates = chart.ohlc_overnight_volatility_constant_fraction(
        open=[100.0, 110.0], high=[100.0, 120.0],
        low=[100.0, 105.0], close=[100.0, 115.0],
        year_fraction=1 / 252, overnight_fraction=0.25,
    )
    assert estimates.garman_klass_sigma1.first_valid == 1
    assert abs(estimates.garman_klass_sigma1.values[1] - 2.2159224836472786) < 1e-12
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    estimates, err := itofin.OHLCOvernightVolatilityConstantFraction(
        []float64{100, 110}, []float64{100, 120},
        []float64{100, 105}, []float64{100, 115}, 1.0/252, 0.25,
    )
    if err != nil { panic(err) }
    _ = estimates.GarmanKlassSigma1.NullableValues()
    ```

## Fixed-parameter GARCH(1,1)

`garch11_filter` consumes **returns**, not prices. Given `alpha`, `beta`, and
`long_run_variance`, it uses `omega = (1-alpha-beta)*long_run_variance` and
`next_variance = omega + alpha*return² + beta*current_variance`. The last
parameter is a variance, despite QuantLib's historical `ltVol` name. Returns
and conditional volatility use the same units; the API does not annualize or
fit parameters.

The first return seeds variance as its square. Conditional volatility at index
`i >= 1` uses returns through index `i-1`; index zero is a zero placeholder and
`first_valid` is one. `next_variance` uses the final return and is the forecast
for the step after the input series. A single return has no valid conditional
volatility yet, but still produces a forecast. Use `garch11_forecast` to advance
the recurrence separately. Parameters must be finite and nonnegative, with
`alpha+beta < 1`; returns and all intermediate variances must be finite.

=== "Python"

    ```python
    from itofin import chart

    result = chart.garch11_filter([0.1] * 10, 0.2, 0.3, 0.4)
    assert result.conditional_volatility.first_valid == 1
    assert abs(result.conditional_volatility.values[1] - 0.452769) < 1e-6
    assert abs(result.next_variance**0.5 - 0.537187) < 1e-6
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    returns := []float64{0.1, 0.1, 0.1}
    result, err := itofin.Garch11Filter(returns, 0.2, 0.3, 0.4)
    if err != nil { panic(err) }
    _ = result.ConditionalVolatility.NullableValues()
    _ = result.NextVariance
    ```

## Fitted GARCH(1,1)

`garch11_fit` estimates stationary GARCH parameters from returns. It reports
`alpha`, `beta`, the intercept `omega`, QuantLib's mean Gaussian log likelihood
without the constant `log(2π)`, and the next conditional variance. Unlike the
fixed-parameter API, `omega` is an intercept; its implied long-run variance is
`omega / (1-alpha-beta)`.

The likelihood recurrence starts with zero previous variance and squared
return, as in QuantLib. The returned forecast uses the fitted parameters with
the filtering convention above, seeded by the first squared return. Fitting
requires 4 to 100,000 finite returns with nonzero variation in squared returns;
invalid or non-convergent fits raise an error. The upper bound keeps the
autocovariance calculation bounded.

=== "Python"

    ```python
    from itofin import chart

    returns = [0.2, -0.3, 0.1, 0.5, -0.4, 0.25, -0.1, 0.3]
    fitted = chart.garch11_fit(returns)
    filtered = chart.garch11_filter(
        returns, fitted.alpha, fitted.beta,
        fitted.omega / (1 - fitted.alpha - fitted.beta),
    )
    assert abs(fitted.next_variance - filtered.next_variance) < 1e-12
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    returns := []float64{0.2, -0.3, 0.1, 0.5, -0.4, 0.25, -0.1, 0.3}
    fitted, err := itofin.Garch11Fit(returns)
    if err != nil { panic(err) }
    longRunVariance := fitted.Omega / (1 - fitted.Alpha - fitted.Beta)
    _ = longRunVariance
    _ = fitted.NextVariance
    ```

## Indicator series

Chart calculations use one Rust implementation for Python and Go. Every result
has one value per input bar. `first_valid` marks the first usable result;
earlier zeroes are warmup placeholders. Use Python's `to_list()` or Go's
`NullableValues()` when a chart or JSON payload needs `null` for those bars.

=== "Python"

    ```python
    from itofin import chart

    close = [1.0, 2.0, 3.0, 4.0]
    average = chart.sma(close, 3)
    assert average.first_valid == 2
    assert average.to_list() == [None, None, 2.0, 3.0]
    assert average.values.tolist() == [0.0, 0.0, 2.0, 3.0]

    bars = chart.volume_bars(
        open=[1.0, 2.0], high=[3.0, 3.0], low=[0.0, 0.0],
        close=[2.0, 1.0], volume=[10.0, 11.0],
    )
    assert bars.direction.tolist() == [1, -1]
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    average, err := itofin.SMA([]float64{1, 2, 3, 4}, 3)
    if err != nil { panic(err) }
    // average.FirstValid == 2
    // average.NullableValues() has nil in the first two slots.

    bars, err := itofin.ChartVolumeBars(
        []float64{1, 2}, []float64{3, 3}, []float64{0, 0},
        []float64{2, 1}, []float64{10, 11},
    )
    if err != nil { panic(err) }
    // bars.Direction == []int8{1, -1}
    ```

`sma`/`SMA` takes the trailing arithmetic mean. `ema`/`EMA` seeds from the
first period-bar SMA, then applies weight `2/(period+1)`. Both require a
positive period. Short inputs return an aligned series with no valid values.
OHLCV arrays must have equal lengths and finite values, high and low must
contain both open and close, and volume must be nonnegative. Volume direction
compares each close with its open: `-1` down, `0` flat, `1` up. Chart colors
remain the caller's choice.

## Cumulative VWAP and on-balance volume

`vwap`/`VWAP` takes **caller-supplied prices** and nonnegative volumes. Choose
trade prices, closes, or precomputed typical prices explicitly. It computes
`sum(price * volume) / sum(volume)` cumulatively from the beginning of each
call, not a rolling window. Call it separately for each session or anchor;
there are no inferred dates or automatic intraday resets.

A zero-volume prefix is missing: `first_valid` is the first positive-volume
bar, or the input length when none exists. After that, zero volume carries the
previous VWAP. `obv`/`OBV` instead starts with a **valid zero at index 0** and
ignores the initial volume after validating it. Later price rises add volume,
falls subtract it, and equal closes preserve the previous OBV.

=== "Python"

    ```python
    from itofin import chart

    price = [10.0, 12.0, 11.0, 11.0, 9.0]
    volume = [0.0, 2.0, 1.0, 0.0, 3.0]
    weighted = chart.vwap(price, volume)
    signed = chart.obv(price, volume)
    assert weighted.first_valid == 1
    assert weighted.to_list()[0] is None
    assert abs(weighted.values[-1] - 31 / 3) < 1e-12
    assert signed.to_list() == [0.0, 2.0, 1.0, 1.0, -2.0]
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    price := []float64{10, 12, 11, 11, 9}
    volume := []float64{0, 2, 1, 0, 3}
    weighted, err := itofin.VWAP(price, volume)
    if err != nil { panic(err) }
    signed, err := itofin.OBV(price, volume)
    if err != nil { panic(err) }
    _ = weighted.NullableValues()
    _ = signed.Values
    ```

Rust exposes `math::chart::{vwap, obv}` with the same `ChartSeries` result.
C exposes `itofin_chart_vwap` and `itofin_chart_obv` with caller-owned output
buffers and one `first_valid` index. The hand fixture above has VWAP values
`[missing, 12, 35/3, 35/3, 31/3]`; all four facades use the same core.

Inputs must have equal lengths and finite prices/volumes. Negative prices are
valid, but negative volume is not. Overflowing cumulative volume or signed OBV
returns an error. VWAP uses an online weighted mean rather than raw products,
so extreme finite prices and tiny positive volumes need not overflow or
underflow artificially. Ordinary floating-point rounding still applies;
very small relative weights may round their contribution to zero.

The formulas follow [StockCharts VWAP](https://chartschool.stockcharts.com/table-of-contents/technical-indicators-and-overlays/technical-overlays/volume-weighted-average-price-vwap)
and [OBV](https://chartschool.stockcharts.com/table-of-contents/technical-indicators-and-overlays/technical-indicators/on-balance-volume-obv).
The zero OBV seed and missing zero-volume VWAP prefix are explicit local conventions.

## True range and Wilder ATR

`true_range`/`TrueRange` measures each bar's range including overnight gaps.
Bar zero uses `high - low`; later bars take the maximum of that range,
`abs(high - previous_close)`, and `abs(low - previous_close)`. It is valid from
index zero, even when the first range is zero.

`atr`/`ATR` seeds the arithmetic mean of the **first `period` true ranges,
including bar zero**, then applies Wilder smoothing:
`previous + (true_range - previous) / period`. The default is 14 bars.
`first_valid` is `period - 1`, capped at the input length; shorter inputs are
entirely missing. Period one returns the exact true-range series.

=== "Python"

    ```python
    from itofin import chart

    high = [12.0, 16.0, 11.0]
    low = [10.0, 14.0, 9.0]
    close = [11.0, 15.0, 10.0]
    ranges = chart.true_range(high, low, close)
    smoothed = chart.atr(high, low, close, period=3)
    assert ranges.to_list() == [2.0, 5.0, 6.0]
    assert smoothed.first_valid == 2
    assert smoothed.to_list()[:2] == [None, None]
    assert abs(smoothed.values[2] - 13 / 3) < 1e-12
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    high := []float64{12, 16, 11}
    low := []float64{10, 14, 9}
    close := []float64{11, 15, 10}
    ranges, err := itofin.TrueRange(high, low, close)
    if err != nil { panic(err) }
    smoothed, err := itofin.ATR(high, low, close, 3)
    if err != nil { panic(err) }
    _ = ranges.Values
    _ = smoothed.NullableValues()
    ```

Python's omitted `period` and Go's `DefaultATR` both select 14. Rust exposes
`math::chart::{true_range, atr, atr_default}`; C exposes
`itofin_chart_true_range` and `itofin_chart_atr` with caller-owned buffers.
The six-bar hand fixture has ranges `[2, 5, 6, 5, 2, 1]` and ATR(3) values
`[missing, missing, 13/3, 41/9, 100/27, 227/81]` across all four facades.

HLC arrays must have equal lengths and finite values with
`low <= close <= high`; negative prices are valid. Periods must be positive.
Every raw range difference must stay finite, including during warmup.
The seed uses an incremental mean to avoid overflowing an otherwise finite
average. Ordinary floating-point rounding still applies. C errors leave
outputs and `first_valid` unchanged.

Formula references: [Fidelity ATR](https://www.fidelity.com/learning-center/trading-investing/technical-analysis/technical-indicator-guide/atr)
and [AAII's first-bar and seed convention](https://www.aaii.com/journal/article/average-true-range-atr).

## Bollinger Bands and RSI

Bollinger Bands use a trailing population standard deviation. The default is
20 closes with a multiplier of 2; the middle, upper, and lower series all
become valid at index `period - 1`. Wilder RSI uses 14 price changes by default,
so its first valid index is `period`. A flat window returns 50, a gain-only
window 100, and a loss-only window 0.

=== "Python"

    ```python
    close = [100.0] * 34
    bands = chart.bollinger_bands(close, period=20, multiplier=2.0)
    strength = chart.rsi(close, period=14)
    upper_for_json = bands.upper.to_list()
    ```

=== "Go"

    ```go
    close := make([]float64, 34)
    bands, err := itofin.DefaultBollingerBands(close)
    if err != nil { panic(err) }
    strength, err := itofin.DefaultRSI(close)
    if err != nil { panic(err) }
    _ = bands.Upper.NullableValues()
    _ = strength.NullableValues()
    ```

Python functions accept explicit periods and also provide these defaults. Go
offers `ChartBollingerBands` and `RSI` for explicit parameters, alongside the
default helpers.

## Modern Keltner channels

`keltner_channels`/`ChartKeltnerChannels` uses the existing **EMA of closes**
for its center and shared **Wilder ATR** for width. This is the modern variant,
not the original typical-price SMA with a high-low-range envelope. Separate
`center_period` and `atr_period` settings preserve each indicator's arithmetic
seed. Upper and lower bands are `center +/- multiplier * ATR`.

Defaults are **EMA 20, ATR 10, multiplier 2**. The ATR period here is 10,
not standalone `atr`/`DefaultATR`'s 14. Every returned `center`, `upper`, and
`lower` series has the same first-valid index: the later of the two warmups,
capped at the input length. All earlier values are zero placeholders and
render as missing through `to_list()` or `NullableValues()`.

=== "Python"

    ```python
    from itofin import chart

    channels = chart.keltner_channels(
        high=[12.0, 16.0, 11.0], low=[10.0, 14.0, 9.0],
        close=[11.0, 15.0, 10.0],
        center_period=3, atr_period=2, multiplier=1.5,
    )
    assert channels.center.to_list() == [None, None, 12.0]
    assert channels.upper.values[2] == 153 / 8
    assert channels.lower.values[2] == 39 / 8
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    channels, err := itofin.ChartKeltnerChannels(
        []float64{12, 16, 11}, []float64{10, 14, 9},
        []float64{11, 15, 10}, 3, 2, 1.5,
    )
    if err != nil { panic(err) }
    _ = channels.Center.NullableValues()
    _ = channels.Upper.Values
    ```

Python returns a read-only `KeltnerChannels` result; its series getters and
NumPy values return copies. Go's `DefaultKeltnerChannels` selects the defaults.
Rust exposes `math::chart::{KeltnerChannels, keltner_channels,
keltner_channels_default}`. C's `itofin_chart_keltner_channels` writes
channel-major center, upper, then lower values with one shared `first_valid`.

Both periods must be positive and the multiplier finite and nonnegative.
Multiplier zero collapses the envelopes to the center but still validates
all HLC values and true-range differences, including during warmup. Inputs
must have equal lengths, finite values and `low <= close <= high`; negative
prices remain valid. Overflowing ATR offsets or either band returns an error,
with C outputs and metadata unchanged. Floating-point rounding still applies.

References: [StockCharts modern formula and defaults](https://chartschool.stockcharts.com/table-of-contents/technical-indicators-and-overlays/technical-overlays/keltner-channels)
and [TradingView's close-price source](https://www.tradingview.com/support/solutions/43000502266-keltner-channels-kc/).

## Taiwan KD and MACD

Taiwan KD uses a nine-bar highest-high/lowest-low RSV by default. A flat range
sets RSV to 50. K and D each use recursive smoothing with periods of three and
start from 50; all three series first become valid at index `period - 1`.
MACD defaults to fast/slow/signal periods of `(12, 26, 9)`. The price EMAs and
signal EMA each start from a simple average. The line first becomes valid at
index 25, and signal and histogram at index 33 with these defaults.

=== "Python"

    ```python
    close = [100.0] * 34
    oscillator = chart.kd(close, close, close)
    momentum = chart.macd(close)
    assert oscillator.k.to_list()[8] == 50.0
    assert momentum.signal.first_valid == 33
    ```

=== "Go"

    ```go
    close := make([]float64, 34)
    oscillator, err := itofin.DefaultKD(close, close, close)
    if err != nil { panic(err) }
    momentum, err := itofin.DefaultMACD(close)
    if err != nil { panic(err) }
    _ = oscillator.K.NullableValues()
    _ = momentum.Histogram.NullableValues()
    ```

Both bindings also accept explicit periods through Python keyword arguments
or Go's `ChartKD` and `ChartMACD` functions.

::: itofin.chart

## Williams percent R

`williams_r` / `WilliamsR` uses an **inclusive trailing window**, including the
current HLC bar. The default is **14 bars**. For the window's highest high H and
lowest low L, the value is `-100 * ((H-close)/(H-L))`, in **[-100, 0]**.
A close at H gives 0; a close at L gives -100. A flat window gives **-50**,
consistent with Taiwan KD's neutral RSV 50, without smoothing K or D.

First-valid is `min(period-1, len)`. Earlier dense values are zero placeholders,
not real oscillator values; use `to_list()` / `NullableValues()` for missing
warmup. Empty inputs return an empty result; short inputs remain entirely missing.
Inputs must be equal-length, finite and satisfy `low <= close <= high`.
Negative prices are valid. A positive period is required. Overflowing high-low
or high-close differences are rejected, including partial windows during warmup.
Returns and intermediates are not silently rescaled or clipped.

=== "Python"

    ```python
    from itofin import chart

    result = chart.williams_r(
        [12, 16, 11], [10, 14, 9], [11, 15, 10], period=3
    )
    assert result.first_valid == 2
    assert result.to_list()[:2] == [None, None]
    assert abs(result.values[2] + 600 / 7) < 1e-12
    ```

=== "Go"

    ```go
    package main

    import (
        "fmt"
        "math"
        itofin "github.com/benbenbang/libitofin/sdk/go"
    )

    func main() {
        result, err := itofin.WilliamsR(
            []float64{12, 16, 11}, []float64{10, 14, 9},
            []float64{11, 15, 10}, 3)
        if err != nil { panic(err) }
        if result.FirstValid != 2 || math.Abs(result.Values[2]+600.0/7) > 1e-12 {
            panic("unexpected Williams percent R")
        }
        fmt.Println(result.NullableValues())
    }
    ```

Rust exposes `math::chart::{williams_r, williams_r_default}` and returns the
existing owned `ChartSeries`. C's `itofin_chart_williams_r` takes an explicit
period and writes caller-owned values and first-valid only on success. No
context or result handle needs closing. Go's `DefaultWilliamsR` and Python's
omitted period use 14. Python value properties return independent NumPy copies.

[Independent fixture](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/tests/data/chart/williams_r.md):
exact standard-library Fraction calculations, shared across Rust, Go and Python;
standalone C and C++ clients separately pin the same rational values.
