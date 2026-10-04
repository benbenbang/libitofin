# Uncentered L2 point-set discrepancy

Measure how a finite point set covers the unit cube. This ports QuantLib's
**uncentered L2 discrepancy**, not SciPy's default centered discrepancy `CD`.
It is a point-set diagnostic, **not** a Monte Carlo confidence interval,
convergence certificate or integration-error bound. A lower value for two
comparable point sets does not establish that a particular integrand's error
is smaller. Do not compare dimensions as though they shared one scale.

## Formula and domain

For `N` points `xᵢ` in dimension `d`, the returned value is the nonnegative
square root of:

```text
A = Σᵢ Σⱼ Πₖ (1 - max(xᵢₖ, xⱼₖ))
C = Σᵢ Πₖ (1 - xᵢₖ²)
D² = A/N² - 2^(1-d) C/N + 3^(-d)
```

- Every coordinate must be finite in the **closed** cube `[0, 1]^d`.
  Boundary points and duplicates are accepted.
- Dimension: **2..256**. Native QuantLib rejects dimension one; this API
  also rejects constructing dimension zero rather than leaving it unspecified.
- Point count: **1..4,096**, subject to **N²d ≤ 100,000,000** and
  **Nd ≤ 1,000,000**. Limits apply before flattening/native computation.
- Omitted weights or exactly one per point are accepted. All other weights
  are rejected; general weighted discrepancy is not implemented.
- Storage is **O(Nd)**; building the set costs **O(N²d)**. A Rust
  append costs O(Nd) and its final query costs O(1). This is not a
  large-stream, sparse or approximate discrepancy estimator.
- Rejected Rust additions/reset leave the existing accumulator unchanged.
  Rust `reset(0)` retains the initialized dimension and clears observations.
  Empty discrepancy queries raise an error.
- Pair and coordinate-product sums use compensated accumulation; ordinary
  roundoff can differ from native QuantLib. Binary64 products can underflow
  in high dimensions. Squared discrepancy
  is rounded to zero only for a small negative result within
  `8 × epsilon × (d + 2) × (|A/N²| + |2^(1-d) C/N| + |3^(-d)|)`; larger
  negative or nonfinite results raise an error. This is not arbitrary precision.

Independent fixtures compile the pinned actual QuantLib class and compare
against a separate exact rational evaluation. Seeded fixtures use the actual
pinned Sobol **Jaeckel** sequence and Mersenne Twister, not an unrelated library's
Sobol order. No extra numerical package is required by the Go/Python calls.

## Paired examples

Four interior unit-square points below have exact squared discrepancy
**71/4,608**, so the returned discrepancy is approximately **0.124128909248**.

=== "Rust"

    ```rust
    use libitofin::math::statistics::DiscrepancyStatistics;

    let mut statistics = DiscrepancyStatistics::new(2)?;
    for point in [[0.25, 0.25], [0.75, 0.75], [0.25, 0.75], [0.75, 0.25]] {
        statistics.add(&point)?;
    }
    let discrepancy = statistics.discrepancy()?;
    assert!((discrepancy - (71.0_f64 / 4608.0).sqrt()).abs() < 1e-12);
    ```

=== "Python"

    ```python
    import math
    from itofin import statistics

    points = [[0.25, 0.25], [0.75, 0.75], [0.25, 0.75], [0.75, 0.25]]
    discrepancy = statistics.discrepancy(points)
    assert math.isclose(discrepancy, math.sqrt(71.0 / 4608.0), abs_tol=1e-12)
    ```

=== "Go"

    ```go
    import itofin "github.com/benbenbang/libitofin/sdk/go"

    points := [][]float64{{0.25, 0.25}, {0.75, 0.75}, {0.25, 0.75}, {0.75, 0.25}}
    discrepancy, err := itofin.StatisticsDiscrepancy(points, nil)
    if err != nil { panic(err) }
    _ = discrepancy
    ```

Each row is one point, each column one coordinate. Go/Python return a scalar;
there is no hidden random generation or persistent handle in these batch calls.
Python accepts ordinary row sequences, including lists and tuples, not
iterator-only generators. Python argument extraction occurs before the native
shape/work gate; passing a huge Python sequence still has conversion cost.

Executable examples:

- [Rust](https://github.com/benbenbang/libitofin/blob/main/crates/libitofin/examples/discrepancy_statistics.rs)
- [Python](https://github.com/benbenbang/libitofin/blob/main/example/python/discrepancy_statistics.py)
- [Go](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/discrepancy-statistics/main.go)
- [Pinned oracle and regeneration](https://github.com/benbenbang/libitofin/tree/main/sdk/go/testdata/discrepancy-statistics)

For mean trajectories use [mean-convergence diagnostics](convergence.md).
For weighted vector covariance and correlation use [weighted statistics](statistics.md#weighted-vector-statistics).
