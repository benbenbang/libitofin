package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"fmt"
	"runtime/cgo"
	"unsafe"
)

func bootstrapFailure(e *C.ItofinError, err error) C.int32_t {
	e.code = C.ITOFIN_CORE_ERROR
	bytes := []byte(err.Error())
	if len(bytes) > len(e.message)-1 {
		bytes = bytes[:len(e.message)-1]
	}
	for i, b := range bytes {
		if b == 0 {
			b = '?'
		}
		e.message[i] = C.char(b)
	}
	e.message[len(bytes)] = 0
	return e.code
}

func runBootstrap(handle C.uintptr_t, out *C.ItofinBootstrapOutput, e *C.ItofinError, call func(bootstrapCallbacks) ([]float64, error)) (status C.int32_t) {
	defer func() {
		if value := recover(); value != nil {
			status = bootstrapFailure(e, fmt.Errorf("Go panic: %v", value))
		}
	}()
	callbacks := cgo.Handle(handle).Value().(bootstrapCallbacks)
	callbacks.session.callbackActive.Store(true)
	defer callbacks.session.callbackActive.Store(false)
	values, err := call(callbacks)
	if err != nil {
		return bootstrapFailure(e, err)
	}
	return C.itofin_bootstrap_output_set(out, (*C.double)(unsafe.Pointer(unsafe.SliceData(values))), C.size_t(len(values)), e)
}

func copyBootstrapValues(ptr *C.double, count C.size_t) []float64 {
	return append([]float64(nil), unsafe.Slice((*float64)(unsafe.Pointer(ptr)), int(count))...)
}

//export goBootstrapPenalty
func goBootstrapPenalty(handle C.uintptr_t, state *C.ItofinBootstrapState, out *C.ItofinBootstrapOutput, e *C.ItofinError) C.int32_t {
	return runBootstrap(handle, out, e, func(callbacks bootstrapCallbacks) ([]float64, error) {
		return callbacks.penalties(BootstrapState{
			Times: copyBootstrapValues(state.times, state.node_count), Data: copyBootstrapValues(state.data, state.node_count),
			QuoteValues: copyBootstrapValues(state.quote_values, state.quote_count), HelperErrors: copyBootstrapValues(state.helper_errors, state.helper_count),
		})
	})
}

//export goBootstrapDates
func goBootstrapDates(handle C.uintptr_t, state *C.ItofinBootstrapState, out *C.ItofinBootstrapOutput, e *C.ItofinError) C.int32_t {
	return runBootstrap(handle, out, e, func(callbacks bootstrapCallbacks) ([]float64, error) {
		dates, err := callbacks.dates()
		if err != nil {
			return nil, err
		}
		values := make([]float64, len(dates))
		for i, d := range dates {
			values[i] = float64(d.Serial())
		}
		return values, nil
	})
}

//export goBootstrapRelease
func goBootstrapRelease(handle C.uintptr_t) { cgo.Handle(handle).Delete() }
