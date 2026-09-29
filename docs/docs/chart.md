# Chart indicators

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

::: itofin.chart
