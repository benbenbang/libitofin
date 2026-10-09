package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

func chartRangeLine(high, low, close []float64, period int, smooth bool) (ChartSeries, error) {
	if len(high) != len(close) || len(low) != len(close) {
		return ChartSeries{}, fmt.Errorf("itofin: chart high/low/close lengths differ")
	}
	if smooth && period <= 0 {
		return ChartSeries{}, fmt.Errorf("itofin: chart period must be positive")
	}
	if len(close) > DefaultMaxOutputValues {
		return ChartSeries{}, fmt.Errorf("itofin: chart result exceeds output limit")
	}
	result := ChartSeries{Values: make([]float64, len(close))}
	var firstValid C.size_t
	var e C.ItofinError
	var status C.int32_t
	if smooth {
		status = C.itofin_chart_atr(doubles(high), doubles(low), doubles(close), C.size_t(len(close)), C.size_t(period), doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e)
	} else {
		status = C.itofin_chart_true_range(doubles(high), doubles(low), doubles(close), C.size_t(len(close)), doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e)
	}
	if err := ffiError(status, &e); err != nil {
		return ChartSeries{}, err
	}
	result.FirstValid = int(firstValid)
	return result, nil
}

// TrueRange computes gap-aware true range. The first bar uses high-low; later
// bars also consider distances from the preceding close. All HLC inputs must
// be finite, equal-length and ordered. Every range difference must be finite.
func TrueRange(high, low, close []float64) (ChartSeries, error) {
	return chartRangeLine(high, low, close, 1, false)
}

// ATR computes Wilder average true range, seeded by the arithmetic mean of
// the first period true ranges including bar zero. FirstValid is period-1,
// capped at input length, with zero placeholders during warmup. Period one
// returns exactly TrueRange. Invalid HLC and differences fail even in warmup.
func ATR(high, low, close []float64, period int) (ChartSeries, error) {
	return chartRangeLine(high, low, close, period, true)
}

// DefaultATR uses a 14-bar Wilder window.
func DefaultATR(high, low, close []float64) (ChartSeries, error) {
	return ATR(high, low, close, 14)
}
