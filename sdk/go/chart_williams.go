package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// WilliamsR computes inclusive trailing Williams percent R in [-100, 0].
// Flat windows return -50, matching KD's neutral RSV 50. FirstValid is
// period-1 capped at length; warmup slots contain zero placeholders.
// Finite ordered HLC and finite rolling differences are required, even during
// warmup. Negative prices are valid. The result owns its values.
func WilliamsR(high, low, close []float64, period int) (ChartSeries, error) {
	if len(high) != len(close) || len(low) != len(close) {
		return ChartSeries{}, fmt.Errorf("itofin: chart high/low/close lengths differ")
	}
	if period <= 0 {
		return ChartSeries{}, fmt.Errorf("itofin: chart period must be positive")
	}
	if len(close) > DefaultMaxOutputValues {
		return ChartSeries{}, fmt.Errorf("itofin: chart result exceeds output limit")
	}
	result := ChartSeries{Values: make([]float64, len(close))}
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_chart_williams_r(doubles(high), doubles(low), doubles(close),
		C.size_t(len(close)), C.size_t(period), doubles(result.Values),
		C.size_t(len(result.Values)), &firstValid, &e)
	if err := ffiError(status, &e); err != nil {
		return ChartSeries{}, err
	}
	result.FirstValid = int(firstValid)
	return result, nil
}

// DefaultWilliamsR uses a 14-bar inclusive trailing window.
func DefaultWilliamsR(high, low, close []float64) (ChartSeries, error) {
	return WilliamsR(high, low, close, 14)
}
