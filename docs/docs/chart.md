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
