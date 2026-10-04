package itofin

/*
#include "itofin.h"
*/
import "C"

import "fmt"

// ConvergencePoint records a running weighted mean at a completed sample checkpoint.
type ConvergencePoint struct {
	Samples int
	Mean    float64
}

func convergenceEntries(samples int) (int, error) {
	if samples < 0 || samples > 100000 {
		return 0, fmt.Errorf("itofin: convergence statistics requires at most 100000 observations")
	}
	entries := 0
	for checkpoint := 1; checkpoint <= samples; checkpoint = 2*checkpoint + 1 {
		entries++
	}
	return entries, nil
}

func convergenceWeights(observations, weights []float64) error {
	if weights != nil && len(weights) != len(observations) {
		return fmt.Errorf("itofin: convergence observation and weight lengths differ")
	}
	return nil
}

func convergenceCounts(values []C.size_t) *C.size_t {
	if len(values) == 0 {
		return nil
	}
	return &values[0]
}

func convergencePoints(counts []C.size_t, means []float64) []ConvergencePoint {
	points := make([]ConvergencePoint, len(counts))
	for i, count := range counts {
		points[i] = ConvergencePoint{Samples: int(count), Mean: means[i]}
	}
	return points
}

// StatisticsConvergence returns weighted means at completed counts 1, 3, 7, 15, ... .
// Nil weights select unit weights. Empty input returns an empty table. Each checkpoint
// requires positive cumulative weight. Observations are limited to 100000.
func StatisticsConvergence(observations, weights []float64) ([]ConvergencePoint, error) {
	entries, err := convergenceEntries(len(observations))
	if err != nil {
		return nil, err
	}
	if err := convergenceWeights(observations, weights); err != nil {
		return nil, err
	}
	counts, means := make([]C.size_t, entries), make([]float64, entries)
	var e C.ItofinError
	status := C.itofin_convergence_statistics_evaluate(
		doubles(observations), C.size_t(len(observations)), doubles(weights), C.size_t(len(weights)),
		convergenceCounts(counts), doubles(means), C.size_t(entries), &e,
	)
	if err := ffiError(status, &e); err != nil {
		return nil, err
	}
	return convergencePoints(counts, means), nil
}

// ConvergenceStatistics retains a bounded weighted running-mean convergence table.
// It is session-owned and serializes concurrent calls on the session worker.
type ConvergenceStatistics struct{ object }

// NewConvergenceStatistics creates an empty session-owned accumulator.
func (s *Session) NewConvergenceStatistics() (*ConvergenceStatistics, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_convergence_statistics_new(s.ctx, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &ConvergenceStatistics{object{s, uint64(id)}}, nil
}

// Add atomically appends one finite observation with finite nonnegative weight.
// Zero weight still counts, but a zero-total-weight checkpoint is rejected.
func (s *ConvergenceStatistics) Add(value, weight float64) error {
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_convergence_statistics_add(s.session.ctx, C.uint64_t(s.id), C.double(value), C.double(weight), &e), &e)
	})
}

// AddBatch appends all observations or none on error. Nil weights selects unit weights.
func (s *ConvergenceStatistics) AddBatch(observations, weights []float64) error {
	if _, err := convergenceEntries(len(observations)); err != nil {
		return err
	}
	if err := convergenceWeights(observations, weights); err != nil {
		return err
	}
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_convergence_statistics_add_batch(
			s.session.ctx, C.uint64_t(s.id), doubles(observations), C.size_t(len(observations)),
			doubles(weights), C.size_t(len(weights)), &e,
		), &e)
	})
}

// Reset removes all observations and completed checkpoints.
func (s *ConvergenceStatistics) Reset() error {
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_convergence_statistics_reset(s.session.ctx, C.uint64_t(s.id), &e), &e)
	})
}

// Samples returns observation count, including zero-weight observations.
func (s *ConvergenceStatistics) Samples() (int, error) {
	var count C.size_t
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_convergence_statistics_summary(s.session.ctx, C.uint64_t(s.id), &count, nil, nil, &e), &e)
	})
	return int(count), err
}

// WeightSum returns cumulative observation weight.
func (s *ConvergenceStatistics) WeightSum() (float64, error) {
	var weight C.double
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_convergence_statistics_summary(s.session.ctx, C.uint64_t(s.id), nil, &weight, nil, &e), &e)
	})
	return float64(weight), err
}

// Mean returns the current weighted mean, including samples after the last checkpoint.
func (s *ConvergenceStatistics) Mean() (float64, error) {
	var mean C.double
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_convergence_statistics_mean(s.session.ctx, C.uint64_t(s.id), &mean, &e), &e)
	})
	return float64(mean), err
}

// Table returns an independent snapshot of the completed checkpoints.
func (s *ConvergenceStatistics) Table() ([]ConvergencePoint, error) {
	var points []ConvergencePoint
	err := s.session.invoke(func() error {
		var entries C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_convergence_statistics_summary(s.session.ctx, C.uint64_t(s.id), nil, nil, &entries, &e), &e); err != nil {
			return err
		}
		counts, means := make([]C.size_t, int(entries)), make([]float64, int(entries))
		if err := ffiError(C.itofin_convergence_statistics_table(s.session.ctx, C.uint64_t(s.id), convergenceCounts(counts), doubles(means), entries, &e), &e); err != nil {
			return err
		}
		points = convergencePoints(counts, means)
		return nil
	})
	return points, err
}
