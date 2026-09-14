package itofin

/*
#include "itofin.h"
*/
import "C"

type Settings struct{ object }

func (s *Session) NewSettings() (*Settings, error) {
	var id C.uint64_t
	err := s.invoke(func() error { var e C.ItofinError; return ffiError(C.itofin_settings_new(s.ctx, &id, &e), &e) })
	if err != nil {
		return nil, err
	}
	return &Settings{object{s, uint64(id)}}, nil
}
func (s *Settings) SetEvaluationDate(d Date) error {
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_settings_set_evaluation_date(s.session.ctx, C.uint64_t(s.id), C.int32_t(d.serial), &e), &e)
	})
}

// SetIncludeTodaysCashFlows accepts nil to restore the unset, engine-default state.
func (s *Settings) SetIncludeTodaysCashFlows(value *bool) error {
	flag := C.int32_t(-1)
	if value != nil {
		flag = 0
		if *value {
			flag = 1
		}
	}
	return s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_settings_set_include_todays_cash_flows(s.session.ctx, C.uint64_t(s.id), flag, &e), &e)
	})
}
func (s *Settings) IncludeTodaysCashFlows() (*bool, error) {
	var out C.int32_t
	err := s.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_settings_include_todays_cash_flows(s.session.ctx, C.uint64_t(s.id), &out, &e), &e)
	})
	if err != nil || out < 0 {
		return nil, err
	}
	value := out != 0
	return &value, nil
}
