package itofin

/*
#include "itofin.h"
*/
import "C"
import "fmt"

// BMAIndex retains a weekly SIFMA forecast curve and settings history.
type BMAIndex struct{ object }

func (s *Session) NewBMAIndex(forwarding *YieldTermStructure, settings *Settings) (*BMAIndex, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	objects := []object{settings.object}
	var forecast uint64
	if forwarding != nil {
		objects = append(objects, forwarding.object)
		forecast = forwarding.id
	}
	if err := sameSession(s, objects...); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_index_new(s.ctx, C.uint64_t(forecast), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &BMAIndex{object{s, uint64(id)}}, nil
}
func (i *BMAIndex) AddFixing(date Date, value float64) error {
	if i == nil {
		return errNilArgument("BMA index")
	}
	if err := sameSession(i.session, i.object); err != nil {
		return err
	}
	return i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_add_fixing(i.session.ctx, C.uint64_t(i.id), C.int32_t(date.Serial()), C.double(value), &e), &e)
	})
}
func (i *BMAIndex) Fixing(date Date, forecastToday bool) (float64, error) {
	if i == nil {
		return 0, errNilArgument("BMA index")
	}
	if err := sameSession(i.session, i.object); err != nil {
		return 0, err
	}
	var value C.double
	var forecast C.uint8_t
	if forecastToday {
		forecast = 1
	}
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_fixing(i.session.ctx, C.uint64_t(i.id), C.int32_t(date.Serial()), forecast, &value, &e), &e)
	})
	return float64(value), err
}
func (i *BMAIndex) dateQuery(date Date, query int32) (int32, error) {
	if i == nil {
		return 0, errNilArgument("BMA index")
	}
	if err := sameSession(i.session, i.object); err != nil {
		return 0, err
	}
	var value C.int32_t
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_date(i.session.ctx, C.uint64_t(i.id), C.int32_t(query), C.int32_t(date.Serial()), &value, &e), &e)
	})
	return int32(value), err
}
func (i *BMAIndex) ValueDate(date Date) (Date, error) {
	v, e := i.dateQuery(date, 0)
	return Date{v}, e
}
func (i *BMAIndex) MaturityDate(date Date) (Date, error) {
	v, e := i.dateQuery(date, 1)
	return Date{v}, e
}
func (i *BMAIndex) IsValidFixingDate(date Date) (bool, error) {
	v, e := i.dateQuery(date, 2)
	return v != 0, e
}
func bmaDates(o object, kind int32, start, end Date) ([]Date, error) {
	if err := sameSession(o.session, o); err != nil {
		return nil, err
	}
	var result []Date
	err := o.session.invoke(func() error {
		var e C.ItofinError
		var n C.size_t
		if err := ffiError(C.itofin_bma_dates(o.session.ctx, C.uint64_t(o.id), C.int32_t(kind), C.int32_t(start.Serial()), C.int32_t(end.Serial()), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		if n == 0 {
			return nil
		}
		serials := make([]C.int32_t, int(n))
		if err := ffiError(C.itofin_bma_dates(o.session.ctx, C.uint64_t(o.id), C.int32_t(kind), C.int32_t(start.Serial()), C.int32_t(end.Serial()), &serials[0], n, &n, &e), &e); err != nil {
			return err
		}
		result = make([]Date, len(serials))
		for j, v := range serials {
			result[j] = Date{int32(v)}
		}
		return nil
	})
	return result, err
}
func (i *BMAIndex) FixingSchedule(start, end Date) ([]Date, error) {
	if i == nil {
		return nil, errNilArgument("BMA index")
	}
	return bmaDates(i.object, 0, start, end)
}

// AverageBMACoupon pays the calendar-day weighted mean of municipal fixings.
type AverageBMACoupon struct{ object }
type AverageBMACouponConfig struct {
	PaymentDate, StartDate, EndDate Date
	Nominal                         float64
	Index                           *BMAIndex
	DayCounter                      *DayCounter
	Gearing, Spread                 *float64
	ReferenceStart, ReferenceEnd    *Date
}

func (s *Session) NewAverageBMACoupon(a AverageBMACouponConfig) (*AverageBMACoupon, error) {
	if a.Index == nil || a.DayCounter == nil {
		return nil, errNilArgument("BMA coupon input")
	}
	if err := sameSession(s, a.Index.object, a.DayCounter.object); err != nil {
		return nil, err
	}
	c := C.ItofinBmaCouponConfig{payment_date: C.int32_t(a.PaymentDate.Serial()), nominal: C.double(a.Nominal), start_date: C.int32_t(a.StartDate.Serial()), end_date: C.int32_t(a.EndDate.Serial()), index: C.uint64_t(a.Index.id), day_counter: C.uint64_t(a.DayCounter.id), gearing: 1}
	if a.Gearing != nil {
		c.gearing = C.double(*a.Gearing)
	}
	if a.Spread != nil {
		c.spread = C.double(*a.Spread)
	}
	if a.ReferenceStart != nil {
		c.reference_start = C.int32_t(a.ReferenceStart.Serial())
	}
	if a.ReferenceEnd != nil {
		c.reference_end = C.int32_t(a.ReferenceEnd.Serial())
	}
	var id C.uint64_t
	err := s.invoke(func() error { var e C.ItofinError; return ffiError(C.itofin_bma_coupon_new(s.ctx, &c, &id, &e), &e) })
	if err != nil {
		return nil, err
	}
	return &AverageBMACoupon{object{s, uint64(id)}}, nil
}
func (c *AverageBMACoupon) value(query int32) (float64, error) {
	if c == nil {
		return 0, errNilArgument("BMA coupon")
	}
	if err := sameSession(c.session, c.object); err != nil {
		return 0, err
	}
	var value C.double
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_coupon_value(c.session.ctx, C.uint64_t(c.id), C.int32_t(query), &value, &e), &e)
	})
	return float64(value), err
}
func (c *AverageBMACoupon) Rate() (float64, error)          { return c.value(0) }
func (c *AverageBMACoupon) Amount() (float64, error)        { return c.value(1) }
func (c *AverageBMACoupon) AccrualPeriod() (float64, error) { return c.value(2) }
func (c *AverageBMACoupon) FixingDates() ([]Date, error) {
	if c == nil {
		return nil, errNilArgument("BMA coupon")
	}
	return bmaDates(c.object, 1, Date{}, Date{})
}

