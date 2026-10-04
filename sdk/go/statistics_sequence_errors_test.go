package itofin

import (
	"math"
	"testing"
)

func TestStatisticsSequenceRejectsInvalidInputs(t *testing.T) {
	for _, samples := range [][][]float64{
		nil, {}, {nil}, {{1}, {1, 2}}, {{1}, {math.NaN()}},
		{{1}, {math.Inf(1)}}, {{1}, {math.Inf(-1)}},
	} {
		if got, err := StatisticsSequenceMean(samples, nil); err == nil || got != nil {
			t.Fatalf("accepted samples %v: %v, %v", samples, got, err)
		}
	}
	for _, weights := range [][]float64{
		{}, {1}, {-1, 2}, {math.NaN(), 1}, {math.Inf(1), 1},
		{0, 0}, {math.MaxFloat64, math.MaxFloat64},
	} {
		if got, err := StatisticsSequenceMean([][]float64{{1}, {2}}, weights); err == nil || got != nil {
			t.Fatalf("accepted weights %v: %v, %v", weights, got, err)
		}
	}
	if _, err := StatisticsSequenceMean([][]float64{{1}, {math.NaN()}}, []float64{1, 0}); err == nil {
		t.Fatal("accepted nonfinite value in a zero-weight row")
	}
	for _, operation := range []func([][]float64, []float64) ([]float64, error){
		StatisticsSequenceVariance, StatisticsSequenceStandardDeviation, StatisticsSequenceErrorEstimate,
	} {
		if got, err := operation([][]float64{{1, 2}}, nil); err == nil || got != nil {
			t.Fatal("accepted one row for dispersion")
		}
	}
	for _, operation := range []func([][]float64, []float64) ([][]float64, error){StatisticsCovariance, StatisticsCorrelation} {
		for _, samples := range [][][]float64{{{1, 2}}, {{math.MaxFloat64, 1}, {-math.MaxFloat64, 2}}, nil} {
			if got, err := operation(samples, nil); err == nil || got != nil {
				t.Fatal("accepted invalid matrix input")
			}
		}
	}
	for _, operation := range []func([][]float64, []float64) ([]float64, error){StatisticsSequenceMean, StatisticsSequenceMinimum, StatisticsSequenceMaximum} {
		got, err := operation([][]float64{{1, 2}}, nil)
		sequenceClose(t, got, []float64{1, 2}, err)
	}
}

func TestStatisticsSequenceBounds(t *testing.T) {
	for _, shape := range [][2]int{{100001, 1}, {1, 257}, {100000, 11}} {
		samples := make([][]float64, shape[0])
		samples[0] = make([]float64, shape[1])
		if got, err := StatisticsSequenceMean(samples, nil); err == nil || got != nil {
			t.Fatalf("accepted shape %v", shape)
		}
	}
	samples := make([][]float64, 6104)
	samples[0] = make([]float64, 128)
	for _, operation := range []func([][]float64, []float64) ([][]float64, error){StatisticsCovariance, StatisticsCorrelation} {
		if got, err := operation(samples, nil); err == nil || got != nil {
			t.Fatal("accepted matrix work above 100000000")
		}
	}
	samples = make([][]float64, 100000)
	row := make([]float64, 10)
	for i := range samples {
		samples[i] = row
	}
	got, err := StatisticsSequenceMean(samples, nil)
	sequenceClose(t, got, make([]float64, 10), err)
	wide := make([]float64, 256)
	got, err = StatisticsSequenceMean([][]float64{wide}, nil)
	sequenceClose(t, got, wide, err)
}
