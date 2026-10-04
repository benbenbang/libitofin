package itofin

import (
	"math"
	"testing"
)

func sequenceClose(t *testing.T, got, want []float64, err error) {
	t.Helper()
	if err != nil {
		t.Fatal(err)
	}
	if len(got) != len(want) {
		t.Fatalf("got length %d, want %d", len(got), len(want))
	}
	for i := range got {
		if math.IsNaN(got[i]) || math.Abs(got[i]-want[i]) > 1e-13 {
			t.Fatalf("coordinate %d: got %.17g, want %.17g", i, got[i], want[i])
		}
	}
}
