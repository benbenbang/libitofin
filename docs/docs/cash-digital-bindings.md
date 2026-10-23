# European cash-or-nothing options

The additive factory exposes the existing cash-or-nothing payoff through the
ordinary option facades in Python and C/Go. Existing vanilla constructors and
signatures are unchanged.

| Language | Constructor | Analytic engine |
| --- | --- | --- |
| Python | `VanillaOption.cash_or_nothing(option_type, strike, cash_payoff, expiry, settings)` | `option.set_engine(process)` or `option.price(process)` |
| Go | `session.NewCashOrNothingOption(kind, strike, cashPayoff, expiry, settings)` | `option.SetEngine(process)` or `option.Price(process)` |
| C | `itofin_option_cash_or_nothing_new` | Existing `itofin_option_set_engine`, engine kind0 |

## Supported scope

- European exercise and a fixed cash amount. The raw payoff pays cash for a call
  strictly above strike or a put strictly below strike; otherwise it pays zero.
- The analytic European Black-Scholes engine, using the process's existing
  discount, dividend and volatility inputs. Values and ordinary Greeks reuse the
  existing option query methods and the original core implementation.
- Strike must be finite and nonnegative. Cash must be finite; signed and zero
  cash are accepted. Cash is a currency amount, not a probability or notional multiplier.
- The constructor retains settings. An attached engine retains its process and
  live curves/quotes, so releasing input wrappers does not destroy the option's
  dependencies. Evaluation-date and observable market changes invalidate pricing.
- C uses the existing typed, thread-confined context handles, writes the constructor
  output only on success, and requires callers to satisfy the C pointer contract.
  Go rejects foreign-session inputs and serializes native calls on its worker.
  Python retains its existing unsendable native ownership.

## Numerical and engine limits

This is a binding, not a repair to digital-option mathematics or a claim of full
QuantLib parity. Existing core behavior, including errors, is retained.

- At zero or sub-epsilon standard deviation, digital price uses the original
  deterministic/ATM-half branch. The delta/rho ladder is inherited from the
  core's plain-vanilla-style fallback, including for zero cash. These are **not
  textbook digital Greek limits**. In particular, put delta, risk-free rho and
  dividend rho differ from the independent compiled QuantLib 1.43 reference on
  the zero/sub-epsilon diagnostic grid. The raw payoff's strict strike comparison
  is distinct from the analytic zero-volatility convention.
- Non-notifying settings retain the ordinary option's cached-versus-fresh
  behavior. At evaluation date equal to expiry, changing
  `include_reference_date_events` after calculating can leave the cached value
  at zero while a freshly constructed plain/digital option has nonzero value.
  This also occurs in the native reference. Reconstructing the option or
  reattaching its engine restores the fresh result. This factory adds neither
  silent-policy synchronization nor new freeze/unfreeze methods.
- Zero strike and extreme market inputs retain the existing calculator's
  singular/nonfinite-result behavior. At zero strike, gamma, theta and vega can
  return NaN for both calls and puts, as in the independent native diagnostic.
  Constructibility does not guarantee finite Greeks or convert those inherited
  nonfinite results into exceptions.
- Analytic Heston, Merton, Bates and analytic GJR engines require a plain-vanilla
  payoff and reject this digital payoff through their existing core checks.
  Other generic MC/FD engine paths are not certified by this factory. No new
  engine compatibility, payoff visitor or numerical algorithm is introduced.
- Asset-or-nothing, gap and percentage-strike payoff factories, American digital
  exercise and the wider payoff family remain outside this slice.

## Examples

Python: [cash digital example](https://github.com/benbenbang/libitofin/blob/main/example/python/cash_digital.py).
Go: [cash digital example](https://github.com/benbenbang/libitofin/blob/main/sdk/go/examples/cash_digital/main.go).

Fixtures compare regular positive-volatility prices and Greeks to an independent
normal-distribution formula, while boundary regressions explicitly pin original
core behavior. Retained-input, date/quote invalidation and native-error probes are
separate from those numerical checks. Source contract: existing
`crates/libitofin/src/instruments/payoffs.rs`,
`pricingengines/blackcalculator.rs` and `pricingengines/vanilla/mod.rs`.

## Validation qualifications

Ordinary positive-volatility closed-form checks are separate from the known
boundary diagnostics. Original-artifact public clients passed 392 Python and 442
C checks, including 140 ordinary closed-form price/Greek fields per facade.

The separate strict native-edge diagnostic **exited 1** and retained 27 distinct
finite put-Greek differences, 54 when counted across the C and Python facades.
Its grid used volatility 0, 1e-18 and 1e-16, strikes 80/100/120, and a one-year horizon.
The six distinct zero-strike call/put gamma/theta/vega routes were nonfinite in
both facades and the reference, not successful finite-Greek comparisons. Of 210
shared C/Python edge routes, 204 were finite and bit-identical and six had matching
nonfinite classifications. Facade equivalence is not native numerical parity.

The separate strict silent-policy diagnostic **exited 1 in each facade**: cached
plain/digital values 0 differed from fresh 20/17; engine reattachment restored 20/17.
This matched the independent native reference and is retained, not repaired or
hidden by a tolerance change. The compiled QuantLib 1.43 package is a separately
qualified reference, not a binary claimed to come from the local 1.43-dev checkout.
