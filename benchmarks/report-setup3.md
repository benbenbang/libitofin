Python 3.14.8 | arm64 | 28 CPUs | QuantLib 1.44 | itofin 0.45.0
scale=1.0 workers=28 repeat=5

== 0. Correctness ==
European max |diff| over 2000 strikes: NPV 1.07e-14 | delta 6.66e-16
Heston sigma: itofin 0.000120 | QuantLib 0.000120
Heston v0   : itofin 0.010000 | QuantLib 0.010000
Heston theta: itofin 0.010000 | QuantLib 0.010000

== 1. Single-thread European (20,000 options/run) ==
itofin        2.41 us/option |    414,736 options/s | CV  2.7%
QuantLib     20.90 us/option |     47,849 options/s | CV  1.7%

== 2. Heston calibration (35 helpers, order 192) ==
itofin         82.1 ms/calibration | CV  1.6%
QuantLib      135.0 ms/calibration | CV  1.3%

== 3. 28 threads x 20,000 options (isolated process) ==
itofin       414,834 options/s | speed-up vs 1 thread 1.00x
QuantLib      47,647 options/s | speed-up vs 1 thread 1.00x

== 4. 28 processes x 20,000 options ==
itofin     4,810,117 options/s | speed-up vs 1 process 11.60x
QuantLib     867,359 options/s | speed-up vs 1 process 18.13x

== 5. Memory soak (50 x 5,000 options, fresh process) ==
itofin    peak RSS    61.7 MB ->    61.8 MB (growth +0.1 MB)
QuantLib  peak RSS    61.6 MB ->    61.7 MB (growth +0.1 MB)

== 6a. European, market built once (20,000 options) ==
itofin        1.07 us/option | CV  1.3%
QuantLib      3.52 us/option | CV  3.3%

== 6b. Reprice one option at 20,000 new spots ==
Checksum |diff| over 1,000 spots: 9.09e-13
itofin        2.06 us/reprice (new process) | CV  3.4%
QuantLib      1.75 us/reprice (bump SimpleQuote) | CV  2.1%

== 7. Curve bootstrap (3 deposits + 9 swaps, log-linear discount) ==
Max |diff| in discount factors over 59 times: 3.33e-16
itofin       1253.1 us/build+bootstrap | CV  4.3%
QuantLib      726.7 us/build+bootstrap | CV  1.5%
itofin        720.6 us/bump+re-bootstrap | CV  1.9%
QuantLib      251.5 us/bump+re-bootstrap | CV  2.8%

== 8. Swaption sweep, Jamshidian (2,000 strikes) ==
K=3% NPV: itofin 1.5666103956 | QuantLib 1.5666103956
Max |diff| over 200 strikes: 2.61e-13
itofin       16.78 us/swaption | CV  2.9%
QuantLib     18.24 us/swaption | CV  1.7%

== Summary (itofin advantage; > 1.00x means itofin is faster) ==
European, rebuild all       8.67x
European, market reused     3.29x
Reprice at new spot         0.85x
Multi-thread European       8.71x
Multi-process European      5.55x
Swaption sweep              1.09x
Heston calibration          1.64x
Curve bootstrap             0.58x
Curve bump + re-bootstrap   0.35x
