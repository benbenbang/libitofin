package itofin

import (
	"math"
	"testing"
)

func TestVolatilityCubeGoBufferOwnershipAndSABRLimit(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	settings, _ := s.NewSettings()
	ref := volDate(t, 15, 6, 2026)
	if e = settings.SetEvaluationDate(ref); e != nil {
		t.Fatal(e)
	}
	dc, _ := s.Actual365Fixed()
	fixedDC, _ := s.Thirty360BondBasis()
	cal, _ := s.Target()
	eur, _ := s.EUR()
	curve, e := s.NewFlatForward(ref, .05, dc)
	if e != nil {
		t.Fatal(e)
	}
	ibor, e := s.NewEuriborSixMonths(curve, settings)
	if e != nil {
		t.Fatal(e)
	}
	ic := SwapIndexConfig{Family: "test", Tenor: Period{2, Years}, FixedLegTenor: Period{1, Years}, SettlementDays: 2, Currency: eur, Calendar: cal, FixedLegConvention: ModifiedFollowing, FixedLegDayCounter: fixedDC, Index: ibor, Settings: settings}
	index, e := s.NewSwapIndex(ic)
	if e != nil {
		t.Fatal(e)
	}
	ic.Tenor = Period{1, Years}
	short, e := s.NewSwapIndex(ic)
	if e != nil {
		t.Fatal(e)
	}
	atm, e := s.ConstantSwaptionVolatility(ConstantRateVolConfig{Calendar: cal, Convention: ModifiedFollowing, DayCounter: dc, Settings: settings, Volatility: .2})
	if e != nil {
		t.Fatal(e)
	}
	cfg := SwaptionVolatilityCubeConfig{ATMVol: atm, OptionTenors: []Period{{1, Years}, {5, Years}, {10, Years}}, SwapTenors: []Period{{2, Years}, {5, Years}, {10, Years}}, StrikeSpreads: []float64{-.01, 0, .01}, SwapIndexBase: index, ShortSwapIndexBase: short, Settings: settings}
	for node := 0; node < 9; node++ {
		row := make([]*SimpleQuote, 3)
		for k := range row {
			v := 0.
			if k != 1 {
				v = .001 * float64(node+1) * float64(k+1)
			}
			row[k], e = s.NewSimpleQuote(v)
			if e != nil {
				t.Fatal(e)
			}
		}
		cfg.VolSpreads = append(cfg.VolSpreads, row)
	}
	cube, e := s.InterpolatedSwaptionVolatilityCube(cfg)
	if e != nil {
		t.Fatal(e)
	}
	for i, o := range cfg.OptionTenors {
		for j, swap := range cfg.SwapTenors {
			strike, e := cube.ATMStrikeFromTenor(o, swap)
			if e != nil {
				t.Fatal(e)
			}
			for k, spread := range cfg.StrikeSpreads {
				want := .2
				if k != 1 {
					want += .001 * float64(i*3+j+1) * float64(k+1)
				}
				x, e := cube.Volatility(o, swap, strike+spread, true)
				volNear(t, x, want, e)
			}
		}
	}
	if e = cfg.VolSpreads[4][2].SetValue(.07); e != nil {
		t.Fatal(e)
	}
	strike, e := cube.ATMStrikeFromTenor(Period{5, Years}, Period{5, Years})
	if e != nil {
		t.Fatal(e)
	}
	x, e := cube.Volatility(Period{5, Years}, Period{5, Years}, strike+.01, true)
	volNear(t, x, .27, e)
	for _, row := range cfg.VolSpreads {
		for _, q := range row {
			if e = q.SetValue(0); e != nil {
				t.Fatal(e)
			}
		}
		guess := make([]*SimpleQuote, 4)
		for k, v := range []float64{.2, 1, 0, 0} {
			guess[k], e = s.NewSimpleQuote(v)
			if e != nil {
				t.Fatal(e)
			}
		}
		cfg.ParametersGuess = append(cfg.ParametersGuess, guess)
	}
	cfg.IsParameterFixed = [4]bool{true, true, true, true}
	sabr, e := s.SabrSwaptionVolatilityCube(cfg)
	if e != nil {
		t.Fatal(e)
	}
	strike, e = sabr.ATMStrikeFromTenor(Period{5, Years}, Period{5, Years})
	if e != nil {
		t.Fatal(e)
	}
	for _, spread := range cfg.StrikeSpreads {
		x, e = sabr.Volatility(Period{5, Years}, Period{5, Years}, strike+spread, true)
		volNear(t, x, .2, e)
	}
	if e = atm.Close(); e != nil {
		t.Fatal(e)
	}
	if e = index.Close(); e != nil {
		t.Fatal(e)
	}
	if e = short.Close(); e != nil {
		t.Fatal(e)
	}
	x, e = sabr.Volatility(Period{5, Years}, Period{5, Years}, strike, true)
	volNear(t, x, .2, e)
	if e = cube.Close(); e != nil {
		t.Fatal(e)
	}
	if e = cube.Close(); e != nil {
		t.Fatal(e)
	}
	if _, e = cube.ATMStrikeFromTenor(Period{5, Years}, Period{5, Years}); e == nil {
		t.Fatal("closed cube queried")
	}
}
func TestOptionletStripperGoQueriesAndAdapterOwnership(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	settings, _ := s.NewSettings()
	ref := volDate(t, 28, 10, 2013)
	if e = settings.SetEvaluationDate(ref); e != nil {
		t.Fatal(e)
	}
	dc, _ := s.Actual365Fixed()
	cal, _ := s.Target()
	curve, e := s.NewFlatForward(ref, .04, dc)
	if e != nil {
		t.Fatal(e)
	}
	index, e := s.NewEuriborSixMonths(curve, settings)
	if e != nil {
		t.Fatal(e)
	}
	grid := RateVolGridConfig{Calendar: cal, DayCounter: dc, Settings: settings, Convention: Following}
	for n := 1; n <= 10; n++ {
		grid.OptionTenors = append(grid.OptionTenors, Period{int32(n), Years})
		grid.Strikes = append(grid.Strikes, float64(n)/100)
		row := make([]float64, 10)
		for j := range row {
			row[j] = .18
		}
		grid.Volatilities = append(grid.Volatilities, row)
	}
	surface, e := s.CapFloorTermVolSurface(grid)
	if e != nil {
		t.Fatal(e)
	}
	stripper, e := s.OptionletStripper1(OptionletStripperConfig{TermVolSurface: surface, IborIndex: index})
	if e != nil {
		t.Fatal(e)
	}
	rates, e := stripper.ATMOptionletRates()
	if e != nil || len(rates) == 0 {
		t.Fatal(rates, e)
	}
	total := 0.
	for _, r := range rates {
		total += r
	}
	x, e := stripper.SwitchStrike()
	volNear(t, x, total/float64(len(rates)), e)
	adapter, e := s.StrippedOptionletAdapter(stripper, settings)
	if e != nil {
		t.Fatal(e)
	}
	if e = stripper.Close(); e != nil {
		t.Fatal(e)
	}
	if e = surface.Close(); e != nil {
		t.Fatal(e)
	}
	x, e = adapter.Volatility(Period{2, Years}, .04, false)
	if e != nil || math.IsNaN(x) || x <= 0 {
		t.Fatal(x, e)
	}
	// Normal stripping is rejected by the core at the lazy solve, not construction.
	surface, e = s.CapFloorTermVolSurface(grid)
	if e != nil {
		t.Fatal(e)
	}
	normal, e := s.OptionletStripper1(OptionletStripperConfig{TermVolSurface: surface, IborIndex: index, VolatilityType: Normal})
	if e != nil {
		t.Fatal(e)
	}
	if _, e = normal.SwitchStrike(); e == nil {
		t.Fatal("unsupported normal stripping succeeded")
	}
	if _, e = s.StrippedOptionletAdapter(normal, settings); e == nil {
		t.Fatal("adapter accepted failed normal strip")
	}
}

