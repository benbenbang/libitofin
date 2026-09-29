package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// OHLCOvernightEstimates contains annualized overnight-aware OHLC estimates aligned to input bars.
type OHLCOvernightEstimates struct {
	GarmanKlassSigma1 ChartSeries `json:"garman_klass_sigma1"`
	GarmanKlassSigma3 ChartSeries `json:"garman_klass_sigma3"`
	GarmanKlassSigma6 ChartSeries `json:"garman_klass_sigma6"`
}

func ohlcOvernightInputLength(open, high, low, close []float64) (int, error) {
	n := len(close)
	if len(open) != n || len(high) != n || len(low) != n {
		return 0, fmt.Errorf("itofin: volatility OHLC lengths differ")
	}
	if n > DefaultMaxOutputValues/3 {
		return 0, fmt.Errorf("itofin: volatility result exceeds output limit")
	}
	return n, nil
}

func ohlcOvernightEstimates(values []float64, n, firstValid int) OHLCOvernightEstimates {
	return OHLCOvernightEstimates{
		GarmanKlassSigma1: ChartSeries{Values: values[:n:n], FirstValid: firstValid},
		GarmanKlassSigma3: ChartSeries{Values: values[n : 2*n : 2*n], FirstValid: firstValid},
		GarmanKlassSigma6: ChartSeries{Values: values[2*n:], FirstValid: firstValid},
	}
}

// OHLCOvernightVolatility estimates three overnight-aware annualized series.
// The first bar has no prior close and is outside the valid range.
func OHLCOvernightVolatility(open, high, low, close, yearFractions []float64, overnightFraction float64) (OHLCOvernightEstimates, error) {
	n, err := ohlcOvernightInputLength(open, high, low, close)
	if err != nil {
		return OHLCOvernightEstimates{}, err
	}
	if len(yearFractions) != n {
		return OHLCOvernightEstimates{}, fmt.Errorf("itofin: volatility OHLC and year fraction lengths differ")
	}
	values := make([]float64, 3*n)
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_volatility_ohlc_overnight(
		doubles(open), doubles(high), doubles(low), doubles(close), doubles(yearFractions), C.size_t(n), C.double(overnightFraction),
		doubles(values), C.size_t(len(values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return OHLCOvernightEstimates{}, err
	}
	return ohlcOvernightEstimates(values, n, int(firstValid)), nil
}

// OHLCOvernightVolatilityConstantFraction uses one year fraction for each bar after the first.
func OHLCOvernightVolatilityConstantFraction(open, high, low, close []float64, yearFraction, overnightFraction float64) (OHLCOvernightEstimates, error) {
	n, err := ohlcOvernightInputLength(open, high, low, close)
	if err != nil {
		return OHLCOvernightEstimates{}, err
	}
	values := make([]float64, 3*n)
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_volatility_ohlc_overnight_constant_fraction(
		doubles(open), doubles(high), doubles(low), doubles(close), C.size_t(n), C.double(yearFraction), C.double(overnightFraction),
		doubles(values), C.size_t(len(values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return OHLCOvernightEstimates{}, err
	}
	return ohlcOvernightEstimates(values, n, int(firstValid)), nil
}
