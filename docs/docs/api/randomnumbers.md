# Random numbers

Pseudo-random generators, mirroring QuantLib's `ql/math/randomnumbers/`.
The class names follow QuantLib's Python API, so `UniformRandomGenerator` is the
Mersenne Twister and `UniformRandomSequenceGenerator` the sequence generator over it.
`GaussianRandomSequenceGenerator` is the pseudo-random policy the Monte Carlo engines
draw from. Draws come back as `float`s or NumPy arrays rather than weighted samples,
and the `next_sequences(count)` methods draw a whole `(count, dimension)` matrix in
one call.

::: itofin.randomnumbers
