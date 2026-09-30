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

=== "Python"

    ```python
    from itofin import statistics

    observations = [-5.0, -2.0, 1.0, 2.0]
    weights = [1.0, 10.0, 1.0, 8.0]
    assert statistics.value_at_risk(observations, 0.9, weights=weights) == 2.0
    assert statistics.expected_shortfall(observations, 0.9, weights=weights) == 5.0
    assert statistics.mean([1.0, 3.0], weights=[1.0, 3.0]) == 2.5
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
    ```

Use `nil` weights in Go, or omit `weights` in Python, for unit weights. The
same signed-observation and tail conventions apply in both languages.
