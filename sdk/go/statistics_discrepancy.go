package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// StatisticsDiscrepancy returns normalized L2 star discrepancy of rectangular
// unit-cube samples in 2-256 dimensions. Nil weights selects unit weights;
// explicit weights must all equal one. At most 4096 rows, 1000000 components
// and 100000000 pair-coordinate operations are allowed. No random draws occur.
func StatisticsDiscrepancy(samples [][]float64, weights []float64) (float64, error) {
	rows := len(samples)
	if rows < 1 || rows > 4096 {
		return 0, fmt.Errorf("itofin: discrepancy requires 1-4096 rows")
	}
	dimension := len(samples[0])
	if dimension < 2 || dimension > 256 {
		return 0, fmt.Errorf("itofin: discrepancy requires 2-256 coordinates")
	}
	if rows > 1000000/dimension || rows > (100000000/dimension)/rows {
		return 0, fmt.Errorf("itofin: discrepancy input or work exceeds size limit")
	}
	if weights != nil && len(weights) != rows {
		return 0, fmt.Errorf("itofin: discrepancy row and weight lengths differ")
	}
	for i, sample := range samples {
		if len(sample) != dimension {
			return 0, fmt.Errorf("itofin: discrepancy row %d has inconsistent dimension", i)
		}
	}
	values := make([]float64, 0, rows*dimension)
	for _, sample := range samples {
		values = append(values, sample...)
	}
	var out C.double
	var e C.ItofinError
	status := C.itofin_discrepancy_statistics_evaluate(
		doubles(values), C.size_t(len(values)), C.size_t(rows), C.size_t(dimension),
		doubles(weights), C.size_t(len(weights)), &out, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return 0, err
	}
	return float64(out), nil
}
