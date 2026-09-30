package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"math"
)

const (
	statisticsMean              = 0
	statisticsSampleVariance    = 1
	statisticsStandardDeviation = 2
	statisticsPercentile        = 3
	statisticsValueAtRisk       = 4
	statisticsExpectedShortfall = 5
)

func statisticsEvaluate(observations, weights []float64, measure int, probability float64) (float64, error) {
	if len(observations) == 0 {
		return 0, fmt.Errorf("itofin: statistics requires at least one observation")
	}
	if len(observations) > DefaultMaxOutputValues {
		return 0, fmt.Errorf("itofin: statistics input exceeds size limit")
	}
	if weights != nil && len(weights) != len(observations) {
		return 0, fmt.Errorf("itofin: statistics observation and weight lengths differ")
	}
	for i, observation := range observations {
		if math.IsNaN(observation) || math.IsInf(observation, 0) {
			return 0, fmt.Errorf("itofin: nonfinite statistics observation at %d", i)
		}
	}
	if weights != nil {
		weightSum := 0.0
		for i, weight := range weights {
			if math.IsNaN(weight) || math.IsInf(weight, 0) || weight < 0 {
				return 0, fmt.Errorf("itofin: invalid statistics weight at %d", i)
			}
			weightSum += weight
		}
		if weightSum <= 0 || math.IsInf(weightSum, 0) {
			return 0, fmt.Errorf("itofin: statistics weights must have a finite positive total")
		}
	}
	if measure == statisticsPercentile && !(probability > 0 && probability <= 1) {
		return 0, fmt.Errorf("itofin: statistics percentile must be in (0, 1]")
	}
	if (measure == statisticsValueAtRisk || measure == statisticsExpectedShortfall) && !(probability >= 0.9 && probability < 1) {
		return 0, fmt.Errorf("itofin: statistics confidence must be in [0.9, 1)")
	}
	var out C.double
	var e C.ItofinError
	status := C.itofin_statistics_evaluate(
		doubles(observations), C.size_t(len(observations)),
		doubles(weights), C.size_t(len(weights)), C.int32_t(measure),
		C.double(probability), &out, &e,
	)
	if err := ffiError(status, &e); err != nil {
		return 0, err
	}
	return float64(out), nil
}

// StatisticsMean returns the weighted mean of signed observations.
// A nil weights slice gives every observation unit weight.
func StatisticsMean(observations, weights []float64) (float64, error) {
	return statisticsEvaluate(observations, weights, statisticsMean, 0)
}

// StatisticsSampleVariance returns weighted population variance scaled by
// N/(N-1), where N is the observation count. It requires two observations.
func StatisticsSampleVariance(observations, weights []float64) (float64, error) {
	return statisticsEvaluate(observations, weights, statisticsSampleVariance, 0)
}

// StatisticsStandardDeviation returns the square root of the count-corrected
// weighted sample variance. It requires two observations.
func StatisticsStandardDeviation(observations, weights []float64) (float64, error) {
	return statisticsEvaluate(observations, weights, statisticsStandardDeviation, 0)
}

// StatisticsPercentile returns the weighted empirical percentile for
// probability in (0, 1], including the first value reaching cumulative weight.
func StatisticsPercentile(observations, weights []float64, probability float64) (float64, error) {
	return statisticsEvaluate(observations, weights, statisticsPercentile, probability)
}

// StatisticsValueAtRisk returns a nonnegative loss magnitude at confidence in
// [0.9, 1). Negative observations are losses.
func StatisticsValueAtRisk(observations, weights []float64, probability float64) (float64, error) {
	return statisticsEvaluate(observations, weights, statisticsValueAtRisk, probability)
}

// StatisticsExpectedShortfall returns a nonnegative mean loss strictly below
// the value-at-risk threshold. It errors when the strict tail is empty.
func StatisticsExpectedShortfall(observations, weights []float64, probability float64) (float64, error) {
	return statisticsEvaluate(observations, weights, statisticsExpectedShortfall, probability)
}
