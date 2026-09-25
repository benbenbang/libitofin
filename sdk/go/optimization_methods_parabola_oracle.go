//go:build optimization_oracle

package itofin

/*
#include "itofin.h"
typedef struct {
    double bridged_x;
    double bridged_value;
    int32_t bridged_reason;
    double core_x;
    double core_value;
    int32_t core_reason;
} ItofinOptimizationParabolaOracle;
int32_t itofin_optimization_parabola_oracle(
    ItofinContext *ctx, uint64_t method, int32_t kind,
    ItofinOptimizationParabolaOracle *out, ItofinError *error);
*/
import "C"

type optimizationParabolaResult struct {
	bridgedX, bridgedValue float64
	bridgedReason          EndCriteriaType
	coreX, coreValue       float64
	coreReason             EndCriteriaType
}

func (s *Session) optimizationParabolaOracle(method OptimizationMethod, kind int32) (optimizationParabolaResult, error) {
	o, err := optimizationMethodObject(method)
	if err != nil {
		return optimizationParabolaResult{}, err
	}
	if err := sameSession(s, o); err != nil {
		return optimizationParabolaResult{}, err
	}
	var result C.ItofinOptimizationParabolaOracle
	err = s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_optimization_parabola_oracle(s.ctx, C.uint64_t(o.id), C.int32_t(kind), &result, &e), &e)
	})
	if err != nil {
		return optimizationParabolaResult{}, err
	}
	return optimizationParabolaResult{
		bridgedX: float64(result.bridged_x), bridgedValue: float64(result.bridged_value),
		bridgedReason: EndCriteriaType(result.bridged_reason),
		coreX:         float64(result.core_x), coreValue: float64(result.core_value),
		coreReason: EndCriteriaType(result.core_reason),
	}, nil
}
