package itofin

/*
#include "itofin.h"
*/
import "C"

// DrawdownResult contains a nonnegative fractional loss and zero-based indices.
// It is an owned snapshot with no session, handles, or cleanup requirement.
type DrawdownResult struct {
	Drawdown    float64 `json:"drawdown"`
	PeakIndex   int     `json:"peak_index"`
	TroughIndex int     `json:"trough_index"`
}

// MaximumDrawdown evaluates ordered finite strictly positive equity/NAV values.
// Empty input is an error. One value or no decline returns zero and indices 0,0.
// Equal peaks retain their earliest index; equal losses retain the first trough.
// Extreme peak/trough ratios can round Drawdown to 1. Returns are not NAVs.
func MaximumDrawdown(values []float64) (DrawdownResult, error) {
	var out C.ItofinDrawdownResult
	var e C.ItofinError
	status := C.itofin_maximum_drawdown(doubles(values), C.size_t(len(values)), &out, &e)
	if err := ffiError(status, &e); err != nil {
		return DrawdownResult{}, err
	}
	return DrawdownResult{float64(out.drawdown), int(out.peak_index), int(out.trough_index)}, nil
}
