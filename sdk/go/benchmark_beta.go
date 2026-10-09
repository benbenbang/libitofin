package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// BenchmarkBeta returns weighted asset/benchmark covariance divided by benchmark
// variance. Returns must already align by date and frequency; no annualization,
// data feed or risk-free adjustment is applied. Nil weights means unit weights.
// It requires 2-100000 finite aligned observations, finite nonnegative weights
// with positive total, positive benchmark variance and finite moments/output.
// The common observation-count N/(N-1) correction includes zero-weight rows.
// Inputs are unchanged and no native context or handle needs closing.
func BenchmarkBeta(assetReturns, benchmarkReturns, weights []float64) (float64, error) {
	if len(assetReturns) < 2 || len(assetReturns) > 100000 || len(assetReturns) != len(benchmarkReturns) {
		return 0, fmt.Errorf("itofin: benchmark beta requires 2-100000 aligned rows")
	}
	if weights != nil && len(weights) != len(assetReturns) {
		return 0, fmt.Errorf("itofin: benchmark beta weight length mismatch")
	}
	var out C.double
	var e C.ItofinError
	status := C.itofin_benchmark_beta(doubles(assetReturns), C.size_t(len(assetReturns)),
		doubles(benchmarkReturns), C.size_t(len(benchmarkReturns)),
		doubles(weights), C.size_t(len(weights)), &out, &e)
	if err := ffiError(status, &e); err != nil {
		return 0, err
	}
	return float64(out), nil
}
