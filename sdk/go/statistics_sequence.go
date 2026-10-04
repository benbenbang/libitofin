package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

const (
	sequenceMean = iota
	sequenceVariance
	sequenceStandardDeviation
	sequenceErrorEstimate
	sequenceMinimum
	sequenceMaximum
	sequenceCovariance
	sequenceCorrelation
)

func sequenceStatisticsEvaluate(samples [][]float64, weights []float64, measure int) ([]float64, error) {
	rows := len(samples)
	if rows < 1 || rows > 100000 {
		return nil, fmt.Errorf("itofin: sequence statistics requires 1-100000 rows")
	}
	dimension := len(samples[0])
	if dimension < 1 || dimension > 256 {
		return nil, fmt.Errorf("itofin: sequence statistics requires 1-256 coordinates")
	}
	if rows > 1000000/dimension {
		return nil, fmt.Errorf("itofin: sequence statistics input exceeds size limit")
	}
	outputLen := dimension
	if measure == sequenceCovariance || measure == sequenceCorrelation {
		outputLen = dimension * dimension
		if rows > 100000000/outputLen {
			return nil, fmt.Errorf("itofin: sequence statistics matrix work exceeds limit")
		}
	}
	if weights != nil && len(weights) != rows {
		return nil, fmt.Errorf("itofin: sequence statistics row and weight lengths differ")
	}
	for i, sample := range samples {
		if len(sample) != dimension {
			return nil, fmt.Errorf("itofin: sequence statistics row %d has inconsistent dimension", i)
		}
	}
	values := make([]float64, 0, rows*dimension)
	for _, sample := range samples {
		values = append(values, sample...)
	}
	out := make([]float64, outputLen)
	var e C.ItofinError
	status := C.itofin_sequence_statistics_evaluate(
		doubles(values), C.size_t(len(values)), C.size_t(rows), C.size_t(dimension),
		doubles(weights), C.size_t(len(weights)), C.int32_t(measure),
		doubles(out), C.size_t(len(out)), &e,
	)
	if err := ffiError(status, &e); err != nil {
		return nil, err
	}
	return out, nil
}

// StatisticsSequenceMean returns the weighted mean of each coordinate.
// Samples must be rectangular. Nil weights selects unit weights.
func StatisticsSequenceMean(samples [][]float64, weights []float64) ([]float64, error) {
	return sequenceStatisticsEvaluate(samples, weights, sequenceMean)
}

// StatisticsSequenceVariance returns each coordinate's weighted population
// variance multiplied by N/(N-1), counting zero-weight rows in N.
func StatisticsSequenceVariance(samples [][]float64, weights []float64) ([]float64, error) {
	return sequenceStatisticsEvaluate(samples, weights, sequenceVariance)
}

// StatisticsSequenceStandardDeviation returns square roots of coordinate variances.
func StatisticsSequenceStandardDeviation(samples [][]float64, weights []float64) ([]float64, error) {
	return sequenceStatisticsEvaluate(samples, weights, sequenceStandardDeviation)
}

// StatisticsSequenceErrorEstimate returns coordinate standard deviations divided
// by sqrt(N), counting zero-weight rows in N.
func StatisticsSequenceErrorEstimate(samples [][]float64, weights []float64) ([]float64, error) {
	return sequenceStatisticsEvaluate(samples, weights, sequenceErrorEstimate)
}

// StatisticsSequenceMinimum returns each coordinate's minimum, including zero-weight rows.
func StatisticsSequenceMinimum(samples [][]float64, weights []float64) ([]float64, error) {
	return sequenceStatisticsEvaluate(samples, weights, sequenceMinimum)
}

// StatisticsSequenceMaximum returns each coordinate's maximum, including zero-weight rows.
func StatisticsSequenceMaximum(samples [][]float64, weights []float64) ([]float64, error) {
	return sequenceStatisticsEvaluate(samples, weights, sequenceMaximum)
}

func sequenceStatisticsMatrix(samples [][]float64, weights []float64, measure int) ([][]float64, error) {
	values, err := sequenceStatisticsEvaluate(samples, weights, measure)
	if err != nil {
		return nil, err
	}
	dimension := len(samples[0])
	matrix := make([][]float64, dimension)
	for i := range matrix {
		matrix[i] = append([]float64(nil), values[i*dimension:(i+1)*dimension]...)
	}
	return matrix, nil
}

// StatisticsCovariance returns the symmetric weighted covariance matrix with
// the observation-count N/(N-1) correction. Zero-weight rows count toward N.
func StatisticsCovariance(samples [][]float64, weights []float64) ([][]float64, error) {
	return sequenceStatisticsMatrix(samples, weights, sequenceCovariance)
}

// StatisticsCorrelation returns covariance normalized by coordinate variances.
// Two constant coordinates have correlation 1; one constant coordinate and one
// varying coordinate have correlation 0. Every diagonal entry is 1.
func StatisticsCorrelation(samples [][]float64, weights []float64) ([][]float64, error) {
	return sequenceStatisticsMatrix(samples, weights, sequenceCorrelation)
}
