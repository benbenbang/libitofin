package itofin

import (
	"math"
	"reflect"
	"testing"
)

func TestIntervalPrices(t *testing.T) {
	d1 := sessionMust(NewDate(1, 1, 2026))
	d2 := sessionMust(NewDate(2, 1, 2026))
	d3 := sessionMust(NewDate(3, 1, 2026))
	dates := []Date{d3, d1, d3, d2}
	open := []float64{10, 1, -2, 5}
	high := []float64{11, 2, -1, 6}
	low := []float64{9, 0, -3, 4}
	close := []float64{10, 1, -2, 5}
	wantDates := append([]Date(nil), dates...)
	wantOpen := append([]float64(nil), open...)
	wantHigh := append([]float64(nil), high...)
	wantLow := append([]float64(nil), low...)
	wantClose := append([]float64(nil), close...)
	got, err := IntervalPrices(dates, open, high, low, close)
	if err != nil {
		t.Fatal(err)
	}
	want := []DatedIntervalPrice{
		{Date: d1, Open: 1, High: 2, Low: 0, Close: 1},
		{Date: d2, Open: 5, High: 6, Low: 4, Close: 5},
		{Date: d3, Open: -2, High: -1, Low: -3, Close: -2},
	}
	if !reflect.DeepEqual(got, want) {
		t.Fatalf("normalized prices = %+v, want %+v", got, want)
	}
	if !reflect.DeepEqual(dates, wantDates) || !reflect.DeepEqual(open, wantOpen) ||
		!reflect.DeepEqual(high, wantHigh) || !reflect.DeepEqual(low, wantLow) ||
		!reflect.DeepEqual(close, wantClose) {
		t.Fatal("IntervalPrices mutated its inputs")
	}
	got, err = IntervalPrices(nil, nil, nil, nil, nil)
	if err != nil || len(got) != 0 {
		t.Fatalf("empty prices = %+v, %v", got, err)
	}
	for _, values := range [][4][]float64{
		{nil, nil, nil, nil},
		{{1}, nil, nil, nil},
		{nil, {1}, nil, nil},
		{nil, nil, {1}, nil},
		{nil, nil, nil, {1}},
	} {
		_, err := IntervalPrices([]Date{d1}, values[0], values[1], values[2], values[3])
		if err == nil {
			t.Fatalf("accepted mismatched lengths: %+v", values)
		}
	}
	for _, tc := range []struct {
		name       string
		date       Date
		o, h, l, c float64
	}{
		{"zero date", Date{}, 1, 2, 0, 1},
		{"out of range date", Date{serial: math.MaxInt32}, 1, 2, 0, 1},
		{"high below low", d1, 1, 0, 2, 1},
		{"open above high", d1, 3, 2, 0, 1},
		{"close below low", d1, 1, 2, 0, -1},
		{"nonfinite open", d1, math.NaN(), 2, 0, 1},
		{"nonfinite high", d1, 1, math.Inf(1), 0, 1},
		{"nonfinite low", d1, 1, 2, math.Inf(-1), 1},
		{"nonfinite close", d1, 1, 2, 0, math.NaN()},
	} {
		t.Run(tc.name, func(t *testing.T) {
			if _, err := IntervalPrices([]Date{tc.date}, []float64{tc.o}, []float64{tc.h}, []float64{tc.l}, []float64{tc.c}); err == nil {
				t.Fatal("accepted invalid interval price")
			}
		})
	}
}
