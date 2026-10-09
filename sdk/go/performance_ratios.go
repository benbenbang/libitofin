package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

func performanceRatioInput(returns []float64) error {
	if len(returns) == 0 {
		return fmt.Errorf("itofin: invalid performance return count")
	}
	return nil
}

// TargetDownsideDeviation returns RMS shortfall below a scalar per-period target.
// All observations enter the denominator N. No downside returns zero.
// This differs from StatisticsDownsideDeviation's conditional measure.
func TargetDownsideDeviation(returns []float64, target float64) (float64, error) {
	if err := performanceRatioInput(returns); err != nil {
		return 0, err
	}
	var out C.double
	var e C.ItofinError
	status := C.itofin_target_downside_deviation(doubles(returns), C.size_t(len(returns)), C.double(target), &out, &e)
	if err := ffiError(status, &e); err != nil {
		return 0, err
	}
	return float64(out), nil
}

// SharpeRatio returns arithmetic excess mean/sample std (N-1), scaled by
// sqrt(periodsPerYear). RiskFreeReturn is per period, not annual.
// Frequency is required and finite-positive. Zero dispersion returns an error.
func SharpeRatio(returns []float64, riskFreeReturn, periodsPerYear float64) (float64, error) {
	if err := performanceRatioInput(returns); err != nil {
		return 0, err
	}
	var out C.double
	var e C.ItofinError
	status := C.itofin_sharpe_ratio(doubles(returns), C.size_t(len(returns)), C.double(riskFreeReturn), C.double(periodsPerYear), &out, &e)
	if err := ffiError(status, &e); err != nil {
		return 0, err
	}
	return float64(out), nil
}

// SortinoRatio returns arithmetic mean minus per-period MAR/all-N downside,
// scaled by sqrt(periodsPerYear). No downside returns an error, not infinity.
// Square-root scaling is not exact annual compounded downside risk.
func SortinoRatio(returns []float64, minimumAcceptableReturn, periodsPerYear float64) (float64, error) {
	if err := performanceRatioInput(returns); err != nil {
		return 0, err
	}
	var out C.double
	var e C.ItofinError
	status := C.itofin_sortino_ratio(doubles(returns), C.size_t(len(returns)), C.double(minimumAcceptableReturn), C.double(periodsPerYear), &out, &e)
	if err := ffiError(status, &e); err != nil {
		return 0, err
	}
	return float64(out), nil
}