// BMASwap pays BMA and receives Ibor when Type is SwapPayer.
type BMASwap struct{ object }
type BMASwapConfig struct {
	Type                                SwapType
	Nominal, LiborFraction, LiborSpread float64
	LiborSchedule, BMASchedule          *Schedule
	LiborIndex                          *IborIndex
	BMAIndex                            *BMAIndex
	LiborDayCounter, BMADayCounter      *DayCounter
	Settings                            *Settings
}

func (s *Session) NewBMASwap(a BMASwapConfig) (*BMASwap, error) {
	if a.LiborSchedule == nil || a.BMASchedule == nil || a.LiborIndex == nil || a.BMAIndex == nil || a.LiborDayCounter == nil || a.BMADayCounter == nil || a.Settings == nil {
		return nil, errNilArgument("BMA swap input")
	}
	if err := sameSession(s, a.LiborSchedule.object, a.BMASchedule.object, a.LiborIndex.object, a.BMAIndex.object, a.LiborDayCounter.object, a.BMADayCounter.object, a.Settings.object); err != nil {
		return nil, err
	}
	c := C.ItofinBmaSwapConfig{swap_type: C.int32_t(a.Type), nominal: C.double(a.Nominal), libor_fraction: C.double(a.LiborFraction), libor_spread: C.double(a.LiborSpread), libor_schedule: C.uint64_t(a.LiborSchedule.id), libor_index: C.uint64_t(a.LiborIndex.id), libor_day_counter: C.uint64_t(a.LiborDayCounter.id), bma_schedule: C.uint64_t(a.BMASchedule.id), bma_index: C.uint64_t(a.BMAIndex.id), bma_day_counter: C.uint64_t(a.BMADayCounter.id), settings: C.uint64_t(a.Settings.id)}
	var id C.uint64_t
	err := s.invoke(func() error { var e C.ItofinError; return ffiError(C.itofin_bma_swap_new(s.ctx, &c, &id, &e), &e) })
	if err != nil {
		return nil, err
	}
	return &BMASwap{object{s, uint64(id)}}, nil
}
func (b *BMASwap) SetEngine(curve *YieldTermStructure, settings *Settings) error {
	if b == nil || curve == nil || settings == nil {
		return errNilArgument("BMA engine input")
	}
	if err := sameSession(b.session, b.object, curve.object, settings.object); err != nil {
		return err
	}
	return b.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_swap_set_engine(b.session.ctx, C.uint64_t(b.id), C.uint64_t(curve.id), C.uint64_t(settings.id), &e), &e)
	})
}
func (b *BMASwap) value(query int32) (float64, error) {
	if b == nil {
		return 0, errNilArgument("BMA swap")
	}
	if err := sameSession(b.session, b.object); err != nil {
		return 0, err
	}
	var value C.double
	err := b.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_swap_value(b.session.ctx, C.uint64_t(b.id), C.int32_t(query), &value, &e), &e)
	})
	return float64(value), err
}
func (b *BMASwap) NPV() (float64, error)               { return b.value(0) }
func (b *BMASwap) FairLiborFraction() (float64, error) { return b.value(1) }
func (b *BMASwap) FairLiborSpread() (float64, error)   { return b.value(2) }
func (b *BMASwap) IsCalculated() (bool, error)         { v, e := b.value(3); return v != 0, e }
func (b *BMASwap) LegNPV(leg int) (float64, error) {
	if leg < 0 || leg > 1 {
		return 0, fmt.Errorf("BMA leg must be zero or one")
	}
	return b.value(int32(4 + leg))
}
func (b *BMASwap) LegBPS(leg int) (float64, error) {
	if leg < 0 || leg > 1 {
		return 0, fmt.Errorf("BMA leg must be zero or one")
	}
	return b.value(int32(6 + leg))
}

