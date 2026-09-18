package itofin

import (
	"math"
	"reflect"
	"testing"
)

func rngClose(got, want, tolerance float64) bool {
	return !math.IsNaN(got) && !math.IsInf(got, 0) && math.Abs(got-want) <= tolerance
}
func TestRNGSeedSemanticsAndMoments(t *testing.T) {
	s := rngSession(t)
	uniform := func(seed uint32) []float64 { return mustRNG(mustRNG(s.NewUniformRandomGenerator(seed)).NextReals(16)) }
	if !reflect.DeepEqual(uniform(7), uniform(7)) || reflect.DeepEqual(uniform(7), uniform(8)) {
		t.Fatal("nonzero uniform seeds")
	}
	if reflect.DeepEqual(uniform(0), uniform(0)) {
		t.Fatal("zero uniform seed must randomize")
	}
	sobol := func() []float64 { return mustRNG(mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 40})).NextSequences(16)) }
	if !reflect.DeepEqual(sobol(), sobol()) {
		t.Fatal("zero Sobol seed must be literal")
	}
	gauss := mustRNG(s.GaussianRandomGeneratorWithSeed(1234))
	seq := mustRNG(s.GaussianRandomSequenceGeneratorWithSeed(10, 99))
	for i, draws := range [][]float64{mustRNG(gauss.NextGaussians(100000)), mustRNG(seq.NextSequences(10000))} {
		mean, second := 0.0, 0.0
		for _, v := range draws {
			mean += v
			second += v * v
		}
		mean /= float64(len(draws))
		variance := second/float64(len(draws)) - mean*mean
		tolerance := []float64{.01, .02}[i]
		if !rngClose(mean, 0, .01) || !rngClose(variance, 1, tolerance) {
			t.Fatal("Gaussian moments", mean, variance)
		}
	}
	g := mustRNG(s.GaussianRandomGeneratorWithSeed(1234))
	mirror := mustRNG(s.GaussianRandomGeneratorWithSeed(1234))
	batch := mustRNG(g.NextGaussians(101))
	for _, value := range batch {
		if value != mustRNG(mirror.NextGaussian()) {
			t.Fatal("odd batch loses cached Gaussian")
		}
	}
	if len(mustRNG(g.NextGaussians(0))) != 0 {
		t.Fatal("empty Gaussian batch")
	}
	if _, err := g.NextGaussians(DefaultMaxOutputValues + 1); err == nil {
		t.Fatal("Gaussian batch limit")
	}
	if mustRNG(g.NextGaussian()) != mustRNG(mirror.NextGaussian()) {
		t.Fatal("empty or invalid Gaussian batch advanced state")
	}
}
func TestLowDiscrepancyBatchesAndCopies(t *testing.T) {
	s := rngSession(t)
	factory := []func() randomSequence{
		func() randomSequence { return mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 3})).randomSequence },
		func() randomSequence { return mustRNG(s.NewHaltonRsg(3)).randomSequence },
	}
	for _, makeRSG := range factory {
		a, b := makeRSG(), makeRSG()
		batch := mustRNG(a.NextSequences(20))
		for row := range 20 {
			if !reflect.DeepEqual(batch[row*3:(row+1)*3], mustRNG(b.NextSequence())) {
				t.Fatal("batch ordering")
			}
		}
		copy := a
		a.Close()
		if _, err := copy.NextSequence(); err == nil {
			t.Fatal("closed copied sequence")
		}
		if err := copy.Close(); err != nil {
			t.Fatal(err)
		}
	}
	defaultRSG := mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 16}))
	table := Jaeckel
	explicit := mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 16, DirectionIntegers: &table}))
	if !reflect.DeepEqual(mustRNG(defaultRSG.NextSequences(32)), mustRNG(explicit.NextSequences(32))) {
		t.Fatal("default direction table")
	}
}
