package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

func chartVolumeLine(price, volume []float64, weighted bool) (ChartSeries, error) {
	if len(price) != len(volume) {
		return ChartSeries{}, fmt.Errorf("itofin: chart price/volume lengths differ")
	}
	if len(price) > DefaultMaxOutputValues {
		return ChartSeries{}, fmt.Errorf("itofin: chart result exceeds output limit")
	}
	result := ChartSeries{Values: make([]float64, len(price))}
	var firstValid C.size_t
	var e C.ItofinError
	var status C.int32_t
	if weighted {
		status = C.itofin_chart_vwap(doubles(price), doubles(volume), C.size_t(len(price)), doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e)
	} else {
		status = C.itofin_chart_obv(doubles(price), doubles(volume), C.size_t(len(price)), doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e)
	}
	if err := ffiError(status, &e); err != nil {
		return ChartSeries{}, err
	}
	result.FirstValid = int(firstValid)
	return result, nil
}

// VWAP computes cumulative volume-weighted supplied prices, starting a new
// session per call. Choose trade, close, or typical prices explicitly. An initial
// zero-volume prefix is missing; later zero volume carries the previous result.
// Inputs must be finite and equal length, with nonnegative volumes. Cumulative
// volume overflow is an error; price-volume products are not formed.
func VWAP(price, volume []float64) (ChartSeries, error) {
	return chartVolumeLine(price, volume, true)
}

// OBV computes zero-seeded on-balance volume. The initial volume is validated
// but not added. Later rises add volume, falls subtract it, and equal closes
// preserve it. Inputs must be finite and equal length, with nonnegative volume.
// Signed-volume overflow returns an error.
func OBV(close, volume []float64) (ChartSeries, error) {
	return chartVolumeLine(close, volume, false)
}
