package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"encoding/json"
	"fmt"
	"math"
	"unsafe"
)

// ChartSeries aligns values to input bars. Values before FirstValid are zero
// placeholders; use NullableValues or JSON to represent them as missing.
type ChartSeries struct {
	Values     []float64
	FirstValid int
}

// NullableValues returns one value per input bar, with nil during warmup.
func (s ChartSeries) NullableValues() []*float64 {
	values := make([]*float64, len(s.Values))
	start := s.FirstValid
	if start < 0 {
		start = 0
	}
	for i := start; i < len(values); i++ {
		values[i] = &s.Values[i]
	}
	return values
}

// MarshalJSON encodes warmup as null without exposing the zero placeholders.
func (s ChartSeries) MarshalJSON() ([]byte, error) {
	return json.Marshal(struct {
		Values     []*float64 `json:"values"`
		FirstValid int        `json:"first_valid"`
	}{s.NullableValues(), s.FirstValid})
}

// VolumeBars contains volume amounts and directions: -1 down, 0 flat, 1 up.
type VolumeBars struct {
	Volume    ChartSeries `json:"volume"`
	Direction []int8      `json:"direction"`
}

// BollingerBands contains population-deviation bands aligned to input closes.
type BollingerBands struct {
	Middle ChartSeries `json:"middle"`
	Upper  ChartSeries `json:"upper"`
	Lower  ChartSeries `json:"lower"`
}

type chartLineKind uint8

const (
	chartSimple chartLineKind = iota
	chartExponential
	chartRelativeStrength
)

func chartLine(close []float64, period int, kind chartLineKind) (ChartSeries, error) {
	if period <= 0 {
		return ChartSeries{}, fmt.Errorf("itofin: chart period must be positive")
	}
	if len(close) > DefaultMaxOutputValues {
		return ChartSeries{}, fmt.Errorf("itofin: chart result exceeds output limit")
	}
	result := ChartSeries{Values: make([]float64, len(close))}
	var firstValid C.size_t
	var e C.ItofinError
	var status C.int32_t
	switch kind {
	case chartExponential:
		status = C.itofin_chart_ema(doubles(close), C.size_t(len(close)), C.size_t(period), doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e)
	case chartRelativeStrength:
		status = C.itofin_chart_rsi(doubles(close), C.size_t(len(close)), C.size_t(period), doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e)
	default:
		status = C.itofin_chart_sma(doubles(close), C.size_t(len(close)), C.size_t(period), doubles(result.Values), C.size_t(len(result.Values)), &firstValid, &e)
	}
	if err := ffiError(status, &e); err != nil {
		return ChartSeries{}, err
	}
	result.FirstValid = int(firstValid)
	return result, nil
}

// SMA computes the trailing simple moving average of close. Its first valid
// value is at period-1; shorter inputs have no valid values.
func SMA(close []float64, period int) (ChartSeries, error) {
	return chartLine(close, period, chartSimple)
}

// EMA computes an exponential moving average seeded by the first period-bar
// SMA, then weighted by 2/(period+1).
func EMA(close []float64, period int) (ChartSeries, error) {
	return chartLine(close, period, chartExponential)
}

// ChartBollingerBands computes trailing population-standard-deviation bands.
// The three series first become valid at period-1. Multiplier must be finite
// and nonnegative.
func ChartBollingerBands(close []float64, period int, multiplier float64) (BollingerBands, error) {
	if period <= 0 || math.IsNaN(multiplier) || math.IsInf(multiplier, 0) || multiplier < 0 {
		return BollingerBands{}, fmt.Errorf("itofin: invalid Bollinger period or multiplier")
	}
	n := len(close)
	if n > DefaultMaxOutputValues/3 {
		return BollingerBands{}, fmt.Errorf("itofin: chart result exceeds output limit")
	}
	values := make([]float64, n*3)
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_chart_bollinger(
		doubles(close), C.size_t(n), C.size_t(period), C.double(multiplier),
		doubles(values), C.size_t(len(values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return BollingerBands{}, err
	}
	valid := int(firstValid)
	return BollingerBands{
		Middle: ChartSeries{Values: values[:n:n], FirstValid: valid},
		Upper:  ChartSeries{Values: values[n : 2*n : 2*n], FirstValid: valid},
		Lower:  ChartSeries{Values: values[2*n:], FirstValid: valid},
	}, nil
}

// DefaultBollingerBands uses a 20-bar window and two standard deviations.
func DefaultBollingerBands(close []float64) (BollingerBands, error) {
	return ChartBollingerBands(close, 20, 2)
}

// RSI computes Wilder's relative strength index from closing prices.
// It first becomes valid after period price changes, at index period.
// Flat input yields 50, pure gains 100, and pure losses 0.
func RSI(close []float64, period int) (ChartSeries, error) {
	return chartLine(close, period, chartRelativeStrength)
}

// DefaultRSI uses a 14-change Wilder window.
func DefaultRSI(close []float64) (ChartSeries, error) {
	return RSI(close, 14)
}

// ChartVolumeBars copies volume and classifies close relative to open.
// Each OHLCV input must have the same length. Finite negative prices are valid.
func ChartVolumeBars(open, high, low, close, volume []float64) (VolumeBars, error) {
	n := len(close)
	if len(open) != n || len(high) != n || len(low) != n || len(volume) != n {
		return VolumeBars{}, fmt.Errorf("itofin: chart OHLCV lengths differ")
	}
	if n > DefaultMaxOutputValues {
		return VolumeBars{}, fmt.Errorf("itofin: chart result exceeds output limit")
	}
	result := VolumeBars{
		Volume:    ChartSeries{Values: make([]float64, n)},
		Direction: make([]int8, n),
	}
	var direction *C.int8_t
	if n > 0 {
		direction = (*C.int8_t)(unsafe.Pointer(&result.Direction[0]))
	}
	var e C.ItofinError
	status := C.itofin_chart_volume_bars(
		doubles(open), doubles(high), doubles(low), doubles(close), doubles(volume),
		C.size_t(n), doubles(result.Volume.Values), direction, C.size_t(n), &e,
	)
	if err := ffiError(status, &e); err != nil {
		return VolumeBars{}, err
	}
	return result, nil
}
