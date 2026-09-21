package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type BachelierCapFloorEngine struct{ object }
type TreeCapFloorEngine struct{ object }

type BachelierCapFloorEngineConfig struct {
	Volatility *OptionletVolatilityStructure
	Discount   *YieldTermStructure
}

func (s *Session) NewBachelierCapFloorEngineFlat(a RateEngineFlatVolConfig) (*BachelierCapFloorEngine, error) {
	o, err := s.rateEngineFlat(a, 3)
	if err != nil {
		return nil, err
	}
	return &BachelierCapFloorEngine{o}, nil
}
func (s *Session) NewBachelierCapFloorEngine(a BachelierCapFloorEngineConfig) (*BachelierCapFloorEngine, error) {
	if a.Discount == nil || a.Volatility == nil {
		return nil, fmt.Errorf("discount and volatility required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Discount.object, a.Volatility.object); err != nil {
			return err
		}
		c := C.ItofinRateEngineConfig{kind: 3, discount: C.uint64_t(a.Discount.id), volatility: C.uint64_t(a.Volatility.id)}
		var e C.ItofinError
		return ffiError(C.itofin_rate_engine_new(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BachelierCapFloorEngine{object{s, uint64(id)}}, nil
}
func (s *Session) NewTreeCapFloorEngine(model *HullWhite, steps uint) (*TreeCapFloorEngine, error) {
	return s.treeCapFloorEngine(model, steps, nil)
}
func (s *Session) NewTreeCapFloorEngineWithTimeGrid(model *HullWhite, times []float64) (*TreeCapFloorEngine, error) {
	if len(times) == 0 {
		return nil, fmt.Errorf("time grid required")
	}
	return s.treeCapFloorEngine(model, 0, times)
}
func (s *Session) treeCapFloorEngine(model *HullWhite, steps uint, times []float64) (*TreeCapFloorEngine, error) {
	if model == nil {
		return nil, errNilArgument("model")
	}
	values := make([]C.double, len(times))
	for i, v := range times {
		values[i] = C.double(v)
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, model.object); err != nil {
			return err
		}
		var ptr *C.double
		if len(values) > 0 {
			ptr = &values[0]
		}
		var e C.ItofinError
		return ffiError(C.itofin_tree_capfloor_engine_new(s.ctx, C.uint64_t(model.id), C.size_t(steps), ptr, C.size_t(len(values)), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &TreeCapFloorEngine{object{s, uint64(id)}}, nil
}
func (o *CapFloor) SetBachelierEngine(e *BachelierCapFloorEngine) error {
	if o == nil || e == nil {
		return errNilArgument("cap/floor and engine")
	}
	return rateOptionSet(o.object, e.object, 4)
}
func (o *CapFloor) SetTreeEngine(e *TreeCapFloorEngine) error {
	if o == nil || e == nil {
		return errNilArgument("cap/floor and engine")
	}
	return rateOptionSet(o.object, e.object, 5)
}
func (o *CapFloor) PriceBachelier(e *BachelierCapFloorEngine) (float64, error) {
	if o == nil || e == nil {
		return 0, errNilArgument("cap/floor and engine")
	}
	return rateOptionPrice(o.object, e.object, 4, 1)
}
func (o *CapFloor) PriceTree(e *TreeCapFloorEngine) (float64, error) {
	if o == nil || e == nil {
		return 0, errNilArgument("cap/floor and engine")
	}
	return rateOptionPrice(o.object, e.object, 5, 1)
}
