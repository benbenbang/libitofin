# Conditional exotic-option scope

[Request #1173](https://github.com/benbenbang/libitofin/issues/1173) records
scope for aquor, not blanket authorization to implement exotic pricing.
The following audit uses source checkpoint `2fb97c85` and QuantLib
`9863b578af0caa4cecabf697196533e84a8308b6`.

## Current disposition

| Family | Concrete Rust/C/Go/Python pricing | Disposition |
| --- | --- | --- |
| Barrier | Not implemented | Deferred pending a named consumer contract |
| Asian | Not implemented | Deferred pending a named consumer contract |
| Lookback options | Not implemented | Deferred pending a named consumer contract |
| Basket | Not implemented | Deferred pending a named consumer contract |

No family is selected or owner-dropped. Existing vanilla engines, correlated
processes and path generators do not constitute exotic instrument or engine
support. Overnight-index lookback conventions are unrelated to lookback options.

## Before activating an implementation

A consumer-backed child ticket must specify:

- Payoff and option family, including strike, rebate or averaging convention.
- Exercise dates and settlement rules.
- Continuous or discrete monitoring, observation dates and historical fixings.
- A concrete pricing engine, market/process requirements and explicit exclusions.
- Pinned native fixtures plus independent limiting cases and numerical tolerances.
- Rust/C/Go/Python construction, results, errors, ownership and live-update tests.

Examples of bounded contracts, not selected work:

| Family | Candidate boundary | Important exclusions or decisions |
| --- | --- | --- |
| Barrier | Single European barrier with an analytic engine | Barrier direction, knock type, monitoring and rebate timing |
| Asian | Discrete geometric-average European option | Observation schedule, past fixings and average-price versus average-strike |
| Lookback | Continuous fixed-strike European option | Historical extremum, monitoring and floating-strike payoff |
| Basket | Two-asset European spread with a named approximation | Correlation limits, approximation error and no generic basket/early exercise claim |

## Backlog ownership

Selected children must coordinate with
[instrument epic #70](https://github.com/benbenbang/libitofin/issues/70) and
[pricing epic #73](https://github.com/benbenbang/libitofin/issues/73).
Those broader epics remain open. Recording pending-demand dispositions does not
deliver an exotic API, activate an implementation child or establish family parity.
