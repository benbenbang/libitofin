package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

const (
	generalMin = iota
	generalMax
	generalMean
	generalVariance
	generalStandardDeviation
	generalErrorEstimate
	generalSkewness
	generalKurtosis
	generalPercentile
	generalTopPercentile
	generalSemiVariance
	generalSemiDeviation
	generalDownsideVariance
	generalDownsideDeviation
	generalRegret
	generalPotentialUpside
	generalValueAtRisk
	generalExpectedShortfall
	generalShortfall
	generalAverageShortfall
)

// GeneralStatistics retains weighted observations in a Session.
// A zero-weight observation still counts toward moment corrections.
type GeneralStatistics struct{ object }

// NewGeneralStatistics creates an empty, session-owned accumulator.
func (s *Session) NewGeneralStatistics() (*GeneralStatistics, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_general_statistics_new(s.ctx, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &GeneralStatistics{object{s, uint64(id)}}, nil
}

// Add appends one finite observation with a finite nonnegative weight.
func (s *GeneralStatistics) Add(value, weight float64) error {
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_general_statistics_add(s.session.ctx, C.uint64_t(s.id), C.double(value), C.double(weight), &e), &e)
	})
}

// AddBatch appends all observations or none on error. Nil weights select unit weights.
func (s *GeneralStatistics) AddBatch(observations, weights []float64) error {
	if len(observations) > DefaultMaxOutputValues {
		return fmt.Errorf("itofin: statistics input exceeds size limit")
	}
	if weights != nil && len(weights) != len(observations) {
		return fmt.Errorf("itofin: statistics observation and weight lengths differ")
	}
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_general_statistics_add_batch(
			s.session.ctx, C.uint64_t(s.id), doubles(observations), C.size_t(len(observations)),
			doubles(weights), C.size_t(len(weights)), &e,
		), &e)
	})
}

// Reset removes all observations.
func (s *GeneralStatistics) Reset() error {
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_general_statistics_reset(s.session.ctx, C.uint64_t(s.id), &e), &e)
	})
}

// Samples returns the number of observations, including zero-weight observations.
func (s *GeneralStatistics) Samples() (int, error) {
	var count C.size_t
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_general_statistics_summary(s.session.ctx, C.uint64_t(s.id), &count, nil, &e), &e)
	})
	return int(count), err
}

// WeightSum returns the total observation weight.
func (s *GeneralStatistics) WeightSum() (float64, error) {
	var weight C.double
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_general_statistics_summary(s.session.ctx, C.uint64_t(s.id), nil, &weight, &e), &e)
	})
	return float64(weight), err
}

func (s *GeneralStatistics) query(selector int, argument float64) (float64, error) {
	var result C.double
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_general_statistics_query(s.session.ctx, C.uint64_t(s.id), C.int32_t(selector), C.double(argument), &result, &e), &e)
	})
	return float64(result), err
}

func (s *GeneralStatistics) Min() (float64, error)      { return s.query(generalMin, 0) }
func (s *GeneralStatistics) Max() (float64, error)      { return s.query(generalMax, 0) }
func (s *GeneralStatistics) Mean() (float64, error)     { return s.query(generalMean, 0) }
func (s *GeneralStatistics) Variance() (float64, error) { return s.query(generalVariance, 0) }
func (s *GeneralStatistics) StandardDeviation() (float64, error) {
	return s.query(generalStandardDeviation, 0)
}
func (s *GeneralStatistics) ErrorEstimate() (float64, error) { return s.query(generalErrorEstimate, 0) }
func (s *GeneralStatistics) Skewness() (float64, error)      { return s.query(generalSkewness, 0) }
func (s *GeneralStatistics) Kurtosis() (float64, error)      { return s.query(generalKurtosis, 0) }
func (s *GeneralStatistics) Percentile(probability float64) (float64, error) {
	return s.query(generalPercentile, probability)
}
func (s *GeneralStatistics) TopPercentile(probability float64) (float64, error) {
	return s.query(generalTopPercentile, probability)
}
func (s *GeneralStatistics) SemiVariance() (float64, error)  { return s.query(generalSemiVariance, 0) }
func (s *GeneralStatistics) SemiDeviation() (float64, error) { return s.query(generalSemiDeviation, 0) }
func (s *GeneralStatistics) DownsideVariance() (float64, error) {
	return s.query(generalDownsideVariance, 0)
}
func (s *GeneralStatistics) DownsideDeviation() (float64, error) {
	return s.query(generalDownsideDeviation, 0)
}
func (s *GeneralStatistics) Regret(target float64) (float64, error) {
	return s.query(generalRegret, target)
}
func (s *GeneralStatistics) PotentialUpside(confidence float64) (float64, error) {
	return s.query(generalPotentialUpside, confidence)
}
func (s *GeneralStatistics) ValueAtRisk(confidence float64) (float64, error) {
	return s.query(generalValueAtRisk, confidence)
}
func (s *GeneralStatistics) ExpectedShortfall(confidence float64) (float64, error) {
	return s.query(generalExpectedShortfall, confidence)
}
func (s *GeneralStatistics) Shortfall(target float64) (float64, error) {
	return s.query(generalShortfall, target)
}
func (s *GeneralStatistics) AverageShortfall(target float64) (float64, error) {
	return s.query(generalAverageShortfall, target)
}
