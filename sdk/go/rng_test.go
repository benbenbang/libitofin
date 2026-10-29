package itofin

import (
	"errors"
	"slices"
	"testing"
)

func rngSession(t *testing.T) *Session {
	t.Helper()
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() {
		if err := s.Close(); err != nil {
			t.Error(err)
		}
	})
	return s
}
func mustRNG[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}
func TestScalarRNGOracles(t *testing.T) {
	s := rngSession(t)
	words := mustRNG(s.UniformRandomGeneratorFromSeeds([]uint32{0x123, 0x234, 0x345, 0x456}))
	for _, want := range []uint32{1067595299, 955945823, 477289528, 4107218783, 4228976476, 3344332714, 3355579695, 227628506, 810200273, 2591290167} {
		if got := mustRNG(words.NextU32()); got != want {
			t.Fatalf("word %d want %d", got, want)
		}
	}
	a := mustRNG(s.NewUniformRandomGenerator(5489))
	for range 9999 {
		mustRNG(a.NextU32())
	}
	if got := mustRNG(a.NextU32()); got != 4123659995 {
		t.Fatal(got)
	}
	a = mustRNG(s.NewUniformRandomGenerator(42))
	b := mustRNG(s.NewUniformRandomGenerator(42))
	for _, got := range mustRNG(b.NextReals(64)) {
		want := (float64(mustRNG(a.NextU32())) + .5) / 4294967296
		if got != want || got <= 0 || got >= 1 {
			t.Fatal(got, want)
		}
	}
	a = mustRNG(s.NewUniformRandomGenerator(42))
	g := mustRNG(s.NewGaussianRandomGenerator(a))
	seeded := mustRNG(s.GaussianRandomGeneratorWithSeed(42))
	want := []float64{-.51696416445487181, 1.2219212173764127, .72133261267083881, .86963581617716534, 1.6182168832131514, 1.5885563656499377, -1.1883085743351087, -.18712466949524548}
	for i, got := range mustRNG(g.NextGaussians(len(want))) {
		if !rngClose(got, want[i], 1e-15) {
			t.Fatal(got, want[i])
		}
	}
	for _, v := range want {
		if got := mustRNG(seeded.NextGaussian()); !rngClose(got, v, 1e-15) {
			t.Fatal(got, v)
		}
	}
	mirror := mustRNG(s.NewUniformRandomGenerator(42))
	if mustRNG(a.NextReal()) != mustRNG(mirror.NextReal()) {
		t.Fatal("constructor advanced source")
	}
	if err := a.Close(); err != nil {
		t.Fatal(err)
	}
	mustRNG(g.NextGaussian())
}
func TestScalarRNGLifecycle(t *testing.T) {
	s, other := rngSession(t), rngSession(t)
	a := mustRNG(s.NewUniformRandomGenerator(7))
	if _, err := s.UniformRandomGeneratorFromSeeds(nil); err == nil {
		t.Fatal("empty seeds")
	}
	if _, err := s.NewGaussianRandomGenerator(nil); err == nil {
		t.Fatal("nil source")
	}
	if _, err := other.NewGaussianRandomGenerator(a); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	for _, n := range []int{-1, DefaultMaxOutputValues + 1, int(^uint(0) >> 1)} {
		if _, err := a.NextReals(n); err == nil {
			t.Fatal("invalid count", n)
		}
	}
	if got := mustRNG(a.NextReals(0)); len(got) != 0 {
		t.Fatal(got)
	}
	mirror := mustRNG(s.NewUniformRandomGenerator(7))
	if mustRNG(a.NextReal()) != mustRNG(mirror.NextReal()) {
		t.Fatal("empty or invalid batch advanced state")
	}
	copy := *a
	if err := a.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := copy.NextReal(); err == nil {
		t.Fatal("closed copy")
	}
	if err := copy.Close(); err != nil {
		t.Fatal(err)
	}
	s.Close()
	if _, err := mirror.NextReal(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}

func TestRandomSeedIsReportedAndReplays(t *testing.T) {
	s := rngSession(t)
	random := mustRNG(s.NewUniformRandomGenerator(0))
	seed := mustRNG(random.Seed())
	if seed == 0 {
		t.Fatal("seed zero must resolve to a reported nonzero seed")
	}
	replay := mustRNG(s.NewUniformRandomGenerator(seed))
	if got := mustRNG(replay.Seed()); got != seed {
		t.Fatal(got, seed)
	}
	for range 1000 {
		if a, b := mustRNG(random.NextU32()), mustRNG(replay.NextU32()); a != b {
			t.Fatal(a, b)
		}
	}
	if other := mustRNG(mustRNG(s.NewUniformRandomGenerator(0)).Seed()); other == seed {
		t.Fatal("successive random seeds must differ")
	}
	if got := mustRNG(mustRNG(s.UniformRandomGeneratorFromSeeds([]uint32{1, 2})).Seed()); got != 0 {
		t.Fatal(got)
	}
	gaussian := mustRNG(s.GaussianRandomGeneratorWithSeed(0))
	gaussianSeed := mustRNG(gaussian.Seed())
	gaussianReplay := mustRNG(s.GaussianRandomGeneratorWithSeed(gaussianSeed))
	if a, b := mustRNG(gaussian.NextGaussians(64)), mustRNG(gaussianReplay.NextGaussians(64)); gaussianSeed == 0 || !slices.Equal(a, b) {
		t.Fatal(gaussianSeed)
	}
	sequence := mustRNG(s.GaussianRandomSequenceGeneratorWithSeed(3, 0))
	sequenceSeed := mustRNG(sequence.Seed())
	sequenceReplay := mustRNG(s.GaussianRandomSequenceGeneratorWithSeed(3, sequenceSeed))
	if a, b := mustRNG(sequence.NextSequences(8)), mustRNG(sequenceReplay.NextSequences(8)); sequenceSeed == 0 || !slices.Equal(a, b) {
		t.Fatal(sequenceSeed)
	}
	if got := mustRNG(mustRNG(s.UniformRandomSequenceGeneratorWithSeed(2, 42)).Seed()); got != 42 {
		t.Fatal(got)
	}
	poisson := mustRNG(s.NewPoissonRandomSequenceGenerator(4, 0, 2))
	poissonSeed := mustRNG(poisson.Seed())
	poissonReplay := mustRNG(s.NewPoissonRandomSequenceGenerator(4, poissonSeed, 2))
	if a, b := mustRNG(poisson.NextSequence()), mustRNG(poissonReplay.NextSequence()); poissonSeed == 0 || !slices.Equal(a, b) {
		t.Fatal(poissonSeed)
	}
	if got := mustRNG(mustRNG(s.NewPoissonRandomGenerator(7, 1)).Seed()); got != 7 {
		t.Fatal(got)
	}
}
