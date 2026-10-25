package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

type CPILeg struct{ object }
type CPICoupon struct{ object }
type CPICouponPricer struct{ object }

type CPILegConfig struct {
	Schedule              *Schedule
	Index                 *ZeroInflationIndex
	BaseCPI               float64
	ObservationLag        Period
	Interpolation         CpiInterpolationType
	Notionals, FixedRates []float64
	DayCounter            *DayCounter
	PaymentCalendar       *Calendar
	PaymentAdjustment     BusinessDayConvention
	GrowthOnly            bool
	ExCouponPeriod        Period
	ExCouponCalendar      *Calendar
	ExCouponAdjustment    BusinessDayConvention
	ExCouponEndOfMonth    bool
}

func (s *Session) NewCPILeg(a CPILegConfig) (*CPILeg, error) {
	if a.Schedule == nil || a.Index == nil || a.DayCounter == nil || a.PaymentCalendar == nil {
		return nil, fmt.Errorf("schedule, index, day counter and calendar required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Schedule.object, a.Index.object, a.DayCounter.object, a.PaymentCalendar.object); err != nil {
			return err
		}
		var ex uint64
		if a.ExCouponPeriod.Length != 0 {
			if a.ExCouponCalendar == nil {
				return fmt.Errorf("ex-coupon calendar required")
			}
			if err := sameSession(s, a.ExCouponCalendar.object); err != nil {
				return err
			}
			ex = a.ExCouponCalendar.id
		}
		cfg := C.ItofinCpiLegConfig{schedule: C.uint64_t(a.Schedule.id), index: C.uint64_t(a.Index.id), base_cpi: C.double(a.BaseCPI), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), day_counter: C.uint64_t(a.DayCounter.id), payment_calendar: C.uint64_t(a.PaymentCalendar.id), payment_convention: C.int32_t(a.PaymentAdjustment), notionals: inflationDoubles(a.Notionals), notionals_len: C.size_t(len(a.Notionals)), fixed_rates: inflationDoubles(a.FixedRates), fixed_rates_len: C.size_t(len(a.FixedRates)), growth_only: creditBool(a.GrowthOnly), ex_coupon_length: C.int32_t(a.ExCouponPeriod.Length), ex_coupon_unit: C.int32_t(a.ExCouponPeriod.Unit), ex_coupon_calendar: C.uint64_t(ex), ex_coupon_convention: C.int32_t(a.ExCouponAdjustment), ex_coupon_end_of_month: creditBool(a.ExCouponEndOfMonth)}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_leg_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CPILeg{object{s, uint64(id)}}, nil
}
func (l *CPILeg) Build() (*Leg, error) {
	if l == nil {
		return nil, errNilArgument("CPI leg")
	}
	var id C.uint64_t
	err := l.session.invoke(func() error {
		if err := sameSession(l.session, l.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_leg_build(l.session.ctx, C.uint64_t(l.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Leg{object{l.session, uint64(id)}}, nil
}

type CPICouponConfig struct {
	Index                                      *ZeroInflationIndex
	BaseCPI, Nominal, FixedRate                float64
	PaymentDate, StartDate, EndDate            Date
	ReferenceStart, ReferenceEnd, ExCouponDate *Date
	ObservationLag                             Period
	Interpolation                              CpiInterpolationType
	DayCounter                                 *DayCounter
}

func (s *Session) NewCPICoupon(a CPICouponConfig) (*CPICoupon, error) {
	for _, date := range []*Date{a.ReferenceStart, a.ReferenceEnd, a.ExCouponDate} {
		if date != nil && date.Serial() == 0 {
			return nil, fmt.Errorf("invalid explicit CPI coupon date")
		}
	}
	if a.Index == nil || a.DayCounter == nil {
		return nil, fmt.Errorf("index and day counter required")
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		if err := sameSession(s, a.Index.object, a.DayCounter.object); err != nil {
			return err
		}
		optional := func(d *Date) C.int32_t {
			if d == nil {
				return 0
			}
			return C.int32_t(d.Serial())
		}
		cfg := C.ItofinCpiCouponConfig{index: C.uint64_t(a.Index.id), base_cpi: C.double(a.BaseCPI), nominal: C.double(a.Nominal), fixed_rate: C.double(a.FixedRate), payment: C.int32_t(a.PaymentDate.Serial()), start: C.int32_t(a.StartDate.Serial()), end: C.int32_t(a.EndDate.Serial()), reference_start: optional(a.ReferenceStart), reference_end: optional(a.ReferenceEnd), ex_coupon: optional(a.ExCouponDate), lag_length: C.int32_t(a.ObservationLag.Length), lag_unit: C.int32_t(a.ObservationLag.Unit), interpolation: C.int32_t(a.Interpolation), day_counter: C.uint64_t(a.DayCounter.id)}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_coupon_new(s.ctx, cfg, &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CPICoupon{object{s, uint64(id)}}, nil
}
func (s *Session) NewCPICouponPricer(discount *YieldTermStructure) (*CPICouponPricer, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var curve uint64
		if discount != nil {
			if err := sameSession(s, discount.object); err != nil {
				return err
			}
			curve = discount.id
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_coupon_pricer_new(s.ctx, C.uint64_t(curve), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &CPICouponPricer{object{s, uint64(id)}}, nil
}
func (c *CPICoupon) SetPricer(p *CPICouponPricer) error {
	if c == nil {
		return errNilArgument("CPI coupon")
	}
	if p == nil {
		return fmt.Errorf("pricer required")
	}
	return c.session.invoke(func() error {
		if err := sameSession(c.session, c.object, p.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_coupon_set_pricer(c.session.ctx, C.uint64_t(c.id), C.uint64_t(p.id), &e), &e)
	})
}
func (c *CPICoupon) value(field int32, date Date) (float64, error) {
	if c == nil {
		return 0, errNilArgument("CPI coupon")
	}
	var out C.double
	err := c.session.invoke(func() error {
		if err := sameSession(c.session, c.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_coupon_value(c.session.ctx, C.uint64_t(c.id), C.int32_t(field), C.int32_t(date.Serial()), &out, &e), &e)
	})
	return float64(out), err
}
func (c *CPICoupon) Rate() (float64, error)                   { return c.value(0, Date{}) }
func (c *CPICoupon) Amount() (float64, error)                 { return c.value(1, Date{}) }
func (c *CPICoupon) IndexFixing() (float64, error)            { return c.value(2, Date{}) }
func (c *CPICoupon) IndexRatio(date Date) (float64, error)    { return c.value(3, date) }
func (c *CPICoupon) AccruedAmount(date Date) (float64, error) { return c.value(4, date) }
func (c *CPICoupon) AccrualPeriod() (float64, error)          { return c.value(5, Date{}) }
func (p *CPICouponPricer) value(c *CPICoupon, field int32, date Date) (float64, error) {
	if p == nil {
		return 0, errNilArgument("CPI pricer")
	}
	if c == nil {
		return 0, fmt.Errorf("coupon required")
	}
	var out C.double
	err := p.session.invoke(func() error {
		if err := sameSession(p.session, p.object, c.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_cpi_pricer_value(p.session.ctx, C.uint64_t(p.id), C.uint64_t(c.id), C.int32_t(field), C.int32_t(date.Serial()), &out, &e), &e)
	})
	return float64(out), err
}
func (p *CPICouponPricer) SwapletRate(c *CPICoupon) (float64, error)  { return p.value(c, 0, Date{}) }
func (p *CPICouponPricer) SwapletPrice(c *CPICoupon) (float64, error) { return p.value(c, 1, Date{}) }
func (p *CPICouponPricer) AccruedRate(c *CPICoupon, date Date) (float64, error) {
	return p.value(c, 2, date)
}
