package itofin

import (
	"encoding/json"
	"math"
	"os"
	"path/filepath"
	"reflect"
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

func TestStatisticsSequenceWeightedVectors(t *testing.T) {
	samples := [][]float64{{1, 2}, {3, 6}, {5, 10}}
	weights := []float64{1, 2, 1}
	for _, operation := range []struct {
		name string
		call func([][]float64, []float64) ([]float64, error)
		want []float64
	}{
		{"mean", StatisticsSequenceMean, []float64{3, 6}},
		{"variance", StatisticsSequenceVariance, []float64{3, 12}},
		{"standard deviation", StatisticsSequenceStandardDeviation, []float64{math.Sqrt(3), math.Sqrt(12)}},
		{"error estimate", StatisticsSequenceErrorEstimate, []float64{1, 2}},
		{"minimum", StatisticsSequenceMinimum, []float64{1, 2}},
		{"maximum", StatisticsSequenceMaximum, []float64{5, 10}},
	} {
		t.Run(operation.name, func(t *testing.T) {
			got, err := operation.call(samples, weights)
			sequenceClose(t, got, operation.want, err)
		})
	}
	if !reflect.DeepEqual(samples, [][]float64{{1, 2}, {3, 6}, {5, 10}}) || !reflect.DeepEqual(weights, []float64{1, 2, 1}) {
		t.Fatal("sequence evaluation changed caller input")
	}
}

func TestStatisticsSequenceCovarianceAndCorrelation(t *testing.T) {
	samples := [][]float64{{1, 2}, {3, 6}, {5, 10}}
	weights := []float64{1, 2, 1}
	covariance, err := StatisticsCovariance(samples, weights)
	if err != nil {
		t.Fatal(err)
	}
	sequenceClose(t, covariance[0], []float64{3, 6}, nil)
	sequenceClose(t, covariance[1], []float64{6, 12}, nil)
	correlation, err := StatisticsCorrelation(samples, weights)
	if err != nil {
		t.Fatal(err)
	}
	sequenceClose(t, correlation[0], []float64{1, 1}, nil)
	sequenceClose(t, correlation[1], []float64{1, 1}, nil)
	correlation, err = StatisticsCorrelation([][]float64{{1, 5}, {2, 3}, {3, 1}}, nil)
	if err != nil {
		t.Fatal(err)
	}
	sequenceClose(t, correlation[0], []float64{1, -1}, nil)
	sequenceClose(t, correlation[1], []float64{-1, 1}, nil)
}

func TestStatisticsSequenceZeroWeightCountAndConstants(t *testing.T) {
	samples := [][]float64{{1, 2}, {999, -999}, {3, 6}}
	weights := []float64{1, 0, 1}
	variance, err := StatisticsSequenceVariance(samples, weights)
	sequenceClose(t, variance, []float64{1.5, 6}, err)
	errorEstimate, err := StatisticsSequenceErrorEstimate(samples, weights)
	sequenceClose(t, errorEstimate, []float64{math.Sqrt(0.5), math.Sqrt(2)}, err)
	minimum, err := StatisticsSequenceMinimum(samples, weights)
	sequenceClose(t, minimum, []float64{1, -999}, err)
	maximum, err := StatisticsSequenceMaximum(samples, weights)
	sequenceClose(t, maximum, []float64{999, 6}, err)
	covariance, err := StatisticsCovariance(samples, weights)
	if err != nil {
		t.Fatal(err)
	}
	sequenceClose(t, covariance[0], []float64{1.5, 3}, nil)
	sequenceClose(t, covariance[1], []float64{3, 6}, nil)
	correlation, err := StatisticsCorrelation([][]float64{{2, 7, 1}, {2, 7, 2}, {2, 7, 3}}, nil)
	if err != nil {
		t.Fatal(err)
	}
	sequenceClose(t, correlation[0], []float64{1, 1, 0}, nil)
	sequenceClose(t, correlation[1], []float64{1, 1, 0}, nil)
	sequenceClose(t, correlation[2], []float64{0, 0, 1}, nil)
}

func TestStatisticsSequenceUnitWeightsAndIndependentStorage(t *testing.T) {
	samples := [][]float64{{1, 4}, {2, 7}, {4, 2}}
	for _, operation := range []func([][]float64, []float64) ([]float64, error){
		StatisticsSequenceMean, StatisticsSequenceVariance, StatisticsSequenceStandardDeviation,
		StatisticsSequenceErrorEstimate, StatisticsSequenceMinimum, StatisticsSequenceMaximum,
	} {
		unit, err := operation(samples, nil)
		if err != nil {
			t.Fatal(err)
		}
		explicit, err := operation(samples, []float64{1, 1, 1})
		sequenceClose(t, explicit, unit, err)
		unit[0] = 999
		if samples[0][0] != 1 || explicit[0] == 999 {
			t.Fatal("returned vector aliases input or another result")
		}
	}
	for _, operation := range []func([][]float64, []float64) ([][]float64, error){StatisticsCovariance, StatisticsCorrelation} {
		unit, err := operation(samples, nil)
		if err != nil {
			t.Fatal(err)
		}
		explicit, err := operation(samples, []float64{1, 1, 1})
		if err != nil {
			t.Fatal(err)
		}
		for i := range unit {
			sequenceClose(t, explicit[i], unit[i], nil)
			for j := range unit {
				if unit[i][j] != unit[j][i] {
					t.Fatal("matrix is not symmetric")
				}
			}
		}
		unit[0][0] = 999
		unit[0] = append(unit[0], 888)
		if samples[0][0] != 1 || explicit[0][0] == 999 || unit[1][0] == 888 {
			t.Fatal("returned matrix aliases caller, another result, or another row")
		}
	}
	first, err := StatisticsSequenceMean(samples, nil)
	if err != nil {
		t.Fatal(err)
	}
	samples[0][0] = 100
	second, err := StatisticsSequenceMean(samples, nil)
	if err != nil || first[0] == second[0] {
		t.Fatal("stateless calculation retained input or stale results")
	}
}

func TestStatisticsSequencePinnedNativeAndCenteredOracle(t *testing.T) {
	fixtureDir := filepath.Join("testdata", "sequence-statistics")
	readJSON := func(name string, out any) {
		t.Helper()
		data, err := os.ReadFile(filepath.Join(fixtureDir, name))
		if err != nil {
			t.Fatal(err)
		}
		if err := json.Unmarshal(data, out); err != nil {
			t.Fatal(err)
		}
	}
	var manifest struct{ Cases []string }
	readJSON("oracle.json", &manifest)
	if len(manifest.Cases) != 8 {
		t.Fatalf("unexpected oracle case count %d", len(manifest.Cases))
	}
	for _, filename := range manifest.Cases {
		var fixture struct {
			Name     string
			Samples  [][]float64
			Weights  []float64
			Native   map[string][]float64
			Centered map[string][]float64
		}
		readJSON(filename, &fixture)
		t.Run(fixture.Name, func(t *testing.T) {
			operations := map[string]func([][]float64, []float64) ([]float64, error){
				"mean":               StatisticsSequenceMean,
				"variance":           StatisticsSequenceVariance,
				"standard_deviation": StatisticsSequenceStandardDeviation,
				"error_estimate":     StatisticsSequenceErrorEstimate,
				"minimum":            StatisticsSequenceMinimum,
				"maximum":            StatisticsSequenceMaximum,
			}
			for name, call := range map[string]func([][]float64, []float64) ([][]float64, error){
				"covariance": StatisticsCovariance, "correlation": StatisticsCorrelation,
			} {
				operations[name] = func(samples [][]float64, weights []float64) ([]float64, error) {
					matrix, err := call(samples, weights)
					if err != nil {
						return nil, err
					}
					var values []float64
					for _, row := range matrix {
						values = append(values, row...)
					}
					return values, nil
				}
			}
			for name, call := range operations {
				got, err := call(fixture.Samples, fixture.Weights)
				if err != nil {
					t.Fatalf("%s: %v", name, err)
				}
				references := []map[string][]float64{fixture.Centered}
				if fixture.Name != "large_offset_cancellation" {
					references = append(references, fixture.Native)
				}
				for _, reference := range references {
					want := reference[name]
					if len(got) != len(want) {
						t.Fatalf("%s: result length %d, want %d", name, len(got), len(want))
					}
					for i := range got {
						if math.IsNaN(got[i]) || math.IsInf(got[i], 0) || math.Abs(got[i]-want[i]) > 2e-12+2e-12*math.Abs(want[i]) {
							t.Fatalf("%s coordinate %d: got %.17g, want %.17g", name, i, got[i], want[i])
						}
					}
				}
			}
		})
	}
}
