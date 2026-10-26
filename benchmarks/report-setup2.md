--- default ---
Python 3.14.8 | arm64 | 28 CPUs | QuantLib 1.44 | itofin 0.45.0
scale=1.0 workers=28 repeat=5

== 0. Correctness ==
European max |diff| over 2000 strikes: NPV 1.07e-14 | delta 6.66e-16
Heston sigma: itofin 0.000120 | QuantLib 0.000120
Heston v0   : itofin 0.010000 | QuantLib 0.010000
Heston theta: itofin 0.010000 | QuantLib 0.010000

== 1. Single-thread European (20,000 options/run) ==
itofin        2.26 us/option |    442,077 options/s | CV  1.2%
QuantLib     21.28 us/option |     47,003 options/s | CV  2.4%

== 2. Heston calibration (35 helpers, order 192) ==
itofin         78.2 ms/calibration | CV  0.0%
QuantLib      137.5 ms/calibration | CV  1.8%

== 3. 28 threads x 20,000 options (isolated process) ==
itofin       410,323 options/s | speed-up vs 1 thread 0.93x
QuantLib      47,891 options/s | speed-up vs 1 thread 1.02x

== 4. 28 processes x 20,000 options ==
itofin     4,250,443 options/s | speed-up vs 1 process 9.61x
QuantLib     911,306 options/s | speed-up vs 1 process 19.39x

== 5. Memory soak (50 x 5,000 options, fresh process) ==
itofin    peak RSS    61.7 MB ->    61.8 MB (growth +0.1 MB)
QuantLib  peak RSS    61.7 MB ->    61.8 MB (growth +0.1 MB)

== Summary (itofin advantage; > 1.00x means itofin is faster) ==
Single-thread European   9.41x
Multi-thread European    8.57x
Multi-process European   4.66x
Heston calibration       1.76x

--- scale to 3 ---
Python 3.14.8 | arm64 | 28 CPUs | QuantLib 1.44 | itofin 0.45.0
scale=3.0 workers=28 repeat=5

== 0. Correctness ==
European max |diff| over 2000 strikes: NPV 1.07e-14 | delta 6.66e-16
Heston sigma: itofin 0.000120 | QuantLib 0.000120
Heston v0   : itofin 0.010000 | QuantLib 0.010000
Heston theta: itofin 0.010000 | QuantLib 0.010000

== 1. Single-thread European (60,000 options/run) ==
itofin        2.38 us/option |    420,093 options/s | CV  2.3%
QuantLib     20.76 us/option |     48,178 options/s | CV  1.0%

== 2. Heston calibration (35 helpers, order 192) ==
itofin         79.8 ms/calibration | CV  1.5%
QuantLib      138.9 ms/calibration | CV  1.4%

== 3. 28 threads x 60,000 options (isolated process) ==
itofin       412,647 options/s | speed-up vs 1 thread 0.98x
QuantLib      47,437 options/s | speed-up vs 1 thread 0.98x

== 4. 28 processes x 60,000 options ==
itofin     6,157,931 options/s | speed-up vs 1 process 14.66x
QuantLib     961,115 options/s | speed-up vs 1 process 19.95x

== 5. Memory soak (50 x 15,000 options, fresh process) ==
itofin    peak RSS    62.2 MB ->    62.2 MB (growth +0.0 MB)
QuantLib  peak RSS    62.1 MB ->    62.1 MB (growth +0.0 MB)

== Summary (itofin advantage; > 1.00x means itofin is faster) ==
Single-thread European   8.72x
Multi-thread European    8.70x
Multi-process European   6.41x
Heston calibration       1.74x
