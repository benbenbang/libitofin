# Weighted statistics and empirical risk

These stateless functions accept signed observations. A negative observation is
a loss; value at risk (VaR) and expected shortfall (ES) return a nonnegative
loss magnitude. Pass one nonnegative weight per observation, or omit weights
to give every observation weight one. Inputs must be finite and the total
weight must be positive.

The mean is `Σ(wᵢ xᵢ) / Σwᵢ`. Sample variance uses the core's **observation-count**
correction, `N/(N-1) × Σ(wᵢ (xᵢ - mean)²) / Σwᵢ`, even when weights differ or
some are zero. Standard deviation is its square root. Mean requires at least
one observation; variance and standard deviation require at least two.

Percentile accepts a fraction in `(0, 1]` and returns the first sorted value
whose cumulative weight reaches that fraction of total weight. VaR and ES
accept a confidence level in `[0.9, 1)`. VaR is the negative of the lower
`1 - confidence` percentile, floored at zero. ES averages observations
**strictly below** the negative VaR threshold, negates that average, and floors
it at zero. ES raises an error when that tail has no positive-weight samples.

The remaining conditional risk measures use the same signed observations.
`semi_variance` measures squared distance below the weighted mean;
`downside_variance` uses zero; `regret` uses a supplied target. They apply the
count correction to the number of observations **strictly below** the threshold,
including zero-weight observations, and require at least two such observations
with a positive total tail weight. Their deviation forms are square roots.
`shortfall(target)` is the weighted probability below target, and
`average_shortfall(target)` is the conditional mean of `target - observation`
there. Average shortfall requires a positive-weight tail; shortfall returns zero
if none lies below target. Targets must be finite.

`potential_upside(confidence)` is the upper percentile floored at zero, with
confidence in `[0.9, 1)`. `top_percentile(probability)` walks the sorted
observations from highest to lowest and returns the first value whose
cumulative weight reaches probability in `(0, 1]`.

=== "Python"

    ```python
    from itofin import statistics

    observations = [-5.0, -2.0, 1.0, 2.0]
    weights = [1.0, 10.0, 1.0, 8.0]
    assert statistics.value_at_risk(observations, 0.9, weights=weights) == 2.0
    assert statistics.expected_shortfall(observations, 0.9, weights=weights) == 5.0
    assert statistics.mean([1.0, 3.0], weights=[1.0, 3.0]) == 2.5
    assert statistics.shortfall(observations, 0.0, weights=weights) == 11 / 20
    assert statistics.top_percentile(observations, 0.5, weights=weights) == -2.0
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    observations := []float64{-5, -2, 1, 2}
    weights := []float64{1, 10, 1, 8}
    var90, err := itofin.StatisticsValueAtRisk(observations, weights, 0.9)
    if err != nil { panic(err) }
    es90, err := itofin.StatisticsExpectedShortfall(observations, weights, 0.9)
    if err != nil { panic(err) }
    _, _ = var90, es90
    missed, err := itofin.StatisticsShortfall(observations, weights, 0)
    if err != nil { panic(err) }
    upper, err := itofin.StatisticsTopPercentile(observations, weights, 0.5)
    if err != nil { panic(err) }
    _, _ = missed, upper
    ```

Use `nil` weights in Go, or omit `weights` in Python, for unit weights. The
same signed-observation and tail conventions apply in both languages.
## Stored samples

`GeneralStatistics` retains observations so you can add data and query moments,
percentiles, and empirical risk repeatedly. `add_batch` is atomic: an invalid
value, weight, or length leaves the prior sample set unchanged. A zero-weight
observation increases `samples` and contributes to count-based moment
corrections. Weight-dependent queries need a positive total weight; min/max
need only samples. `reset` empties the set.

=== "Python"

    ```python
    from itofin import statistics

    samples = statistics.GeneralStatistics()
    samples.add_batch([-10.0, -5.0, 1.0], weights=[1.0, 2.0, 17.0])
    assert samples.value_at_risk(0.9) == 5.0
    assert samples.expected_shortfall(0.9) == 10.0
    samples.add(-20.0)
    assert samples.percentile(0.01) == -20.0
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    session, err := itofin.NewSession()
    if err != nil { panic(err) }
    defer session.Close()
    samples, err := session.NewGeneralStatistics()
    if err != nil { panic(err) }
    defer samples.Close()
    if err := samples.AddBatch([]float64{-10, -5, 1}, []float64{1, 2, 17}); err != nil { panic(err) }
    var90, err := samples.ValueAtRisk(0.9)
    if err != nil { panic(err) }
    _ = var90
    ```

Python owns the accumulator directly. Go accumulators belong to their creating
session; close each handle and the session when finished. The same count-based
variance and signed-loss rules as the batch functions apply.
