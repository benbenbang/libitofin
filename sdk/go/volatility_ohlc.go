package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// OHLCPointEstimates contains annualized point estimates aligned to input bars.
type OHLCPointEstimates struct {
	SimpleSigma       ChartSeries `json:"simple_sigma"`
	ParkinsonSigma    ChartSeries `json:"parkinson_sigma"`
	GarmanKlassSigma4 ChartSeries `json:"garman_klass_sigma4"`
	GarmanKlassSigma5 ChartSeries `json:"garman_klass_sigma5"`
}

func ohlcPointInputLength(open, high, low, close []float64) (int, error) {
	n := len(close)
	if len(open) != n || len(high) != n || len(low) != n {
		return 0, fmt.Errorf("itofin: volatility OHLC lengths differ")
	}
	if n > DefaultMaxOutputValues/4 {
		return 0, fmt.Errorf("itofin: volatility result exceeds output limit")
	}
	return n, nil
}

func ohlcPointEstimates(values []float64, n, firstValid int) OHLCPointEstimates {
	return OHLCPointEstimates{
		SimpleSigma:       ChartSeries{Values: values[:n:n], FirstValid: firstValid},
		ParkinsonSigma:    ChartSeries{Values: values[n : 2*n : 2*n], FirstValid: firstValid},
		GarmanKlassSigma4: ChartSeries{Values: values[2*n : 3*n : 3*n], FirstValid: firstValid},
		GarmanKlassSigma5: ChartSeries{Values: values[3*n:], FirstValid: firstValid},
	}
}

// OHLCPointVolatility estimates four annualized volatility series from positive
// OHLC bars and an indexed year fraction for each bar.
func OHLCPointVolatility(open, high, low, close, yearFractions []float64) (OHLCPointEstimates, error) {
	n, err := ohlcPointInputLength(open, high, low, close)
	if err != nil {
		return OHLCPointEstimates{}, err
	}
	if len(yearFractions) != n {
		return OHLCPointEstimates{}, fmt.Errorf("itofin: volatility OHLC and year fraction lengths differ")
	}
	values := make([]float64, 4*n)
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_volatility_ohlc_point(
		doubles(open), doubles(high), doubles(low), doubles(close), doubles(yearFractions), C.size_t(n),
		doubles(values), C.size_t(len(values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return OHLCPointEstimates{}, err
	}
	return ohlcPointEstimates(values, n, int(firstValid)), nil
}

// OHLCPointVolatilityConstantFraction uses one year fraction for every OHLC bar.
func OHLCPointVolatilityConstantFraction(open, high, low, close []float64, yearFraction float64) (OHLCPointEstimates, error) {
	n, err := ohlcPointInputLength(open, high, low, close)
	if err != nil {
		return OHLCPointEstimates{}, err
	}
	values := make([]float64, 4*n)
	var firstValid C.size_t
	var e C.ItofinError
	status := C.itofin_volatility_ohlc_point_constant_fraction(
		doubles(open), doubles(high), doubles(low), doubles(close), C.size_t(n), C.double(yearFraction),
		doubles(values), C.size_t(len(values)), &firstValid, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return OHLCPointEstimates{}, err
	}
	return ohlcPointEstimates(values, n, int(firstValid)), nil
}
