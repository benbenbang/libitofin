package itofin

/*
#include "itofin.h"
*/
import "C"
import "unsafe"

type Currency struct{ object }
type IborIndex struct{ object }
type OvernightIndex struct{ object }
type Euribor = IborIndex
type UsdLibor = IborIndex
type JpyLibor = IborIndex
type GbpLibor = IborIndex
type EurLibor = IborIndex
type CustomIborIndex = IborIndex
type Estr = OvernightIndex
type Eonia = OvernightIndex

func (s *Session) currency(kind int) (*Currency, error) {
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_currency_new(s.ctx, C.int32_t(kind), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &Currency{object{s, uint64(id)}}, nil
}
func (s *Session) EUR() (*Currency, error) { return s.currency(0) }
func (s *Session) USD() (*Currency, error) { return s.currency(1) }
func (s *Session) GBP() (*Currency, error) { return s.currency(2) }
func (s *Session) JPY() (*Currency, error) { return s.currency(3) }
func (c *Currency) Code() (string, error) {
	var buf [4]C.uint8_t
	err := c.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_currency_code(c.session.ctx, C.uint64_t(c.id), &buf[0], 4, &e), &e)
	})
	return string([]byte{byte(buf[0]), byte(buf[1]), byte(buf[2])}), err
}
func (c *Currency) Repr() (string, error) {
	v, e := c.Code()
	if e != nil {
		return "", e
	}
	return "Currency(" + v + ")", nil
}
func (s *Session) iborFamily(family int, tenor Period, curve *YieldTermStructure, settings *Settings) (*IborIndex, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	if err := sameSession(s, settings.object); err != nil {
		return nil, err
	}
	var f uint64
	if curve != nil {
		if err := sameSession(s, curve.object); err != nil {
			return nil, err
		}
		f = curve.id
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_ibor_family_new(s.ctx, C.int32_t(family), C.int32_t(tenor.Length), C.int32_t(tenor.Unit), C.uint64_t(f), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &IborIndex{object{s, uint64(id)}}, nil
}
func (s *Session) NewEuribor(t Period, c *YieldTermStructure, v *Settings) (*IborIndex, error) {
	return s.iborFamily(0, t, c, v)
}
func (s *Session) NewUsdLibor(t Period, c *YieldTermStructure, v *Settings) (*IborIndex, error) {
	return s.iborFamily(1, t, c, v)
}
func (s *Session) NewJpyLibor(t Period, c *YieldTermStructure, v *Settings) (*IborIndex, error) {
	return s.iborFamily(2, t, c, v)
}
func (s *Session) NewGbpLibor(t Period, c *YieldTermStructure, v *Settings) (*IborIndex, error) {
	return s.iborFamily(3, t, c, v)
}
func (s *Session) NewEurLibor(t Period, c *YieldTermStructure, v *Settings) (*IborIndex, error) {
	return s.iborFamily(4, t, c, v)
}
func (s *Session) NewEuriborThreeMonths(c *YieldTermStructure, v *Settings) (*IborIndex, error) {
	if c == nil {
		return nil, errNilArgument("curve")
	}
	return s.NewEuribor(Period{Length: 3, Unit: TimeUnit(2)}, c, v)
}
func (s *Session) NewEuriborSixMonths(c *YieldTermStructure, v *Settings) (*IborIndex, error) {
	if c == nil {
		return nil, errNilArgument("curve")
	}
	return s.NewEuribor(Period{Length: 6, Unit: TimeUnit(2)}, c, v)
}

type IborIndexConfig struct {
	FamilyName       string
	Tenor            Period
	SettlementDays   uint32
	Currency         *Currency
	FixingCalendar   *Calendar
	ValueCalendar    *Calendar // Required only for CustomIborIndex.
	MaturityCalendar *Calendar // Required only for CustomIborIndex.
	Convention       BusinessDayConvention
	EndOfMonth       bool
	DayCounter       *DayCounter
	Forwarding       *YieldTermStructure
	Settings         *Settings
}

func (s *Session) newIbor(cfg IborIndexConfig, custom bool) (*IborIndex, error) {
	if cfg.Currency == nil || cfg.FixingCalendar == nil || cfg.DayCounter == nil || cfg.Settings == nil {
		return nil, errNilArgument("index convention")
	}
	if err := sameSession(s, cfg.Currency.object, cfg.FixingCalendar.object, cfg.DayCounter.object, cfg.Settings.object); err != nil {
		return nil, err
	}
	a := C.ItofinIborConfig{tenor_length: C.int32_t(cfg.Tenor.Length), tenor_unit: C.int32_t(cfg.Tenor.Unit), settlement_days: C.uint32_t(cfg.SettlementDays), currency: C.uint64_t(cfg.Currency.id), fixing_calendar: C.uint64_t(cfg.FixingCalendar.id), convention: C.int32_t(cfg.Convention), end_of_month: C.bool(cfg.EndOfMonth), day_counter: C.uint64_t(cfg.DayCounter.id), settings: C.uint64_t(cfg.Settings.id)}
	if cfg.Forwarding != nil {
		if err := sameSession(s, cfg.Forwarding.object); err != nil {
			return nil, err
		}
		a.forwarding = C.uint64_t(cfg.Forwarding.id)
	}
	if custom {
		if cfg.ValueCalendar == nil || cfg.MaturityCalendar == nil {
			return nil, errNilArgument("custom index calendar")
		}
		if err := sameSession(s, cfg.ValueCalendar.object, cfg.MaturityCalendar.object); err != nil {
			return nil, err
		}
		a.value_calendar = C.uint64_t(cfg.ValueCalendar.id)
		a.maturity_calendar = C.uint64_t(cfg.MaturityCalendar.id)
	}
	name := []byte(cfg.FamilyName)
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_ibor_new(s.ctx, (*C.uint8_t)(unsafe.Pointer(unsafe.SliceData(name))), C.size_t(len(name)), &a, C.bool(custom), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &IborIndex{object{s, uint64(id)}}, nil
}
func (s *Session) NewIborIndex(c IborIndexConfig) (*IborIndex, error) { return s.newIbor(c, false) }
func (s *Session) NewCustomIborIndex(c IborIndexConfig) (*IborIndex, error) {
	return s.newIbor(c, true)
}
func (s *Session) NewEstr(curve *YieldTermStructure, settings *Settings) (*OvernightIndex, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	if err := sameSession(s, settings.object); err != nil {
		return nil, err
	}
	var f uint64
	if curve != nil {
		if err := sameSession(s, curve.object); err != nil {
			return nil, err
		}
		f = curve.id
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_estr_new(s.ctx, C.uint64_t(f), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OvernightIndex{object{s, uint64(id)}}, nil
}
func (s *Session) NewEonia(curve *YieldTermStructure, settings *Settings) (*OvernightIndex, error) {
	if settings == nil {
		return nil, errNilArgument("settings")
	}
	if err := sameSession(s, settings.object); err != nil {
		return nil, err
	}
	var f uint64
	if curve != nil {
		if err := sameSession(s, curve.object); err != nil {
			return nil, err
		}
		f = curve.id
	}
	var id C.uint64_t
	err := s.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_eonia_new(s.ctx, C.uint64_t(f), C.uint64_t(settings.id), &id, &e), &e)
	})
	if err != nil {
		return nil, err
	}
	return &OvernightIndex{object{s, uint64(id)}}, nil
}
func indexFixing(o object, overnight bool, d Date, forecast bool) (float64, error) {
	var v C.double
	err := o.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_index_fixing(o.session.ctx, C.uint64_t(o.id), C.bool(overnight), C.int32_t(d.Serial()), C.bool(forecast), &v, &e), &e)
	})
	return float64(v), err
}
func (i *IborIndex) Fixing(d Date, forecastTodaysFixing bool) (float64, error) {
	return indexFixing(i.object, false, d, forecastTodaysFixing)
}
func (i *OvernightIndex) Fixing(d Date, forecastTodaysFixing bool) (float64, error) {
	return indexFixing(i.object, true, d, forecastTodaysFixing)
}
func (i *IborIndex) date(query int, d Date) (Date, error) {
	var v C.int32_t
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_ibor_date(i.session.ctx, C.uint64_t(i.id), C.int32_t(query), C.int32_t(d.Serial()), &v, &e), &e)
	})
	return Date{serial: int32(v)}, err
}
func (i *IborIndex) ValueDate(d Date) (Date, error)    { return i.date(0, d) }
func (i *IborIndex) FixingDate(d Date) (Date, error)   { return i.date(1, d) }
func (i *IborIndex) MaturityDate(d Date) (Date, error) { return i.date(2, d) }
func (i *IborIndex) component(query int) (object, error) {
	var id C.uint64_t
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_ibor_component(i.session.ctx, C.uint64_t(i.id), C.int32_t(query), &id, &e), &e)
	})
	return object{i.session, uint64(id)}, err
}
func (i *IborIndex) DayCounter() (*DayCounter, error) {
	o, e := i.component(0)
	if e != nil {
		return nil, e
	}
	return &DayCounter{o}, nil
}
func (i *IborIndex) FixingCalendar() (*Calendar, error) {
	o, e := i.component(1)
	if e != nil {
		return nil, e
	}
	return &Calendar{o}, nil
}
func (i *IborIndex) Currency() (*Currency, error) {
	o, e := i.component(2)
	if e != nil {
		return nil, e
	}
	return &Currency{o}, nil
}
func (i *IborIndex) info() (C.ItofinIborInfo, error) {
	var v C.ItofinIborInfo
	err := i.session.invoke(func() error {
		var e C.ItofinError
		return ffiError(C.itofin_ibor_info(i.session.ctx, C.uint64_t(i.id), &v, &e), &e)
	})
	return v, err
}
func (i *IborIndex) Tenor() (Period, error) {
	v, e := i.info()
	return Period{Length: int32(v.tenor_length), Unit: TimeUnit(v.tenor_unit)}, e
}
func (i *IborIndex) BusinessDayConvention() (BusinessDayConvention, error) {
	v, e := i.info()
	return BusinessDayConvention(v.convention), e
}
func (i *IborIndex) EndOfMonth() (bool, error) { v, e := i.info(); return bool(v.end_of_month), e }
func (i *IborIndex) Name() (string, error) {
	var buf []C.uint8_t
	err := i.session.invoke(func() error {
		var e C.ItofinError
		var n C.size_t
		if err := ffiError(C.itofin_ibor_name(i.session.ctx, C.uint64_t(i.id), nil, 0, &n, &e), &e); err != nil {
			return err
		}
		buf = make([]C.uint8_t, int(n)+1)
		return ffiError(C.itofin_ibor_name(i.session.ctx, C.uint64_t(i.id), unsafe.SliceData(buf), C.size_t(len(buf)), &n, &e), &e)
	})
	if err != nil {
		return "", err
	}
	value := make([]byte, len(buf)-1)
	for j := range value {
		value[j] = byte(buf[j])
	}
	return string(value), nil
}

