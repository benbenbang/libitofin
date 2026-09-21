package itofin

/*
#include "itofin.h"
*/
import "C"
import (
	"fmt"
	"unsafe"
)

type IborIborBasisSwapRateHelper = RateHelper

type BasisHelperConfig struct {
	Quote                 *SimpleQuote
	Tenor                 Period
	SettlementDays        uint32
	Calendar              *Calendar
	Convention            BusinessDayConvention
	EndOfMonth            bool
	BaseIndex, OtherIndex *IborIndex
	DiscountCurve         *YieldTermStructure
	BootstrapBaseCurve    bool
}

func (s *Session) NewIborIborBasisSwapRateHelper(cfg BasisHelperConfig) (*RateHelper, error) {
	if cfg.Quote == nil || cfg.Calendar == nil || cfg.BaseIndex == nil || cfg.OtherIndex == nil || cfg.DiscountCurve == nil {
		return nil, errNilArgument("basis helper input")
	}
	if err := sameSession(s, cfg.Quote.object, cfg.Calendar.object, cfg.BaseIndex.object, cfg.OtherIndex.object, cfg.DiscountCurve.object); err != nil {
		return nil, err
	}
	c := C.ItofinBasisHelperConfig{quote: C.uint64_t(cfg.Quote.id), tenor_length: C.int32_t(cfg.Tenor.Length), tenor_unit: C.int32_t(cfg.Tenor.Unit), settlement_days: C.uint32_t(cfg.SettlementDays), calendar: C.uint64_t(cfg.Calendar.id), convention: C.int32_t(cfg.Convention), base_index: C.uint64_t(cfg.BaseIndex.id), other_index: C.uint64_t(cfg.OtherIndex.id), discount_curve: C.uint64_t(cfg.DiscountCurve.id)}
	if cfg.EndOfMonth {
		c.end_of_month = 1
	}
	if cfg.BootstrapBaseCurve {
		c.bootstrap_base_curve = 1
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_basis_helper_new(s.ctx, &c, &id, &e), &e)
	})
	return helperResult(s, id, err)
}

type JointYieldCurves struct{ object }

type JointYieldCurvesConfig struct {
	ReferenceDate                             Date
	FirstHelpers, SecondHelpers, BasisHelpers []*RateHelper
	DayCounter                                *DayCounter
	Accuracy                                  float64
}

func (s *Session) NewJointYieldCurves(cfg JointYieldCurvesConfig) (*JointYieldCurves, error) {
	if cfg.DayCounter == nil {
		return nil, errNilArgument("day counter")
	}
	if err := sameSession(s, cfg.DayCounter.object); err != nil {
		return nil, err
	}
	var ids [3][]C.uint64_t
	for i, helpers := range [][]*RateHelper{cfg.FirstHelpers, cfg.SecondHelpers, cfg.BasisHelpers} {
		ids[i] = make([]C.uint64_t, len(helpers))
		for j, helper := range helpers {
			if helper == nil {
				return nil, errNilArgument("joint curve helper")
			}
			if err := sameSession(s, helper.object); err != nil {
				return nil, err
			}
			ids[i][j] = C.uint64_t(helper.id)
		}
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_joint_curves_new(s.ctx, C.int32_t(cfg.ReferenceDate.Serial()), (*C.uint64_t)(unsafe.Pointer(unsafe.SliceData(ids[0]))), C.size_t(len(ids[0])), (*C.uint64_t)(unsafe.Pointer(unsafe.SliceData(ids[1]))), C.size_t(len(ids[1])), (*C.uint64_t)(unsafe.Pointer(unsafe.SliceData(ids[2]))), C.size_t(len(ids[2])), C.uint64_t(cfg.DayCounter.id), C.double(cfg.Accuracy), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &JointYieldCurves{object{s, uint64(id)}}, nil
}

func (j *JointYieldCurves) Curve(member int) (*YieldTermStructure, error) {
	if j == nil {
		return nil, errNilArgument("joint curves")
	}
	if member < 0 || member > 1 {
		return nil, fmt.Errorf("joint curve member must be 0 or 1")
	}
	if err := sameSession(j.session, j.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := j.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_joint_curve(j.session.ctx, C.uint64_t(j.id), C.int32_t(member), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YieldTermStructure{object{j.session, uint64(id)}}, nil
}

func (s *Session) NewSwapRateHelperWithDiscount(cfg SwapRateHelperConfig, discount *YieldTermStructure) (*RateHelper, error) {
	if cfg.Quote == nil || cfg.Calendar == nil || cfg.FixedDayCount == nil || cfg.IborIndex == nil || discount == nil {
		return nil, errNilArgument("swap helper input")
	}
	if err := sameSession(s, cfg.Quote.object, cfg.Calendar.object, cfg.FixedDayCount.object, cfg.IborIndex.object, discount.object); err != nil {
		return nil, err
	}
	c := C.ItofinSwapHelperConfig{quote: C.uint64_t(cfg.Quote.id), tenor_length: C.int32_t(cfg.Tenor.Length), tenor_unit: C.int32_t(cfg.Tenor.Unit), calendar: C.uint64_t(cfg.Calendar.id), frequency: C.int32_t(cfg.FixedFrequency), convention: C.int32_t(cfg.FixedConvention), day_counter: C.uint64_t(cfg.FixedDayCount.id), index: C.uint64_t(cfg.IborIndex.id)}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_swap_helper_new_with_pillar(s.ctx, &c, C.uint64_t(discount.id), C.int32_t(creditOptional(cfg.Pillar, LastRelevantDate)), customPillarSerial(cfg.CustomPillarDate), &id, &e), &e)
	})
	return helperResult(s, id, err)
}

func (i *IborIndex) AddFixing(date Date, value float64) error {
	if i == nil {
		return errNilArgument("index")
	}
	if err := sameSession(i.session, i.object); err != nil {
		return err
	}
	return i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_ibor_add_fixing(i.session.ctx, C.uint64_t(i.id), C.int32_t(date.Serial()), C.double(value), &e), &e)
	})
}

func (s *Session) NewFlatForwardFromQuote(reference Date, quote *SimpleQuote, dc *DayCounter) (*YieldTermStructure, error) {
	if quote == nil || dc == nil {
		return nil, errNilArgument("flat curve input")
	}
	if err := sameSession(s, quote.object, dc.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_flat_forward_from_quote(s.ctx, C.int32_t(reference.Serial()), C.uint64_t(quote.id), C.uint64_t(dc.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &YieldTermStructure{object{s, uint64(id)}}, nil
}