// BMASwapRateHelper uses the existing rate-helper and piecewise-curve interfaces.
type BMASwapRateHelper = RateHelper
type BMASwapRateHelperConfig struct {
	Quote          *SimpleQuote
	Tenor          Period
	SettlementDays uint32
	Calendar       *Calendar
	BMAPeriod      Period
	BMAConvention  BusinessDayConvention
	BMADayCounter  *DayCounter
	BMAIndex       *BMAIndex
	LiborIndex     *IborIndex
}

func (s *Session) NewBMASwapRateHelper(a BMASwapRateHelperConfig) (*RateHelper, error) {
	if a.Quote == nil || a.Calendar == nil || a.BMADayCounter == nil || a.BMAIndex == nil || a.LiborIndex == nil {
		return nil, errNilArgument("BMA helper input")
	}
	if err := sameSession(s, a.Quote.object, a.Calendar.object, a.BMADayCounter.object, a.BMAIndex.object, a.LiborIndex.object); err != nil {
		return nil, err
	}
	c := C.ItofinBmaHelperConfig{quote: C.uint64_t(a.Quote.id), tenor_length: C.int32_t(a.Tenor.Length), tenor_unit: C.int32_t(a.Tenor.Unit), settlement_days: C.uint32_t(a.SettlementDays), calendar: C.uint64_t(a.Calendar.id), bma_length: C.int32_t(a.BMAPeriod.Length), bma_unit: C.int32_t(a.BMAPeriod.Unit), bma_convention: C.int32_t(a.BMAConvention), bma_day_counter: C.uint64_t(a.BMADayCounter.id), bma_index: C.uint64_t(a.BMAIndex.id), libor_index: C.uint64_t(a.LiborIndex.id)}
	var id C.uint64_t
	err := s.invoke(func() error { var e C.ItofinError; return ffiError(C.itofin_bma_helper_new(s.ctx, &c, &id, &e), &e) })
	return helperResult(s, id, err)
}

func (i *BMAIndex) FixingCalendar() (*Calendar, error) {
	if i == nil {
		return nil, errNilArgument("BMA index")
	}
	if err := sameSession(i.session, i.object); err != nil {
		return nil, err
	}
	var id C.uint64_t
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_calendar(i.session.ctx, C.uint64_t(i.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Calendar{object{i.session, uint64(id)}}, nil
}
func (i *BMAIndex) ClearFixings() error {
	if i == nil {
		return errNilArgument("BMA index")
	}
	if err := sameSession(i.session, i.object); err != nil {
		return err
	}
	return i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_clear_fixings(i.session.ctx, C.uint64_t(i.id), &e), &e)
	})
}
func (i *BMAIndex) HasHistoricalFixing(date Date) (bool, error) {
	v, e := i.dateQuery(date, 3)
	return v != 0, e
}
func (i *BMAIndex) PastFixing(date Date) (float64, bool, error) {
	if i == nil {
		return 0, false, errNilArgument("BMA index")
	}
	if err := sameSession(i.session, i.object); err != nil {
		return 0, false, err
	}
	var value C.double
	var found C.uint8_t
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_bma_past_fixing(i.session.ctx, C.uint64_t(i.id), C.int32_t(date.Serial()), &value, &found, &e), &e)
	})
	return float64(value), found != 0, err
}