// Public QuantLib swaptionvolstructuresutilities fixture, matching the Python
// facade's original 3e-4 ATM and 12e-4 spread calibration tolerances.
func TestSABRCubeFreeCalibrationPythonOracle(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	ref := volDate(t, 15, 6, 2026)
	settings, _ := s.NewSettings()
	if e = settings.SetEvaluationDate(ref); e != nil {
		t.Fatal(e)
	}
	dc, _ := s.Actual365Fixed()
	curveDC, _ := s.Actual360()
	fixedDC, _ := s.Thirty360BondBasis()
	cal, _ := s.Target()
	eur, _ := s.EUR()
	curve, e := s.NewFlatForward(ref, .05, curveDC)
	if e != nil {
		t.Fatal(e)
	}
	ibor, e := s.NewEuriborSixMonths(curve, settings)
	if e != nil {
		t.Fatal(e)
	}
	ic := SwapIndexConfig{Family: "EuriborSwapIsdaFixA", Tenor: Period{2, Years}, FixedLegTenor: Period{1, Years}, SettlementDays: 2, Currency: eur, Calendar: cal, FixedLegConvention: ModifiedFollowing, FixedLegDayCounter: fixedDC, Index: ibor, Settings: settings}
	index, e := s.NewSwapIndex(ic)
	if e != nil {
		t.Fatal(e)
	}
	ic.Tenor = Period{1, Years}
	short, e := s.NewSwapIndex(ic)
	if e != nil {
		t.Fatal(e)
	}
	quoteRows := func(values [][]float64) [][]*SimpleQuote {
		rows := make([][]*SimpleQuote, len(values))
		for i, row := range values {
			rows[i] = make([]*SimpleQuote, len(row))
			for j, v := range row {
				q, e := s.NewSimpleQuote(v)
				if e != nil {
					t.Fatal(e)
				}
				rows[i][j] = q
			}
		}
		return rows
	}
	options := []Period{{1, Months}, {6, Months}, {1, Years}, {5, Years}, {10, Years}, {30, Years}}
	swaps := []Period{{1, Years}, {5, Years}, {10, Years}, {30, Years}}
	vols := [][]float64{{.1300, .1560, .1390, .1220}, {.1440, .1580, .1460, .1260}, {.1600, .1590, .1470, .1290}, {.1640, .1470, .1370, .1220}, {.1400, .1300, .1250, .1100}, {.1130, .1090, .1070, .0930}}
	atm, e := s.SwaptionVolatilityMatrix(RateVolGridConfig{Calendar: cal, Convention: ModifiedFollowing, DayCounter: dc, Settings: settings, OptionTenors: options, SwapTenors: swaps, Quotes: quoteRows(vols)})
	if e != nil {
		t.Fatal(e)
	}
	spreads := [][]float64{{.0599, .0049, 0, -.0001, .0127}, {.0729, .0086, 0, -.0024, .0098}, {.0738, .0102, 0, -.0039, .0065}, {.0465, .0063, 0, -.0032, -.0010}, {.0558, .0084, 0, -.0050, -.0057}, {.0576, .0083, 0, -.0043, -.0014}, {.0437, .0059, 0, -.0030, -.0006}, {.0533, .0078, 0, -.0045, -.0046}, {.0545, .0079, 0, -.0042, -.0020}}
	guesses := make([][]float64, 9)
	for i := range guesses {
		guesses[i] = []float64{.2, .5, .4, 0}
	}
	cfg := SwaptionVolatilityCubeConfig{ATMVol: atm, OptionTenors: []Period{{1, Years}, {10, Years}, {30, Years}}, SwapTenors: []Period{{2, Years}, {10, Years}, {30, Years}}, StrikeSpreads: []float64{-.020, -.005, 0, .005, .020}, VolSpreads: quoteRows(spreads), SwapIndexBase: index, ShortSwapIndexBase: short, Settings: settings, ParametersGuess: quoteRows(guesses), IsATMCalibrated: true}
	cube, e := s.SabrSwaptionVolatilityCube(cfg)
	if e != nil {
		t.Fatal(e)
	}
	for _, o := range options {
		for _, swap := range swaps {
			strike, e := cube.ATMStrikeFromTenor(o, swap)
			if e != nil {
				t.Fatal(e)
			}
			got, e := cube.Volatility(o, swap, strike, true)
			if e != nil {
				t.Fatal(e)
			}
			want, e := atm.Volatility(o, swap, strike, true)
			if e != nil {
				t.Fatal(e)
			}
			if math.Abs(got-want) > 3e-4 {
				t.Fatalf("ATM recovery: %.15g vs %.15g", got, want)
			}
		}
	}
	for i, o := range cfg.OptionTenors {
		for j, swap := range cfg.SwapTenors {
			strike, e := cube.ATMStrikeFromTenor(o, swap)
			if e != nil {
				t.Fatal(e)
			}
			base, e := atm.Volatility(o, swap, strike, true)
			if e != nil {
				t.Fatal(e)
			}
			for k, spread := range cfg.StrikeSpreads {
				got, e := cube.Volatility(o, swap, strike+spread, true)
				if e != nil {
					t.Fatal(e)
				}
				if math.Abs(got-base-spreads[i*3+j][k]) > 12e-4 {
					t.Fatalf("smile recovery node %d/%d/%d: %g", i, j, k, got-base)
				}
			}
		}
	}
}
