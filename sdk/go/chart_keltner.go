package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"math"
)

// KeltnerChannels contains EMA-close center and Wilder-ATR envelopes.
// All three series share the later first-valid index after both warmups.
type KeltnerChannels struct {
	Center ChartSeries `json:"center"`
	Upper  ChartSeries `json:"upper"`
	Lower  ChartSeries `json:"lower"`
}

// ChartKeltnerChannels preserves EMA's arithmetic close-price seed and adds
// or subtracts multiplier*ATR. Warmup slots are zero placeholders in all three
// series. Periods must be positive; multiplier must be finite and nonnegative.
// Ordered, equal-length, finite HLC inputs and finite range/band arithmetic
// are required. Invalid raw ranges fail even during warmup or with multiplier 0.
func ChartKeltnerChannels(high, low, close []float64, centerPeriod, atrPeriod int, multiplier float64) (KeltnerChannels, error) {
	n := len(close)
	if len(high) != n || len(low) != n {
		return KeltnerChannels{}, fmt.Errorf("itofin: chart high/low/close lengths differ")
	}
	if centerPeriod <= 0 || atrPeriod <= 0 || math.IsNaN(multiplier) || math.IsInf(multiplier, 0) || multiplier < 0 {
		return KeltnerChannels{}, fmt.Errorf("itofin: invalid Keltner periods or multiplier")
	}
	if n > DefaultMaxOutputValues/3 {
		return KeltnerChannels{}, fmt.Errorf("itofin: chart result exceeds output limit")
	}
	values := make([]float64, n*3)
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_chart_keltner_channels(
		doubles(high), doubles(low), doubles(close), C.size_t(n),
		C.size_t(centerPeriod), C.size_t(atrPeriod), C.double(multiplier),
		doubles(values), C.size_t(len(values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return KeltnerChannels{}, err
	}
	valid := int(firstValid)
	return KeltnerChannels{
		Center: ChartSeries{Values: values[:n:n], FirstValid: valid},
		Upper:  ChartSeries{Values: values[n : 2*n : 2*n], FirstValid: valid},
		Lower:  ChartSeries{Values: values[2*n:], FirstValid: valid},
	}, nil
}

// DefaultKeltnerChannels uses EMA(close) 20, Wilder ATR 10 and multiplier 2.
// Its ATR period is 10, distinct from DefaultATR's 14-bar window.
func DefaultKeltnerChannels(high, low, close []float64) (KeltnerChannels, error) {
	return ChartKeltnerChannels(high, low, close, 20, 10, 2)
}
