package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// Garch11Result contains conditional volatility aligned to returns and the
// next conditional variance. The volatility has the same units as returns.
type Garch11Result struct {
	ConditionalVolatility ChartSeries `json:"conditional_volatility"`
	NextVariance          float64     `json:"next_variance"`
}

// Garch11FitResult holds stationary fitted parameters and a one-step forecast.
// Omega is the variance intercept; LogLikelihood is per return and omits log(2π).
type Garch11FitResult struct {
	Alpha         float64 `json:"alpha"`
	Beta          float64 `json:"beta"`
	Omega         float64 `json:"omega"`
	LogLikelihood float64 `json:"log_likelihood"`
	NextVariance  float64 `json:"next_variance"`
}

// Garch11Fit estimates stationary GARCH(1,1) parameters from returns.
func Garch11Fit(returns []float64) (Garch11FitResult, error) {
	if len(returns) > 100_000 {
		return Garch11FitResult{}, fmt.Errorf("itofin: GARCH fit supports at most 100000 returns")
	}
	var alpha, beta, omega, logLikelihood, nextVariance C.double
	var e C.ItofinError
	status := C.itofin_garch11_fit(
		doubles(returns), C.size_t(len(returns)),
		&alpha, &beta, &omega, &logLikelihood, &nextVariance, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return Garch11FitResult{}, err
	}
	return Garch11FitResult{
		Alpha:         float64(alpha),
		Beta:          float64(beta),
		Omega:         float64(omega),
		LogLikelihood: float64(logLikelihood),
		NextVariance:  float64(nextVariance),
	}, nil
}

// Garch11Filter applies fixed GARCH(1,1) parameters to returns. The first
// return seeds the variance, so volatility becomes valid at index 1.
// longRunVariance is the unconditional variance, not the intercept.
func Garch11Filter(returns []float64, alpha, beta, longRunVariance float64) (Garch11Result, error) {
	if len(returns) == 0 {
		return Garch11Result{}, fmt.Errorf("itofin: GARCH requires at least one return")
	}
	if len(returns) > DefaultMaxOutputValues {
		return Garch11Result{}, fmt.Errorf("itofin: GARCH result exceeds output limit")
	}
	result := Garch11Result{ConditionalVolatility: ChartSeries{Values: make([]float64, len(returns))}}
	var firstValid C.size_t
	var nextVariance C.double
	var e C.ItofinError
	status := C.itofin_garch11_filter(
		doubles(returns), C.size_t(len(returns)),
		C.double(alpha), C.double(beta), C.double(longRunVariance),
		doubles(result.ConditionalVolatility.Values), C.size_t(len(result.ConditionalVolatility.Values)),
		&firstValid, &nextVariance, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return Garch11Result{}, err
	}
	result.ConditionalVolatility.FirstValid = int(firstValid)
	result.NextVariance = float64(nextVariance)
	return result, nil
}

// Garch11Forecast computes the next conditional variance from the last return
// and current conditional variance using fixed GARCH(1,1) parameters.
func Garch11Forecast(lastReturn, currentVariance, alpha, beta, longRunVariance float64) (float64, error) {
	var nextVariance C.double
	var e C.ItofinError
	status := C.itofin_garch11_forecast(
		C.double(lastReturn), C.double(currentVariance),
		C.double(alpha), C.double(beta), C.double(longRunVariance), &nextVariance, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return 0, err
	}
	return float64(nextVariance), nil
}
