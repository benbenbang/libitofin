Python 3.14.8 | arm64 | 28 CPUs | QuantLib 1.44 | itofin 0.45.0
scale=10.0 workers=28 repeat=5

== 0. Correctness ==
European max |diff| over 2000 strikes: NPV 1.07e-14 | delta 6.66e-16
Heston sigma: itofin 0.000120 | QuantLib 0.000120
Heston v0   : itofin 0.010000 | QuantLib 0.010000
Heston theta: itofin 0.010000 | QuantLib 0.010000

== 1. Single-thread European (200,000 options/run) ==
itofin        2.43 us/option |    411,318 options/s | CV  1.6%
QuantLib     20.61 us/option |     48,523 options/s | CV  0.6%

== 2. Heston calibration (35 helpers, order 192) ==
itofin         80.7 ms/calibration | CV  2.5%
QuantLib      135.7 ms/calibration | CV  3.1%

== 3. 28 threads x 200,000 options (isolated process) ==
itofin       410,893 options/s | speed-up vs 1 thread 1.00x
QuantLib      47,535 options/s | speed-up vs 1 thread 0.98x

== 4. 28 processes x 200,000 options ==
itofin     7,491,552 options/s | speed-up vs 1 process 18.21x
QuantLib     993,496 options/s | speed-up vs 1 process 20.47x

== 5. Memory soak (50 x 50,000 options, fresh process) ==
itofin    peak RSS    63.3 MB ->    63.3 MB (growth +0.0 MB)
QuantLib  peak RSS    63.1 MB ->    63.1 MB (growth +0.0 MB)

== 6a. European, market built once (200,000 options) ==
itofin        1.16 us/option | CV  3.2%
QuantLib      3.71 us/option | CV  2.3%

== 6b. Reprice one option at 200,000 new spots ==
Checksum |diff| over 1,000 spots: 9.09e-13
itofin        2.03 us/reprice (new process) | CV  2.3%
QuantLib      1.85 us/reprice (bump SimpleQuote) | CV  0.8%

== 7. Curve bootstrap (3 deposits + 9 swaps, log-linear discount) ==
Max |diff| in discount factors over 59 times: 3.33e-16
itofin       1218.7 us/build+bootstrap | CV  2.4%
QuantLib      753.2 us/build+bootstrap | CV  3.4%
itofin        708.0 us/bump+re-bootstrap | CV  0.8%
QuantLib      244.2 us/bump+re-bootstrap | CV  0.7%

== 8. Swaption sweep, Jamshidian (20,000 strikes) ==
K=3% NPV: itofin 1.5666103956 | QuantLib 1.5666103956
Max |diff| over 200 strikes: 2.61e-13
itofin       16.59 us/swaption | CV  1.2%
QuantLib     18.19 us/swaption | CV  1.3%

== Summary (itofin advantage; > 1.00x means itofin is faster) ==
European, rebuild all       8.48x
European, market reused     3.20x
Reprice at new spot         0.91x
Multi-thread European       8.64x
Multi-process European      7.54x
Swaption sweep              1.10x
Heston calibration          1.68x
Curve bootstrap             0.62x
Curve bump + re-bootstrap   0.34x
