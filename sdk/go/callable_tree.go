package itofin

/*
#include "itofin.h"
*/
import "C"

type TreeCallableFixedRateBondEngine struct{ object }

func (s *Session) NewTreeCallableFixedRateBondEngine(model *HullWhite, timeSteps uint, settings *Settings) (*TreeCallableFixedRateBondEngine, error) {
	if model == nil || settings == nil {
		return nil, errNilArgument("model and settings")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, model.object, settings.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_tree_callable_bond_engine_new(s.ctx, C.uint64_t(model.id), C.uint64_t(timeSteps), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &TreeCallableFixedRateBondEngine{object{s, uint64(id)}}, nil
}
func setCallableTreeEngine(b *Bond, engine *TreeCallableFixedRateBondEngine) error {
	if b == nil || b.session == nil || engine == nil {
		return errNilArgument("bond/session and tree engine")
	}
	s := b.session
	return s.invoke(func() error {
		if err := sameSession(s, b.object, engine.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_callable_bond_set_tree_engine(s.ctx, C.uint64_t(b.id), C.uint64_t(engine.id), &e), &e)
	})
}
func (b *CallableFixedRateBond) SetTreeEngine(engine *TreeCallableFixedRateBondEngine) error {
	if b == nil {
		return errNilArgument("bond")
	}
	return setCallableTreeEngine(&b.Bond, engine)
}
func (b *CallableZeroCouponBond) SetTreeEngine(engine *TreeCallableFixedRateBondEngine) error {
	if b == nil {
		return errNilArgument("bond")
	}
	return setCallableTreeEngine(&b.Bond, engine)
}
