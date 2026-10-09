package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// ADXResult contains aligned positive/negative DI, DX and Wilder ADX.
// Each series carries independent warmup metadata and caller-owned values.
type ADXResult struct {
	PlusDI  ChartSeries `json:"plus_di"`
	MinusDI ChartSeries `json:"minus_di"`
	DX      ChartSeries `json:"dx"`
	ADX     ChartSeries `json:"adx"`
}

// ADX computes transition-seeded Wilder directional indicators. Bar zero
// contributes no movement; equal positive up/down movements select neither.
// DI/DX first become valid at period, ADX at 2*period-1, capped at input length.
// Period one is supported. Zero TR sets both DI to zero; zero DI sum sets DX
// to zero. Ordered finite HLC and finite differences are required even during
// warmup. Use NullableValues to mask each series' zero warmup placeholders.
func ADX(high, low, close []float64, period int) (ADXResult, error) {
	n := len(close)
	if len(high) != n || len(low) != n {
		return ADXResult{}, fmt.Errorf("itofin: chart high/low/close lengths differ")
	}
	if period <= 0 {
		return ADXResult{}, fmt.Errorf("itofin: chart period must be positive")
	}
	if n > DefaultMaxOutputValues/4 {
		return ADXResult{}, fmt.Errorf("itofin: chart result exceeds output limit")
	}
	values := make([]float64, n*4)
	var valid [4]C.size_t
	var e C.ItofinError
	status := C.itofin_chart_adx(doubles(high), doubles(low), doubles(close), C.size_t(n), C.size_t(period), doubles(values), C.size_t(len(values)), &valid[0], 4, &e)
	if err := ffiError(status, &e); err != nil {
		return ADXResult{}, err
	}
	return ADXResult{
		PlusDI:  ChartSeries{Values: values[:n:n], FirstValid: int(valid[0])},
		MinusDI: ChartSeries{Values: values[n : 2*n : 2*n], FirstValid: int(valid[1])},
		DX:      ChartSeries{Values: values[2*n : 3*n : 3*n], FirstValid: int(valid[2])},
		ADX:     ChartSeries{Values: values[3*n:], FirstValid: int(valid[3])},
	}, nil
}

// DefaultADX uses Wilder's conventional 14-transition period.
func DefaultADX(high, low, close []float64) (ADXResult, error) { return ADX(high, low, close, 14) }
