package itofin

import (
	"math"
	"reflect"
	"testing"
)

func ouConfig() OUConfig {
	return OUConfig{Initial: 1, Level: 3, Speed: .5, Volatility: .2, Horizon: 1, Steps: 4, Paths: 3, Seed: 42}
}

func TestOUSeededPathsMatchPinnedMultistepFixture(t *testing.T) {
	want := []float64{
		1, 1.2049197140571397, 1.4938576175387845, 1.8262101587088548, 1.879255526298315,
		1, 1.2932179159179402, 1.5663072780654845, 1.758274886469132, 1.9272460691202662,
		1, 1.139911950142801, 1.3456668679224093, 1.4449524117278607, 1.50711446963386,
	}
	c := ouConfig()
	full, err := SimulateOU(c)
	if err != nil {
		t.Fatal(err)
	}
	if len(full.Values) != len(want) {
		t.Fatalf("got %d values, want %d", len(full.Values), len(want))
	}
	for i, value := range full.Values {
		if !(math.Abs(value-want[i]) < 2e-15) {
			t.Fatalf("value %d: %.17g, want %.17g", i, value, want[i])
		}
	}
	c.TerminalOnly = true
	terminal, err := SimulateOU(c)
	if err != nil {
		t.Fatal(err)
	}
	if len(terminal.Values) != c.Paths {
		t.Fatalf("got %d terminals, want %d", len(terminal.Values), c.Paths)
	}
	for path, value := range terminal.Values {
		if !(math.Abs(value-want[path*5+4]) < 2e-15) || value != full.Values[path*5+4] {
			t.Fatalf("terminal %d: %.17g, want %.17g", path, value, want[path*5+4])
		}
	}
}

func TestOULayoutAndTerminalConsistency(t *testing.T) {
	c := ouConfig()
	full, err := SimulateOU(c)
	if err != nil {
		t.Fatal(err)
	}
	if full.Paths != 3 || full.Times != 5 || full.Assets != 1 || len(full.Values) != 15 {
		t.Fatalf("layout: %+v", full)
	}
	for path := 0; path < c.Paths; path++ {
		if full.Values[path*5] != c.Initial {
			t.Fatal("time zero changed")
		}
	}
	draws, err := GaussianDraws(1, c.Seed)
	if err != nil {
		t.Fatal(err)
	}
	dt := c.Horizon / float64(c.Steps)
	want := c.Level + (c.Initial-c.Level)*math.Exp(-c.Speed*dt) + c.Volatility*math.Sqrt((1-math.Exp(-2*c.Speed*dt))/(2*c.Speed))*draws[0]
	if math.Abs(full.Values[1]-want) > 1e-15 {
		t.Fatalf("first transition %g, want %g", full.Values[1], want)
	}
	c.TerminalOnly = true
	terminal, err := SimulateOU(c)
	if err != nil {
		t.Fatal(err)
	}
	for path, value := range terminal.Values {
		if value != full.Values[path*5+4] {
			t.Fatal("terminal differs from full path")
		}
	}
	again, err := SimulateOU(c)
	if err != nil || !reflect.DeepEqual(terminal.Values, again.Values) {
		t.Fatal("seed changed path")
	}
}

func TestOUZeroHorizonAndInvalidInputs(t *testing.T) {
	c := ouConfig()
	c.Horizon = 0
	result, err := SimulateOU(c)
	if err != nil {
		t.Fatal(err)
	}
	for _, value := range result.Values {
		if value != c.Initial {
			t.Fatal("zero horizon changed state")
		}
	}
	c = ouConfig()
	c.Speed = -1
	if _, err := SimulateOU(c); err == nil {
		t.Fatal("accepted negative speed")
	}
	c = ouConfig()
	c.Seed = 0
	if _, err := SimulateOU(c); err == nil {
		t.Fatal("accepted zero seed")
	}
	c = ouConfig()
	c.MaxOutputValues = 2
	if _, err := SimulateOU(c); err == nil {
		t.Fatal("ignored output limit")
	}
}
