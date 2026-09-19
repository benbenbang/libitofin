package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// MakeSwaptionConfig builds a payer vanilla swaption; overnight swap indexes
// and underlying-type selection are unsupported. Exactly one of OptionTenor
// and FixingDate is required. A nil Strike selects the swap index's fair rate.
type MakeSwaptionConfig struct {
	Index                    *SwapIndex
	OptionTenor              *Period
	FixingDate, ExerciseDate *Date
	Strike, Nominal          *float64
	ExerciseCalendar         *Calendar
	OptionConvention         BusinessDayConvention
	SettlementType           SettlementType
	SettlementMethod         SettlementMethod
	IndexedCoupons           *bool
}

func (s *Session) MakeSwaption(a MakeSwaptionConfig) (*Swaption, error) {
	if a.Index == nil {
		return nil, errNilArgument("swap index")
	}
	if (a.OptionTenor == nil) == (a.FixingDate == nil) {
		return nil, fmt.Errorf("exactly one of option tenor and fixing date is required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		objects := []object{a.Index.object}
		if a.ExerciseCalendar != nil {
			objects = append(objects, a.ExerciseCalendar.object)
		}
		if err := sameSession(s, objects...); err != nil {
			return err
		}
		c := C.ItofinMakeSwaptionConfig{index: C.uint64_t(a.Index.id), option_convention: C.int32_t(a.OptionConvention), settlement_type: C.int32_t(a.SettlementType), settlement_method: C.int32_t(a.SettlementMethod)}
		if a.OptionTenor != nil {
			c.tenor_length = C.int32_t(a.OptionTenor.Length)
			c.tenor_unit = C.int32_t(a.OptionTenor.Unit)
		}
		if a.Strike != nil {
			c.flags |= 1
			c.strike = C.double(*a.Strike)
		}
		if a.Nominal != nil {
			c.flags |= 2
			c.nominal = C.double(*a.Nominal)
		}
		if a.FixingDate != nil {
			c.flags |= 4
			c.fixing_date = C.int32_t(a.FixingDate.Serial())
		}
		if a.ExerciseDate != nil {
			c.flags |= 8
			c.exercise_date = C.int32_t(a.ExerciseDate.Serial())
		}
		if a.ExerciseCalendar != nil {
			c.exercise_calendar = C.uint64_t(a.ExerciseCalendar.id)
		}
		if a.IndexedCoupons != nil {
			c.flags |= 16
			if *a.IndexedCoupons {
				c.indexed_coupons = 1
			}
		}
		var e C.ItofinError
		return ffiError(C.itofin_make_swaption(s.ctx, c, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Swaption{object{s, uint64(id)}}, nil
}

func (o *Swaption) details(field int32) (float64, error) {
	var value C.double
	err := o.session.invoke(func() error {
		if err := sameSession(o.session, o.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_swaption_details(o.session.ctx, C.uint64_t(o.id), C.int32_t(field), &value, &e), &e)
	})
	return float64(value), err
}
func (o *Swaption) ExerciseDate() (Date, error) {
	v, err := o.details(0)
	if err != nil {
		return Date{}, err
	}
	return DateFromSerial(int32(v))
}
func (o *Swaption) UnderlyingFixedRate() (float64, error) { return o.details(1) }
func (o *Swaption) UnderlyingNominal() (float64, error)   { return o.details(2) }
