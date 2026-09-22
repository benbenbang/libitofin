package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"runtime"
)

// OvernightCapFloorConfig uses Compound averaging, the index day counter,
// configurable payment adjustment/lag. Nominal is explicit.
type OvernightCapFloorConfig struct {
	Type                 CapFloorType
	Schedule             *Schedule
	Index                *OvernightIndex
	Settings             *Settings
	Nominal              float64
	PaymentLag           int32
	PaymentAdjustment    BusinessDayConvention
	CapRates, FloorRates []float64
}

func (s *Session) NewOvernightCapFloor(x OvernightCapFloorConfig) (*CapFloor, error) {
	if x.Schedule == nil || x.Index == nil || x.Settings == nil {
		return nil, fmt.Errorf("schedule, overnight index and settings are required")
	}
	if err := sameSession(s, x.Schedule.object, x.Index.object, x.Settings.object); err != nil {
		return nil, err
	}
	cfg := C.ItofinOvernightCapFloorConfig{kind: C.int32_t(x.Type), schedule: C.uint64_t(x.Schedule.id), index: C.uint64_t(x.Index.id), settings: C.uint64_t(x.Settings.id), nominal: C.double(x.Nominal), payment_lag: C.int32_t(x.PaymentLag), payment_adjustment: C.int32_t(x.PaymentAdjustment), caps: ratesPtr(x.CapRates), cap_count: C.size_t(len(x.CapRates)), floors: ratesPtr(x.FloorRates), floor_count: C.size_t(len(x.FloorRates))}
	var id C.uint64_t
	err := s.invoke(func() error {
		var pin runtimePinner
		pin.pin(x.CapRates, x.FloorRates)
		defer pin.unpin()
		var e C.ItofinError
		return ffiError(C.itofin_overnight_capfloor_new(s.ctx, &cfg, &id, &e), &e)
	})
	runtime.KeepAlive(x.CapRates)
	runtime.KeepAlive(x.FloorRates)
	if err != nil {
		return nil, err
	}
	return &CapFloor{object{s, uint64(id)}}, nil
}
