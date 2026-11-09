package itofin

/*
#include "itofin.h"
*/
import "C"
import "unsafe"

type yieldJumps struct {
	quotes []C.uint64_t
	dates  []C.int32_t
}

func (s *Session) yieldJumps(quotes []*SimpleQuote, dates []Date) (*yieldJumps, error) {
	if len(quotes) == 0 && len(dates) == 0 {
		return nil, nil
	}
	j := &yieldJumps{make([]C.uint64_t, len(quotes)), make([]C.int32_t, len(dates))}
	for i, q := range quotes {
		if q == nil {
			return nil, errNilArgument("jump quote")
		}
		if err := sameSession(s, q.object); err != nil {
			return nil, err
		}
		j.quotes[i] = C.uint64_t(q.id)
	}
	for i, d := range dates {
		j.dates[i] = C.int32_t(d.Serial())
	}
	return j, nil
}

func yieldConfig(cfg PiecewiseCurveConfig) C.ItofinPiecewiseYieldConfig {
	c := C.ItofinPiecewiseYieldConfig{reference_date: C.int32_t(cfg.ReferenceDate.Serial()), settlement_days: C.uint32_t(cfg.SettlementDays), day_counter: C.uint64_t(cfg.DayCounter.id)}
	if cfg.Settings != nil {
		c.calendar, c.settings = C.uint64_t(cfg.Calendar.id), C.uint64_t(cfg.Settings.id)
	}
	return c
}

func (s *Session) jumpedCurve(cfg PiecewiseCurveConfig, kind, algo int, ids, extra []C.uint64_t, jumps *yieldJumps, out *C.uint64_t) error {
	config := yieldConfig(cfg)
	var options *C.ItofinIterativeBootstrapOptions
	if cfg.IterativeOptions != nil {
		opts := iterativeOptions(cfg.IterativeOptions)
		options = &opts
	}
	var e C.ItofinError
	return ffiError(C.itofin_piecewise_curve_with_jumps_new(s.ctx, &config, unsafe.SliceData(ids), C.size_t(len(ids)), C.int32_t(kind), C.int32_t(algo), unsafe.SliceData(extra), C.size_t(len(extra)), options, unsafe.SliceData(jumps.quotes), C.size_t(len(jumps.quotes)), unsafe.SliceData(jumps.dates), C.size_t(len(jumps.dates)), out, &e), &e)
}

func (c *YieldTermStructure) jumpData(includeTimes bool) ([]Date, []float64, error) {
	if c == nil {
		return nil, nil, errNilArgument("yield curve")
	}
	var dates []Date
	var times []float64
	err := c.session.invoke(func() error {
		if e := sameSession(c.session, c.object); e != nil {
			return e
		}
		var n C.size_t
		var e C.ItofinError
		if err := ffiError(C.itofin_curve_jumps(c.session.ctx, C.uint64_t(c.id), nil, nil, 0, &n, &e), &e); err != nil {
			return err
		}
		dates, times = make([]Date, int(n)), make([]float64, int(n))
		if n == 0 {
			return nil
		}
		ds := make([]C.int32_t, int(n))
		var tp *C.double
		if includeTimes {
			tp = (*C.double)(unsafe.Pointer(&times[0]))
		}
		if err := ffiError(C.itofin_curve_jumps(c.session.ctx, C.uint64_t(c.id), &ds[0], tp, n, &n, &e), &e); err != nil {
			return err
		}
		for i, d := range ds {
			parsed, err := DateFromSerial(int32(d))
			if err != nil {
				return err
			}
			dates[i] = parsed
		}
		return nil
	})
	return dates, times, err
}

// JumpDates returns the discount jump dates in constructor order, including generated year-ends.
func (c *YieldTermStructure) JumpDates() ([]Date, error) {
	dates, _, err := c.jumpData(false)
	return dates, err
}

// JumpTimes returns the jump times against the curve's current reference date.
func (c *YieldTermStructure) JumpTimes() ([]float64, error) {
	_, times, err := c.jumpData(true)
	return times, err
}
