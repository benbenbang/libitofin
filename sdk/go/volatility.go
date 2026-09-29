package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// SimpleLocalVolatility estimates volatility from adjacent positive closes.
// yearFractions is indexed by close; its first element is unused.
func SimpleLocalVolatility(close, yearFractions []float64) (ChartSeries, error) {
	if len(close) != len(yearFractions) {
		return ChartSeries{}, fmt.Errorf("itofin: volatility close and year fraction lengths differ")
	}
	if len(close) > DefaultMaxOutputValues {
		return ChartSeries{}, fmt.Errorf("itofin: volatility result exceeds output limit")
	}
	result := ChartSeries{Values: make([]float64, len(close))}
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_volatility_simple_local(
		doubles(close), doubles(yearFractions), C.size_t(len(close)),
		doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return ChartSeries{}, err
	}
	result.FirstValid = int(firstValid)
	return result, nil
}

// SimpleLocalVolatilityConstantFraction uses one year fraction for every
// adjacent pair of positive closes.
func SimpleLocalVolatilityConstantFraction(close []float64, yearFraction float64) (ChartSeries, error) {
	if len(close) > DefaultMaxOutputValues {
		return ChartSeries{}, fmt.Errorf("itofin: volatility result exceeds output limit")
	}
	result := ChartSeries{Values: make([]float64, len(close))}
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_volatility_simple_local_constant_fraction(
		doubles(close), C.size_t(len(close)), C.double(yearFraction),
		doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return ChartSeries{}, err
	}
	result.FirstValid = int(firstValid)
	return result, nil
}

// ConstantVolatility estimates a constant volatility from the previous window
// valid values, excluding the current bar. Its warmup follows the input series.
func ConstantVolatility(input ChartSeries, window int) (ChartSeries, error) {
	if input.FirstValid < 0 || input.FirstValid > len(input.Values) {
		return ChartSeries{}, fmt.Errorf("itofin: volatility first valid index is out of bounds")
	}
	if window <= 0 {
		return ChartSeries{}, fmt.Errorf("itofin: volatility window must be positive")
	}
	if len(input.Values) > DefaultMaxOutputValues {
		return ChartSeries{}, fmt.Errorf("itofin: volatility result exceeds output limit")
	}
	result := ChartSeries{Values: make([]float64, len(input.Values))}
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_volatility_constant(
		doubles(input.Values), C.size_t(len(input.Values)), C.size_t(input.FirstValid), C.size_t(window),
		doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return ChartSeries{}, err
	}
	result.FirstValid = int(firstValid)
	return result, nil
}
