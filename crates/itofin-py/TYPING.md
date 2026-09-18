# Python typing acceptance

Run from the repository root in a Python 3.10+ virtual environment:

```sh
python -m pip install -r crates/itofin-py/typing-requirements.txt
python -m unittest discover -s crates/itofin-py/scripts -p 'test_*.py'
python crates/itofin-py/scripts/check_typing.py
```

CI and prek run the same checks. Pyright 1.1.411 checks all binding tests using
Python 3.10 semantics and the checkout's stubs, without requiring a native build.
The pinned pytest and NumPy dependencies make imported types reproducible.

The gate compares diagnostic **multisets in both directions**, including repeated
occurrences. Identity is repository-relative file, severity, rule, and the full
message with whitespace normalized. Paths resolve symlinks; absolute checkout
paths and line/column movement do not affect identity. A changed message is both
a new diagnostic and a disappeared diagnostic. Counts alone never establish a pass.

## Reviewing changes

- New diagnostics fail. Fix the regression or deliberately add the exact
  diagnostic and occurrence count to `typing-baseline.json`, with review evidence.
- Disappearances fail too. Keep the original diagnostic and add an
  `accepted_removals` entry using the ID printed by the checker, its count, a
  reason, evidence, and classification: `accuracy-improvement` or
  `accepted-precision-loss`. The latter also requires a filed issue URL in
  `follow_up`. Do not delete baseline entries or silently regenerate the baseline.
- An accepted removal returning is a new diagnostic and fails. To restore it
  deliberately, remove its acceptance entry in the same reviewed change.
- Changing Pyright or dependency versions requires reviewing both sides of the
  resulting diagnostic diff. `--report FILE` compares saved Pyright JSON offline;
  mismatched Pyright versions and malformed records fail closed.

## Historical evidence: #625 / #993

The original 16 errors and 2 warnings were reproduced from `1bb39b61` with
Pyright 1.1.411. Comparing `99111f24` retains 7 errors and 2 warnings. The JSON
names all nine accepted disappearances: helper-list invariance in credit
bootstrap, ISDA Markit/reconciliation, piecewise yield, YoY cap/floor, YoY leg,
YoY repricing, YoY optionlet volatility, and ZC repricing. Generated `Sequence`
parameters correctly admit subtype lists accepted at runtime.

The tenth temporary disappearance was **not accepted**: the invalid
`Date - "a week"` diagnostic must remain. Restoring typed subtraction overloads
in `99111f24` recovered it. [Issue #993 records the source and runtime evidence](https://github.com/benbenbang/libitofin/issues/993#issuecomment-5565178722).
