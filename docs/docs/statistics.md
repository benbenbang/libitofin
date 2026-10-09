# Weighted statistics and empirical risk

## Batch statistics

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

## Incremental statistics

`IncrementalStatistics` keeps weighted moments in fixed memory as observations
arrive. It accepts finite values and finite nonnegative weights; a zero-weight
value still counts and can set the minimum or maximum. A batch is added all at
once or leaves the accumulator unchanged on error. `reset` clears the samples.

Variance, standard deviation, error estimate, skewness and kurtosis use the
core's observation-count corrections. Downside statistics include strictly
negative observations. Downside variance requires two such observations with
positive combined weight. Percentile, VaR and ES require the stored-sample
statistics facade; the incremental accumulator has no empirical tail queries.

=== "Python"

    ```python
    from itofin import statistics

    stream = statistics.IncrementalStatistics()
    stream.add_batch([-4.0, -2.0, 2.0, 8.0], weights=[1.0, 2.0, 1.0, 0.0])
    assert stream.samples() == 4
    assert stream.mean() == -1.5
    assert stream.downside_deviation() == 4.0
    stream.reset()
    ```

=== "Go"

    ```go
    session, err := itofin.NewSession()
    if err != nil { panic(err) }
    defer session.Close()
    stream, err := session.NewIncrementalStatistics()
    if err != nil { panic(err) }
    defer stream.Close()
    err = stream.AddBatch([]float64{-4, -2, 2, 8}, []float64{1, 2, 1, 0})
    if err != nil { panic(err) }
    mean, err := stream.Mean()
    if err != nil { panic(err) }
    _ = mean
    ```

## Weighted vector statistics

Sequence batch functions treat each **row as one observation** and each column
as one component. All rows must have the same positive width. Pass one optional
nonnegative weight per row, not per component; omitted Python weights or Go
`nil` weights mean one per row. No NumPy/pandas array conversion or additional Go dependency is
needed. Missing observations, NaN and infinities are rejected: callers align
and clean their own data, including calendar alignment.

| Python `statistics` function | Go function | Result |
| --- | --- | --- |
| `sequence_mean` | `StatisticsSequenceMean` | One weighted mean per column |
| `sequence_variance` | `StatisticsSequenceVariance` | One count-corrected variance per column |
| `sequence_standard_deviation` | `StatisticsSequenceStandardDeviation` | Square roots of those variances |
| `sequence_error_estimate` | `StatisticsSequenceErrorEstimate` | `sqrt(variance / N)` per column |
| `sequence_minimum` / `sequence_maximum` | `StatisticsSequenceMinimum` / `StatisticsSequenceMaximum` | Column extrema, including zero-weight rows |
| `covariance_matrix` / `correlation_matrix` | `StatisticsCovariance` / `StatisticsCorrelation` | Symmetric column-by-column matrices |

Let `W = Σwᵢ`, `μⱼ = Σ(wᵢ xᵢⱼ)/W`, and `N` be the number of rows.
Covariance entry `(j, k)` is
`N/(N-1) × Σ[wᵢ (xᵢⱼ - μⱼ)(xᵢₖ - μₖ)]/W`.
Its diagonal is the sequence variance. This is **observation-count correction**,
not frequency-weight or effective-sample-size correction. Zero-weight rows still
increase `N`, change the correction/error estimate and can set extrema.

Correlation is covariance divided by the two standard deviations, with these
explicit constant-column conventions:

- Diagonal entries are one, including constant columns.
- Two zero-variance columns have correlation one.
- One zero-variance and one varying column have correlation zero.

Nonconstant correlations can differ from `[-1, 1]` by a few binary64 ULPs;
values are not clipped. Python converts sequence arguments before validating
shape; the limits below bound native storage, flattening and output allocation.

Covariance uses centered products, avoiding QuantLib's raw-product cancellation
for large offsets. For example, rows `[10¹²+i, 10¹²+2i]` for `i=0..3` have
covariance `[[5/3, 10/3], [10/3, 20/3]]`, not a negative variance from subtracting
large raw moments. This is an intentional stability correction; ordinary inputs
retain the pinned native conventions.

=== "Python"

    ```python
    from itofin import statistics

    rows = [[1.0, 4.0], [3.0, 2.0], [100.0, -20.0]]
    weights = [1.0, 3.0, 0.0]
    assert statistics.sequence_mean(rows, weights=weights) == [2.5, 2.5]
    covariance = statistics.covariance_matrix(rows, weights=weights)
    correlation = statistics.correlation_matrix(rows, weights=weights)
    assert abs(covariance[0][0] - 1.125) < 1e-12
    assert abs(correlation[0][1] + 1.0) < 1e-12
    assert statistics.sequence_maximum(rows, weights=weights) == [100.0, 4.0]
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    rows := [][]float64{{1, 4}, {3, 2}, {100, -20}}
    weights := []float64{1, 3, 0}
    mean, err := itofin.StatisticsSequenceMean(rows, weights)
    if err != nil { panic(err) }
    covariance, err := itofin.StatisticsCovariance(rows, weights)
    if err != nil { panic(err) }
    correlation, err := itofin.StatisticsCorrelation(rows, weights)
    if err != nil { panic(err) }
    _, _, _ = mean, covariance, correlation
    ```

Vector results are Python `list[float]` or Go `[]float64`; matrices are
Python `list[list[float]]` or Go `[][]float64`, with independent rows. Rust batch
inputs and Rust/C matrix outputs use flattened row-major storage. All functions
are stateless and return independent results, with no binding handles or sessions.
Every new sequence query, including min/max, requires a positive total weight. Mean/min/max need one row; variance, deviation, error
estimate and matrices need at least two. This stricter sequence-batch rule does
not change scalar `GeneralStatistics` min/max behavior.

Limits are 256 columns, 100,000 rows and 1,000,000 input values. Matrix requests
also require `rows × columns² ≤ 100,000,000`; output matrices have at most 65,536
entries. Invalid shapes, mismatched weight counts, negative/nonfinite weights,
zero total weight, exceeded limits and nonfinite arithmetic/results
return errors, not partial output. Tiny positive weights do not grant additional
sample-count or work-limit capacity.

Rust also provides `SequenceStatistics::new(dimension)` and `add_weighted` for
an owned accumulator; the binding APIs expose only these batch operations.
Independent native and exact-centered fixtures are in
[`sdk/go/testdata/sequence-statistics`](https://github.com/benbenbang/libitofin/tree/main/sdk/go/testdata/sequence-statistics).

## Ordered portfolio drawdown

[Maximum drawdown](drawdown.md) accepts ordered positive equity/NAV values,
not unordered return samples. It returns a nonnegative fractional running-peak
loss and deterministic zero-based peak/trough indices in Rust, C, Go and Python.
