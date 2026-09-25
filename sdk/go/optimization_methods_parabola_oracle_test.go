//go:build optimization_oracle

package itofin

import (
	"math"
	"testing"
)

func TestOptimizationMethodsQuantLibParabola(t *testing.T) {
	for _, variant := range []struct {
		name string
		kind int32
		new  func(*Session) (OptimizationMethod, error)
	}{
		{"simplex", 0, func(s *Session) (OptimizationMethod, error) { return s.NewSimplex(0.1) }},
		{"conjugate_gradient", 1, func(s *Session) (OptimizationMethod, error) { return s.NewConjugateGradient() }},
		{"steepest_descent", 2, func(s *Session) (OptimizationMethod, error) { return s.NewSteepestDescent() }},
	} {
		t.Run(variant.name, func(t *testing.T) {
			s := pricingMust(NewSession())
			defer s.Close()
			method := pricingMust(variant.new(s))
			result := pricingMust(s.optimizationParabolaOracle(method, variant.kind))
			if delta := math.Abs(result.bridgedX - result.coreX); delta > 1e-12 {
				t.Fatalf("x differs from same-input Rust core by %.17g: %+v", delta, result)
			}
			if delta := math.Abs(result.bridgedValue - result.coreValue); delta > 1e-12 {
				t.Fatalf("value differs from same-input Rust core by %.17g: %+v", delta, result)
			}
			if result.bridgedReason != result.coreReason {
				t.Fatalf("end criterion differs from Rust core: %+v", result)
			}
			if result.bridgedReason == EndCriteriaNone || result.bridgedReason == EndCriteriaMaxIterations || result.bridgedReason == EndCriteriaUnknown {
				t.Fatalf("QuantLib parabola did not converge: %+v", result)
			}
			if math.Abs(result.bridgedX+0.5) > 1e-8 && math.Abs(result.bridgedValue-0.75) > 1e-8 {
				t.Fatalf("QuantLib parabola minimum differs from analytic optimum: %+v", result)
			}
		})
	}
}
