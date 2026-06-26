# Copilot Instructions — libitofin

libitofin is a ground-up, bottom-up port of [QuantLib](https://github.com/lballabio/QuantLib)
(~470 k LOC of C++) into safe, idiomatic Rust.  Every design decision in this codebase exists to
serve **one primary goal**: the Rust output must match the QuantLib C++ test-suite numbers
(`test-suite/*.cpp`) within tolerance.  That test-suite is the **oracle**.

---

## 1. The oracle always wins

When reviewing or suggesting code, the QuantLib C++ source is the authoritative reference — not
Rust community style guides, Clippy pedantic lints, or general "clean code" advice.

- **Do not** suggest restructuring an algorithm if the restructuring would change numerical
  behaviour or diverge from the C++ control flow.
- **Do not** flag variable names, loop shapes, branch ordering, or intermediate temporaries as
  "non-idiomatic" if they mirror the corresponding C++ implementation — the parallel structure is
  intentional and aids auditability.
- **Do** flag genuine bugs: wrong formula, wrong tolerance, wrong branch, off-by-one index.
- **Do** flag correctness risks: integer overflow, UB through unsafe, silent precision loss.

---

## 2. Settled design decisions — do not reopen

The table below lists cross-cutting decisions that are **fixed for the current phase**.  Do not
suggest alternatives to these choices; doing so creates noise and stalls reviews.

| Decision | What is settled |
|----------|----------------|
| **D1** | Observer/Observable uses a push-notification, dirty-flag model with weak-ref observer registry.  Do not suggest pull-based or channel-based alternatives. |
| **D2** | `Handle<T>` / `RelinkableHandle<T>` are newtypes over `Rc<RefCell<Link<T>>>`.  Do not suggest `Arc` or `Mutex` here. |
| **D3** | Shared ownership uses `Rc` (not `Arc`).  The codebase exposes three aliases — `Shared<T>`, `SharedMut<T>`, `WeakMut<T>` — that must be used throughout; do not suggest raw `Rc`/`Arc`/`RefCell` at call sites. |
| **D4** | Error handling uses `QlResult<T>` (`Result<T, QlError>`) raised via the `fail!`, `require!`, `assert_ql!`, and `ensure!` macros.  Do not suggest `anyhow`, `eyre`, panics, or `unwrap` in library code. |
| **D5** | `Settings` is an explicit value object passed as `&Context`, not a `thread_local` global or a `lazy_static`. |
| **D6** | The core is single-threaded-mutable.  No `async`/`tokio` anywhere in `crates/itofin`.  Parallelism is `rayon` snapshot-and-fan-out, added only at L9–L11. |
| **D7** | FFI lives in sibling crates (`itofin-ffi`, `itofin-py`); the core crate is FFI-agnostic. |
| **D8** | Logging is deferred.  Do not suggest adding `tracing` spans or `log::debug!` calls inside hot paths. |

---

## 3. Naming conventions

QuantLib identifiers are preserved in Rust with only the mechanical transformations Rust requires:

| C++ | Rust |
|-----|------|
| `UpperCamelCase` types | keep `UpperCamelCase` |
| `lowerCamelCase` methods | convert to `snake_case` |
| `ALL_CAPS` constants | keep `UPPER_SNAKE_CASE` |
| Type aliases (`Real`, `Rate`, `Time`, …) | keep the QuantLib names — do **not** replace with `f64`, `i32`, etc. |

Do not suggest renaming types or methods to "feel more Rustic" if the existing name comes directly
from QuantLib; the name correspondence is a feature, not a defect.

---

## 4. Numeric types

Always use the project's semantic type aliases from `crate::types`:

```
Real, Integer, BigInteger, Natural, BigNatural, Size,
Time, DiscountFactor, Rate, Spread, Volatility, Probability, Decimal
```

Do not replace these with bare primitives (`f64`, `usize`, `i32`).  When you see a bare primitive
used where a QuantLib semantic alias exists, that is worth flagging.

---

## 5. Shared-ownership aliases

Use the aliases from `crate::shared`, never the raw smart-pointer types at call sites:

```rust
Shared<T>    // = Rc<T>
SharedMut<T> // = Rc<RefCell<T>>
WeakMut<T>   // = Weak<RefCell<T>>
```

Constructor helpers `shared(v)` and `shared_mut(v)` replace `Rc::new` / `Rc::new(RefCell::new(v))`.

Do **not** suggest switching `Shared` / `SharedMut` to `Arc` / `Mutex` — that is a future
migration controlled by D3 and touches the entire codebase at once.

---

## 6. Error-handling rules

- Use `fail!`, `require!`, `assert_ql!`, `ensure!` — not `panic!`, `unwrap()`, `expect()`, or
  `todo!()` in non-test code.
- `QlResult<T>` (`Result<T, QlError>`) is the return type for all fallible operations.
- Do not suggest adding `anyhow::Context` or converting `QlError` to a richer error type — the
  QuantLib error model is intentionally simple.

---

## 7. PR size and structure

- PRs target ≤ 350 LOC changed (400 hard cap).  If a review surfaces scope creep, flag it as
  "out of scope for this PR" rather than requesting it inline.
- Each PR should port one coherent unit: a struct and its constructor, a set of methods, or a
  group of tests.  Tests live in the same file as the code they test (`#[cfg(test)]` module at the
  bottom).

---

## 8. Testing philosophy

- Tests port the matching QuantLib `test-suite/*.cpp` cases.  The C++ expected values are the
  ground truth; do not adjust tolerances upward without a documented reason.
- Test functions are named after the QuantLib test they correspond to where possible.
- Do not suggest removing or weakening numerical assertions to make tests pass — fix the
  implementation instead.

---

## 9. What good review feedback looks like

**Appropriate:**
- "This coefficient differs from `ql/math/distributions/normaldistribution.cpp` line 42."
- "The loop bound should be `n - 1`, matching the C++ `for (i = 0; i < n-1; ++i)`."
- "This returns a bare `f64`; should be `Real` per the type-alias conventions."
- "Missing `require!` guard that QuantLib's `QL_REQUIRE` provides at this entry point."

**Not appropriate:**
- "Consider using `Arc<Mutex<T>>` for thread safety." *(D3 is settled.)*
- "This variable name doesn't follow Rust conventions; rename to `foo_bar`." *(Name mirrors C++.)*
- "Use `anyhow` for richer error context." *(D4 is settled.)*
- "Add `async` support." *(D6 is settled.)*
- "Replace `Rc<RefCell<T>>` with `SharedMut<T>`… wait, they're the same thing." *(Use the alias.)*
- "This algorithm could be simplified." *(Only if simplification preserves exact numerical output.)*
