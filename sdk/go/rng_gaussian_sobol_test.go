package itofin

import (
	"math"
	"reflect"
	"sync"
	"testing"
)

func TestGaussianSobolSequence(t *testing.T) {
	s := rngSession(t)
	source := mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 6, Seed: 42}))
	g := mustRNG(s.NewGaussianLowDiscrepancySequenceGenerator(source))
	if mustRNG(g.Dimension()) != 6 {
		t.Fatal("dimension")
	}
	zeros := make([]float64, 6)
	if !reflect.DeepEqual(mustRNG(g.LastSequence()), zeros) || !reflect.DeepEqual(mustRNG(g.NextSequence()), zeros) {
		t.Fatal("initial values")
	}
	mustRNG(source.NextSequence())
	for range 30 {
		uniforms, gaussian := mustRNG(source.NextSequence()), mustRNG(g.NextSequence())
		for i, z := range gaussian {
			if !rngClose(.5*math.Erfc(-z/math.Sqrt2), uniforms[i], 1e-8) {
				t.Fatal(z, uniforms[i])
			}
		}
		if !reflect.DeepEqual(gaussian, mustRNG(g.LastSequence())) {
			t.Fatal("last values")
		}
	}
	clone := mustRNG(s.NewGaussianLowDiscrepancySequenceGenerator(source))
	mirror := mustRNG(s.NewGaussianLowDiscrepancySequenceGenerator(source))
	source.Close()
	batch := mustRNG(clone.NextSequences(10))
	for i := range 10 {
		if !reflect.DeepEqual(batch[i*6:(i+1)*6], mustRNG(mirror.NextSequence())) {
			t.Fatal("batch or source copy")
		}
	}
	if _, err := clone.NextSequences(-1); err == nil {
		t.Fatal("negative count")
	}
	if len(mustRNG(clone.NextSequences(0))) != 0 {
		t.Fatal("empty batch")
	}
	if _, err := s.NewGaussianLowDiscrepancySequenceGenerator(nil); err == nil {
		t.Fatal("nil source")
	}
	other := rngSession(t)
	source = mustRNG(s.NewSobolRsg(SobolConfig{Dimension: 1}))
	if _, err := other.NewGaussianLowDiscrepancySequenceGenerator(source); err != ErrSessionMismatch {
		t.Fatal(err)
	}
	mustRNG(source.SkipTo(math.MaxUint32 - 1))
	last := mustRNG(s.NewGaussianLowDiscrepancySequenceGenerator(source))
	mustRNG(last.NextSequence())
	if _, err := last.NextSequence(); err == nil {
		t.Fatal("period")
	}
}
func TestRNGConcurrentCallers(t *testing.T) {
	s := rngSession(t)
	r := mustRNG(s.NewUniformRandomGenerator(42))
	want := mustRNG(s.NewUniformRandomGenerator(42))
	expected := mustRNG(want.NextReals(64))
	values := make(chan float64, 64)
	var wg sync.WaitGroup
	for range 8 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			for range 8 {
				v, err := r.NextReal()
				if err != nil {
					t.Error(err)
					return
				}
				values <- v
			}
		}()
	}
	wg.Wait()
	close(values)
	counts := map[float64]int{}
	for _, v := range expected {
		counts[v]++
	}
	for v := range values {
		counts[v]--
	}
	for v, count := range counts {
		if count != 0 {
			t.Fatal("lost or repeated draw", v, count)
		}
	}
}
