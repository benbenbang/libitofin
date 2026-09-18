package itofin

/*
#include "itofin.h"
*/
import "C"
import "unsafe"

// Results is a detached valuation snapshot. Nil fields were not supplied by the
// engine. AdditionalResults contains only real-valued engine outputs.
// Mutating this Go value cannot affect the instrument or another snapshot.
type Results struct {
	NPV               *float64
	ErrorEstimate     *float64
	ValuationDate     *Date
	AdditionalResults map[string]float64
}

// readResults consumes a native snapshot handle; call only on the session worker.
func (s *Session) readResults(id uint64) (*Results, error) {
	defer func() { var e C.ItofinError; C.itofin_handle_release(s.ctx, C.uint64_t(id), &e) }()
	var native C.ItofinResultsFields
	var e C.ItofinError
	if err := ffiError(C.itofin_results_fields(s.ctx, C.uint64_t(id), &native, &e), &e); err != nil {
		return nil, err
	}
	result := &Results{AdditionalResults: make(map[string]float64, int(native.additional_count))}
	if native.has_npv != 0 {
		v := float64(native.npv)
		result.NPV = &v
	}
	if native.has_error_estimate != 0 {
		v := float64(native.error_estimate)
		result.ErrorEstimate = &v
	}
	if native.has_valuation_date != 0 {
		d, err := DateFromSerial(int32(native.valuation_date))
		if err != nil {
			return nil, err
		}
		result.ValuationDate = &d
	}
	for i := C.size_t(0); i < native.additional_count; i++ {
		var size C.size_t
		var value C.double
		if err := ffiError(C.itofin_results_additional(s.ctx, C.uint64_t(id), i, nil, 0, &size, &value, &e), &e); err != nil {
			return nil, err
		}
		key := make([]byte, int(size))
		var ptr *C.uint8_t
		if size > 0 {
			ptr = (*C.uint8_t)(unsafe.Pointer(&key[0]))
		}
		if err := ffiError(C.itofin_results_additional(s.ctx, C.uint64_t(id), i, ptr, size, &size, &value, &e), &e); err != nil {
			return nil, err
		}
		result.AdditionalResults[string(key)] = float64(value)
	}
	return result, nil
}
