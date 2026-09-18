package itofin

import "testing"

func TestRateVolatilityMovingQuoteAndDate(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	dc, _ := s.Actual365Fixed()
	cal, _ := s.NullCalendar()
	settings, _ := s.NewSettings()
	quote, _ := s.NewSimpleQuote(.2)
	ref := volDate(t, 1, 1, 2024)
	if e = settings.SetEvaluationDate(ref); e != nil {
		t.Fatal(e)
	}
	cfg := ConstantRateVolConfig{ReferenceDate: ref, Calendar: cal, DayCounter: dc, Settings: settings, Quote: quote, Shift: .01}
	sw, e := s.ConstantSwaptionVolatility(cfg)
	if e != nil {
		t.Fatal(e)
	}
	op, e := s.ConstantOptionletVolatility(cfg)
	if e != nil {
		t.Fatal(e)
	}
	one, five := Period{Length: 1, Unit: Years}, Period{Length: 5, Unit: Years}
	x, e := sw.Volatility(one, five, .03, false)
	volNear(t, x, .2, e)
	x, e = op.Volatility(one, .03, false)
	volNear(t, x, .2, e)
	x, e = op.Displacement()
	volNear(t, x, .01, e)
	end, _ := ref.AddDays(365)
	x, e = sw.Shift(end, 5, false)
	volNear(t, x, .01, e)
	x, e = op.VolatilityDate(end, .03, false)
	volNear(t, x, .2, e)
	x, e = sw.BlackVariance(one, five, .03, false)
	volNear(t, x, .04*366/365, e)
	x, e = op.BlackVariance(one, .03, false)
	volNear(t, x, .04*366/365, e)
	if e = quote.SetValue(.3); e != nil {
		t.Fatal(e)
	}
	x, e = sw.Volatility(one, five, .03, false)
	volNear(t, x, .3, e)
	x, e = op.Volatility(one, .03, false)
	volNear(t, x, .3, e)
	if e = settings.SetEvaluationDate(end); e != nil {
		t.Fatal(e)
	}
	for _, fn := range []func() (Date, error){sw.ReferenceDate, op.ReferenceDate} {
		d, e := fn()
		if e != nil || d != end {
			t.Fatal(d, e)
		}
	}
	if e = op.EnableExtrapolation(); e != nil {
		t.Fatal(e)
	}
	ok, e := op.AllowsExtrapolation()
	if e != nil || !ok {
		t.Fatal(ok, e)
	}
	if e = op.DisableExtrapolation(); e != nil {
		t.Fatal(e)
	}
	for _, o := range []object{quote.object, dc.object, cal.object} {
		if e = o.Close(); e != nil {
			t.Fatal(e)
		}
	}
	x, e = sw.Volatility(one, five, .03, false)
	volNear(t, x, .3, e)
}
func TestRateVolatilityGridPinnedMemoryAndInterpolation(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	dc, _ := s.Actual365Fixed()
	cal, _ := s.NullCalendar()
	ref := volDate(t, 1, 1, 2024)
	cfg := RateVolGridConfig{ReferenceDate: ref, Calendar: cal, DayCounter: dc, OptionTenors: []Period{{1, Years}, {2, Years}, {3, Years}}, SwapTenors: []Period{{2, Years}, {5, Years}, {10, Years}}, Strikes: []float64{.01, .02, .03}, Volatilities: [][]float64{{.2, .2, .2}, {.2, .2, .2}, {.2, .2, .2}}}
	sw, e := s.SwaptionVolatilityMatrix(cfg)
	if e != nil {
		t.Fatal(e)
	}
	x, e := sw.Volatility(Period{18, Months}, Period{4, Years}, .02, false)
	volNear(t, x, .2, e)
	cap, e := s.CapFloorTermVolSurface(cfg)
	if e != nil {
		t.Fatal(e)
	}
	x, e = cap.Volatility(Period{18, Months}, .015, false)
	volNear(t, x, .2, e)
	x, e = cap.VolatilityTime(1.5, .015, false)
	volNear(t, x, .2, e)
	d, _ := ref.AddDays(548)
	x, e = cap.VolatilityDate(d, .015, false)
	volNear(t, x, .2, e)
	settings, _ := s.NewSettings()
	if e = settings.SetEvaluationDate(ref); e != nil {
		t.Fatal(e)
	}
	cfg.Settings = settings
	q, _ := s.NewSimpleQuote(.25)
	cfg.Quotes = [][]*SimpleQuote{{q, q, q}, {q, q, q}, {q, q, q}}
	moving, e := s.SwaptionVolatilityMatrix(cfg)
	if e != nil {
		t.Fatal(e)
	}
	x, e = moving.Volatility(Period{18, Months}, Period{4, Years}, .02, false)
	volNear(t, x, .25, e)
	cap, e = s.CapFloorTermVolSurface(cfg)
	if e != nil {
		t.Fatal(e)
	}
	x, e = cap.VolatilityTime(1.5, .015, false)
	volNear(t, x, .25, e)
	if e = q.SetValue(.3); e != nil {
		t.Fatal(e)
	}
	x, e = moving.Volatility(Period{18, Months}, Period{4, Years}, .02, false)
	volNear(t, x, .3, e)
}
func TestRateVolatilityConstructorModes(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	dc, _ := s.Actual365Fixed()
	cal, _ := s.NullCalendar()
	ref := volDate(t, 1, 1, 2024)
	later, _ := ref.AddDays(10)
	settings, _ := s.NewSettings()
	if e = settings.SetEvaluationDate(later); e != nil {
		t.Fatal(e)
	}
	q, _ := s.NewSimpleQuote(.23)
	for _, moving := range []bool{false, true} {
		for _, live := range []bool{false, true} {
			cfg := ConstantRateVolConfig{ReferenceDate: ref, Calendar: cal, DayCounter: dc, Volatility: .19}
			want, reference := .19, ref
			if live {
				cfg.Quote = q
				want = .23
			}
			if moving {
				cfg.Settings = settings
				reference = later
			}
			a, e := s.ConstantSwaptionVolatility(cfg)
			if e != nil {
				t.Fatal(e)
			}
			b, e := s.ConstantOptionletVolatility(cfg)
			if e != nil {
				t.Fatal(e)
			}
			x, e := a.Volatility(Period{1, Years}, Period{5, Years}, .02, false)
			volNear(t, x, want, e)
			x, e = b.Volatility(Period{1, Years}, .02, false)
			volNear(t, x, want, e)
			for _, fn := range []func() (Date, error){a.ReferenceDate, b.ReferenceDate} {
				d, e := fn()
				if e != nil || d != reference {
					t.Fatal(d, e)
				}
			}
			grid := RateVolGridConfig{ReferenceDate: ref, Calendar: cal, DayCounter: dc, OptionTenors: []Period{{1, Years}, {2, Years}, {3, Years}}, Strikes: []float64{.01, .02, .03}, Volatilities: [][]float64{{.19, .19, .19}, {.19, .19, .19}, {.19, .19, .19}}}
			if moving {
				grid.Settings = settings
			}
			if live {
				grid.Quotes = [][]*SimpleQuote{{q, q, q}, {q, q, q}, {q, q, q}}
			}
			cap, e := s.CapFloorTermVolSurface(grid)
			if e != nil {
				t.Fatal(e)
			}
			x, e = cap.VolatilityTime(1.5, .015, false)
			volNear(t, x, want, e)
		}
	}
}
