 uv run qt-bench
Python 3.13.14 | arm64 | QuantLib 1.44 | itofin 0.45.0

== Correctness ==
European K=65 NPV: itofin 2.1333684449 | QuantLib 2.1333684449
Max |diff| over 1000 strikes: NPV 1.07e-14, delta 6.66e-16
Heston sigma: itofin 0.000216 | QuantLib 0.000216
Heston v0   : itofin 0.010000 | QuantLib 0.010000
Heston theta: itofin 0.010000 | QuantLib 0.010000

== Speed (median of 7 runs, lower is better) ==
European, per option : itofin     2.09 us | QuantLib    18.07 us | ratio QL/itofin  8.63x
Heston calibration   : itofin     7.51 ms | QuantLib    13.52 ms | ratio QL/itofin  1.80x

ratio > 1 means itofin is faster.

---
 uv run qt-bench
Python 3.13.14 | arm64 | QuantLib 1.44 | itofin 0.45.0

== Correctness ==
European K=65 NPV: itofin 2.1333684449 | QuantLib 2.1333684449
Max |diff| over 1000 strikes: NPV 1.07e-14, delta 6.66e-16
Heston sigma: itofin 0.000216 | QuantLib 0.000216
Heston v0 : itofin 0.010000 | QuantLib 0.010000
Heston theta: itofin 0.010000 | QuantLib 0.010000

== Speed (median of 7 runs, lower is better) ==
European, per option : itofin 2.07 us | QuantLib 18.32 us | ratio QL/itofin 8.86x
Heston calibration : itofin 7.52 ms | QuantLib 13.25 ms | ratio QL/itofin 1.76x

ratio > 1 means itofin is faster.

---
 uv run qt-bench
Python 3.13.14 | arm64 | QuantLib 1.44 | itofin 0.45.0

== Correctness ==
European K=65 NPV: itofin 2.1333684449 | QuantLib 2.1333684449
Max |diff| over 1000 strikes: NPV 1.07e-14, delta 6.66e-16
Heston sigma: itofin 0.000216 | QuantLib 0.000216
Heston v0 : itofin 0.010000 | QuantLib 0.010000
Heston theta: itofin 0.010000 | QuantLib 0.010000

== Speed (median of 7 runs, lower is better) ==
European, per option : itofin 2.06 us | QuantLib 18.42 us | ratio QL/itofin 8.93x
Heston calibration : itofin 7.50 ms | QuantLib 13.24 ms | ratio QL/itofin 1.77x

ratio > 1 means itofin is faster.
