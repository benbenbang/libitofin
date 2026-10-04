# Monte Carlo mean-convergence diagnostics

Inspect how a scalar weighted sample mean changes as observations arrive.
This diagnostic **does not certify convergence**, select a sample size or
return a confidence interval or an error estimate. Correlated paths, bias,
rare events and poor random-number coverage can still mislead a stable mean.

## Checkpoint and weight contract

- Default checkpoint counts: **1, 3, 7, 15, ...**; next count is `2n + 1`.
  Custom sampling rules are not exposed.
- Each checkpoint records `(observation count, weighted mean)`, with
  `mean = Σ(wᵢ xᵢ) / Σwᵢ` over that prefix. Counts include zero-weight rows.
- An unfinished prefix is **not** appended. Four observations therefore yield
  checkpoints at 1 and 3, even though the current mean uses all four.
- Observations and weights must be finite; weights must be nonnegative and
  total prefix weight positive whenever a checkpoint mean is evaluated.
  Omitted weights are one. The first observation needs positive weight.
- Maximum retained observation count: **100,000**. Input validation and batch
  additions are atomic: rejected operations leave the accumulator unchanged.
- An empty table is valid. An empty current mean raises an error. Reset clears
  observations, weight total and checkpoints and restarts at count one.
- Returned Python/Go tables are independent copies. Rust exposes an immutable
  borrowed table; no native callback or caller-buffer retention is involved.

The pinned QuantLib executable accepts a zero-weight first observation and
stores a NaN checkpoint mean. Itofin deliberately rejects this atomically.
The schedule above follows the executable `DoublingConvergenceSteps`, not
upstream prose suggesting plain powers of two. Independent native and exact
rational fixtures document this difference. Itofin uses the stable
`SequenceStatistics` mean kernel rather than native raw weighted arithmetic;
roundoff can intentionally differ for extreme cancellation or weight ratios.

## Paired examples

The first three values below have mean `(2×1 + 100×0 + 8×3)/4 = 6.5`.
After the fourth, the current mean is `25/3` but the table is unchanged.

=== "Rust"

    ```rust
    use libitofin::math::statistics::ConvergenceStatistics;

    let mut statistics = ConvergenceStatistics::new();
    statistics.add_batch(&[2.0, 100.0, 8.0, 12.0], Some(&[1.0, 0.0, 3.0, 2.0]))?;
    let table = statistics.convergence_table();
    assert_eq!((table[0].samples, table[0].mean), (1, 2.0));
    assert_eq!((table[1].samples, table[1].mean), (3, 6.5));
    assert_eq!(table.len(), 2);
    statistics.reset();
    ```

=== "Python"

    ```python
    from itofin import statistics

    observations = [2.0, 100.0, 8.0, 12.0]
    weights = [1.0, 0.0, 3.0, 2.0]
    assert statistics.convergence_table(observations, weights=weights) == [(1, 2.0), (3, 6.5)]
    accumulator = statistics.ConvergenceStatistics()
    accumulator.add_batch(observations, weights=weights)
    assert accumulator.convergence_table() == [(1, 2.0), (3, 6.5)]
    assert accumulator.samples() == 4
    accumulator.reset()
    assert accumulator.convergence_table() == []
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    observations := []float64{2, 100, 8, 12}
    weights := []float64{1, 0, 3, 2}
    table, err := itofin.StatisticsConvergence(observations, weights)
    if err != nil { panic(err) }
    session, err := itofin.NewSession()
    if err != nil { panic(err) }
    defer session.Close()
    accumulator, err := session.NewConvergenceStatistics()
    if err != nil { panic(err) }
    defer accumulator.Close()
    if err := accumulator.AddBatch(observations, weights); err != nil { panic(err) }
    current, err := accumulator.Table()
    if err != nil { panic(err) }
    _, _ = table, current
    ```

Go accumulators belong to their session. Calls on one session are serialized
on its worker thread, including calls from multiple goroutines. Close objects
and then the session explicitly; do not use closed handles.
Python accepts ordinary sequences, including lists and tuples; iterator-only
generators are not part of this contract. No NumPy/pandas array conversion
is needed.

Executable examples:

- [Rust](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/convergence_statistics.rs)
- [Python](https://github.com/benbenbang/libitofin/blob/main/example/python/convergence_statistics.py)
- [Go](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/convergence-statistics/main.go)
- [Pinned oracle and regeneration](https://github.com/benbenbang/libitofin/tree/main/sdk/go/testdata/convergence-statistics)

Use [weighted statistics](statistics.md) for other scalar moments.
Mean stability does not establish point-set coverage.
