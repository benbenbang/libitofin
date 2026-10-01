package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

const (
	incrementalWeightSum = iota
	incrementalDownsideWeightSum
	incrementalMin
	incrementalMax
	incrementalMean
	incrementalVariance
	incrementalStandardDeviation
	incrementalErrorEstimate
	incrementalSkewness
	incrementalKurtosis
	incrementalDownsideVariance
	incrementalDownsideDeviation
)

// IncrementalStatistics keeps weighted moments in bounded native memory.
// Call Close when finished; Session.Close also releases any remaining objects.
type IncrementalStatistics struct{ object }

// NewIncrementalStatistics creates an empty accumulator in this session.
func (s *Session) NewIncrementalStatistics() (*IncrementalStatistics, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_incremental_statistics_new(s.ctx, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &IncrementalStatistics{object{s, uint64(id)}}, nil
}

// Add appends one finite observation and a finite nonnegative weight.
// A zero-weight observation still contributes to the sample count and extrema.
func (s *IncrementalStatistics) Add(value, weight float64) error {
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_incremental_statistics_add(s.session.ctx, C.uint64_t(s.id), C.double(value), C.double(weight), &e), &e)
	})
}

// AddBatch appends an entire batch atomically. Nil weights select unit weights.
func (s *IncrementalStatistics) AddBatch(values, weights []float64) error {
	if len(values) == 0 || len(values) > DefaultMaxOutputValues {
		return fmt.Errorf("itofin: incremental statistics batch size outside [1, %d]", DefaultMaxOutputValues)
	}
	if weights != nil && len(weights) != len(values) {
		return fmt.Errorf("itofin: incremental statistics observation and weight lengths differ")
	}
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_incremental_statistics_add_batch(s.session.ctx, C.uint64_t(s.id), doubles(values), C.size_t(len(values)), doubles(weights), C.size_t(len(weights)), &e), &e)
	})
}

// Reset clears observations while retaining the native handle.
func (s *IncrementalStatistics) Reset() error {
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_incremental_statistics_reset(s.session.ctx, C.uint64_t(s.id), &e), &e)
	})
}

func (s *IncrementalStatistics) count(which int) (int, error) {
	var out C.size_t
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_incremental_statistics_count(s.session.ctx, C.uint64_t(s.id), C.int32_t(which), &out, &e), &e)
	})
	return int(out), err
}

func (s *IncrementalStatistics) query(measure int) (float64, error) {
	var out C.double
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_incremental_statistics_query(s.session.ctx, C.uint64_t(s.id), C.int32_t(measure), &out, &e), &e)
	})
	return float64(out), err
}

// Samples returns the number of observations, including zero-weight observations.
func (s *IncrementalStatistics) Samples() (int, error) { return s.count(0) }

// DownsideSamples returns the number of strictly negative observations.
func (s *IncrementalStatistics) DownsideSamples() (int, error) { return s.count(1) }

// WeightSum returns the total sample weight.
func (s *IncrementalStatistics) WeightSum() (float64, error) { return s.query(incrementalWeightSum) }

// DownsideWeightSum returns the weight of strictly negative observations.
func (s *IncrementalStatistics) DownsideWeightSum() (float64, error) {
	return s.query(incrementalDownsideWeightSum)
}

// Min returns the lowest observation.
func (s *IncrementalStatistics) Min() (float64, error) { return s.query(incrementalMin) }

// Max returns the highest observation.
func (s *IncrementalStatistics) Max() (float64, error) { return s.query(incrementalMax) }

// Mean returns the weighted arithmetic mean.
func (s *IncrementalStatistics) Mean() (float64, error) { return s.query(incrementalMean) }

// Variance returns weighted variance with the count-based N/(N-1) correction.
func (s *IncrementalStatistics) Variance() (float64, error) { return s.query(incrementalVariance) }

// StandardDeviation returns the square root of sample variance.
func (s *IncrementalStatistics) StandardDeviation() (float64, error) {
	return s.query(incrementalStandardDeviation)
}

// ErrorEstimate returns the standard error of the mean.
func (s *IncrementalStatistics) ErrorEstimate() (float64, error) {
	return s.query(incrementalErrorEstimate)
}

// Skewness returns bias-corrected weighted skewness.
func (s *IncrementalStatistics) Skewness() (float64, error) { return s.query(incrementalSkewness) }

// Kurtosis returns bias-corrected excess kurtosis.
func (s *IncrementalStatistics) Kurtosis() (float64, error) { return s.query(incrementalKurtosis) }

// DownsideVariance returns the corrected second moment of negative observations.
func (s *IncrementalStatistics) DownsideVariance() (float64, error) {
	return s.query(incrementalDownsideVariance)
}

// DownsideDeviation returns the square root of downside variance.
func (s *IncrementalStatistics) DownsideDeviation() (float64, error) {
	return s.query(incrementalDownsideDeviation)
}
