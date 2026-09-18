package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type MakeYoYInflationCapFloor struct{ object }
type YoYInflationCapFloor struct{ object }
type MakeYoYInflationCapFloorConfig struct {
	Type                CapFloorType
	Index               *YoYInflationIndex
	Length              uint64
	Calendar            *Calendar
	ObservationLag      Period
	Interpolation       CpiInterpolationType
	Settings            *Settings
	Nominal             *float64
	EffectiveDate       *Date
	PaymentDayCounter   *DayCounter
	PaymentAdjustment   *BusinessDayConvention
	FixingDays          *uint32
	Engine              *YoYInflationCapFloorEngine
	AsOptionlet         bool
	ForwardStart        *Period
	FirstCapletExcluded bool
	Strike              *float64
	ATMStrike           *YieldTermStructure
}

func (s *Session) NewMakeYoYInflationCapFloor(a MakeYoYInflationCapFloorConfig) (*MakeYoYInflationCapFloor, error) {
	if a.Index == nil || a.Calendar == nil || a.Settings == nil {
		return nil, fmt.Errorf("index, calendar and settings required")
	}
	objects := []object{a.Index.object, a.Calendar.object, a.Settings.object}
	cfg := C.ItofinMakeYoYCapFloorConfig{kind: C.int32_t(a.Type), index: C.uint64_t(a.Index.id), length: C.size_t(a.Length), calendar: C.uint64_t(a.Calendar.id), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), settings: C.uint64_t(a.Settings.id), payment_convention: -1}
	if uint64(cfg.length) != a.Length {
		return nil, fmt.Errorf("length overflows native size")
	}
	if a.Nominal != nil {
		cfg.flags |= 1
		cfg.nominal = C.double(*a.Nominal)
	}
	if a.EffectiveDate != nil {
		cfg.flags |= 2
		cfg.effective_date = C.int32_t(a.EffectiveDate.serial)
	}
	if a.PaymentDayCounter != nil {
		objects = append(objects, a.PaymentDayCounter.object)
		cfg.payment_day_counter = C.uint64_t(a.PaymentDayCounter.id)
	}
	if a.PaymentAdjustment != nil {
		cfg.payment_convention = C.int32_t(*a.PaymentAdjustment)
	}
	if a.FixingDays != nil {
		cfg.flags |= 4
		cfg.fixing_days = C.uint32_t(*a.FixingDays)
	}
	if a.Engine != nil {
		objects = append(objects, a.Engine.object)
		cfg.engine = C.uint64_t(a.Engine.id)
	}
	if a.AsOptionlet {
		cfg.as_optionlet = 1
	}
	if a.ForwardStart != nil {
		cfg.flags |= 8
		cfg.forward_length = C.int32_t(a.ForwardStart.Length)
		cfg.forward_unit = C.int32_t(a.ForwardStart.Unit)
	}
	if a.FirstCapletExcluded {
		cfg.exclude_first = 1
	}
	if a.Strike != nil {
		cfg.flags |= 16
		cfg.strike = C.double(*a.Strike)
	}
	if a.ATMStrike != nil {
		objects = append(objects, a.ATMStrike.object)
		cfg.atm_curve = C.uint64_t(a.ATMStrike.id)
	}
	if err := sameSession(s, objects...); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_make_yoy_capfloor_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &MakeYoYInflationCapFloor{object{s, uint64(id)}}, nil
}
func (v *MakeYoYInflationCapFloor) Build() (*YoYInflationCapFloor, error) {
	var id C.uint64_t
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_make_yoy_capfloor_build(v.session.ctx, C.uint64_t(v.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YoYInflationCapFloor{object{v.session, uint64(id)}}, nil
}
func (s *Session) NewYoYInflationCapFloor(kind CapFloorType, coupons []*YoYInflationCoupon, capRates, floorRates []float64, settings *Settings) (*YoYInflationCapFloor, error) {
	if settings == nil {
		return nil, fmt.Errorf("settings required")
	}
	objects := []object{settings.object}
	ids := make([]uint64, len(coupons))
	for i, v := range coupons {
		if v == nil {
			return nil, fmt.Errorf("nil coupon")
		}
		objects = append(objects, v.object)
		ids[i] = v.id
	}
	if err := sameSession(s, objects...); err != nil {
		return nil, err
	}
	var out C.uint64_t
	err := s.invoke(func() error {
		var ptr *C.uint64_t
		if len(ids) > 0 {
			ptr = (*C.uint64_t)(unsafe.Pointer(&ids[0]))
		}
		var e C.ItofinError
		return ffiError(C.itofin_yoy_capfloor_new(s.ctx, C.int32_t(kind), ptr, C.size_t(len(ids)), inflationDoubles(capRates), C.size_t(len(capRates)), inflationDoubles(floorRates), C.size_t(len(floorRates)), C.uint64_t(settings.id), &out, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YoYInflationCapFloor{object{s, uint64(out)}}, nil
}
func (s *Session) NewYoYInflationCap(coupons []*YoYInflationCoupon, strikes []float64, settings *Settings) (*YoYInflationCapFloor, error) {
	return s.NewYoYInflationCapFloor(CapType, coupons, strikes, nil, settings)
}
func (s *Session) NewYoYInflationFloor(coupons []*YoYInflationCoupon, strikes []float64, settings *Settings) (*YoYInflationCapFloor, error) {
	return s.NewYoYInflationCapFloor(FloorType, coupons, nil, strikes, settings)
}
func (s *Session) NewYoYInflationCollar(coupons []*YoYInflationCoupon, caps, floors []float64, settings *Settings) (*YoYInflationCapFloor, error) {
	return s.NewYoYInflationCapFloor(CollarType, coupons, caps, floors, settings)
}
func (s *Session) NewYoYInflationCapFloorWithStrikes(kind CapFloorType, coupons []*YoYInflationCoupon, strikes []float64, settings *Settings) (*YoYInflationCapFloor, error) {
	switch kind {
	case CapType:
		return s.NewYoYInflationCap(coupons, strikes, settings)
	case FloorType:
		return s.NewYoYInflationFloor(coupons, strikes, settings)
	}
	return nil, fmt.Errorf("single strike constructor requires cap or floor")
}
func (v *YoYInflationCapFloor) SetEngine(engine *YoYInflationCapFloorEngine) error {
	if engine == nil {
		return fmt.Errorf("engine required")
	}
	if err := sameSession(v.session, engine.object); err != nil {
		return err
	}
	return v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_capfloor_set_engine(v.session.ctx, C.uint64_t(v.id), C.uint64_t(engine.id), &e), &e)
	})
}
func (v *YoYInflationCapFloor) query(query int32, discount uint64) (float64, error) {
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_yoy_capfloor_query(v.session.ctx, C.uint64_t(v.id), C.int32_t(query), C.uint64_t(discount), &out, &e), &e)
	})
	return float64(out), err
}
func (v *YoYInflationCapFloor) NPV() (float64, error) { return v.query(0, 0) }
func (v *YoYInflationCapFloor) Calculate() error      { _, err := v.query(1, 0); return err }
func (v *YoYInflationCapFloor) IsCalculated() (bool, error) {
	n, err := v.query(2, 0)
	return n != 0, err
}
func (v *YoYInflationCapFloor) CouponCount() (int, error) {
	n, err := v.query(3, 0)
	return int(n), err
}
func (v *YoYInflationCapFloor) StartDate() (Date, error) {
	n, err := v.query(4, 0)
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(n))
}
func (v *YoYInflationCapFloor) MaturityDate() (Date, error) {
	n, err := v.query(5, 0)
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(n))
}
func (v *YoYInflationCapFloor) ATMRate(discount *YieldTermStructure) (float64, error) {
	if discount == nil {
		return 0, fmt.Errorf("discount curve required")
	}
	if err := sameSession(v.session, discount.object); err != nil {
		return 0, err
	}
	return v.query(6, discount.id)
}
func (v *YoYInflationCapFloor) Price(engine *YoYInflationCapFloorEngine) (float64, error) {
	if engine == nil {
		return 0, fmt.Errorf("engine required")
	}
	if err := sameSession(v.session, engine.object); err != nil {
		return 0, err
	}
	var out C.double
	err := v.session.invoke(func() error {
		var e C.ItofinError
		if err := ffiError(C.itofin_yoy_capfloor_set_engine(v.session.ctx, C.uint64_t(v.id), C.uint64_t(engine.id), &e), &e); err != nil {
			return err
		}
		return ffiError(C.itofin_yoy_capfloor_query(v.session.ctx, C.uint64_t(v.id), 0, 0, &out, &e), &e)
	})
	return float64(out), err
}
func (v *YoYInflationCapFloor) Results() (*Results, error) {
	var result *Results
	err := v.session.invoke(func() error {
		var id C.uint64_t
		var e C.ItofinError
		if err := ffiError(C.itofin_yoy_capfloor_results(v.session.ctx, C.uint64_t(v.id), &id, &e), &e); err != nil {
			return err
		}
		var err error
		result, err = v.session.readResults(uint64(id))
		return err
	})
	return result, err
}
func (v *YoYInflationCapFloor) strikes(floor bool) ([]float64, error) {
	var result []float64
	var flag C.uint8_t
	if floor {
		flag = 1
	}
	err := v.session.invoke(func() error {
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_yoy_capfloor_strikes(v.session.ctx, C.uint64_t(v.id), flag, nil, 0, &n, &e), &e); err != nil {
			return err
		}
		result = make([]float64, int(n))
		if n == 0 {
			return nil
		}
		return ffiError(C.itofin_yoy_capfloor_strikes(v.session.ctx, C.uint64_t(v.id), flag, inflationDoubles(result), n, &n, &e), &e)
	})
	return result, err
}
func (v *YoYInflationCapFloor) CapRates() ([]float64, error)   { return v.strikes(false) }
func (v *YoYInflationCapFloor) FloorRates() ([]float64, error) { return v.strikes(true) }