func (i *OvernightIndex) component(query int32) (object, error) {
	var id C.uint64_t
	err := i.session.invoke(func() error {
		if err := sameSession(i.session, i.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_overnight_component(i.session.ctx, C.uint64_t(i.id), C.int32_t(query), &id, &e), &e)
	})
	return object{i.session, uint64(id)}, err
}
func (i *OvernightIndex) DayCounter() (*DayCounter, error) {
	o, err := i.component(0)
	if err != nil {
		return nil, err
	}
	return &DayCounter{o}, nil
}
func (i *OvernightIndex) FixingCalendar() (*Calendar, error) {
	o, err := i.component(1)
	if err != nil {
		return nil, err
	}
	return &Calendar{o}, nil
}
func (i *OvernightIndex) Currency() (*Currency, error) {
	o, err := i.component(2)
	if err != nil {
		return nil, err
	}
	return &Currency{o}, nil
}
func (i *OvernightIndex) FixingDays() (uint32, error) {
	var days C.uint32_t
	err := i.session.invoke(func() error {
		if err := sameSession(i.session, i.object); err != nil {
			return err
		}
		var e C.ItofinError
		return ffiError(C.itofin_overnight_fixing_days(i.session.ctx, C.uint64_t(i.id), &days, &e), &e)
	})
	return uint32(days), err
}
