package itofin

import (
	"math"
	"sync"
	"testing"
)

func volDate(t *testing.T, d, m, y int) Date {
	t.Helper()
	x, e := NewDate(d, m, y)
	if e != nil {
		t.Fatal(e)
	}
	return x
}
func volNear(t *testing.T, got, want float64, err error) {
	t.Helper()
	if err != nil || math.Abs(got-want) > 1e-11 {
		t.Fatalf("got %.15g want %.15g: %v", got, want, err)
	}
}
func TestBlackVolatilityOraclesAndOwnership(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	dc, e := s.Actual365Fixed()
	if e != nil {
		t.Fatal(e)
	}
	ref := volDate(t, 1, 1, 2024)
	end, _ := ref.AddDays(365)
	v, e := s.BlackConstantVol(BlackConstantVolConfig{ReferenceDate: ref, Volatility: .2, DayCounter: dc})
	if e != nil {
		t.Fatal(e)
	}
	if e = dc.Close(); e != nil {
		t.Fatal(e)
	}
	x, e := v.BlackVol(2, 100, false)
	volNear(t, x, .2, e)
	x, e = v.BlackVariance(2, 100, false)
	volNear(t, x, .08, e)
	x, e = v.BlackForwardVol(1, 2, 100, false)
	volNear(t, x, .2, e)
	x, e = v.BlackForwardVariance(1, 2, 100, false)
	volNear(t, x, .04, e)
	x, e = v.BlackVolDate(end, 100, false)
	volNear(t, x, .2, e)
	x, e = v.BlackVarianceDate(end, 100, false)
	volNear(t, x, .04, e)
	if _, e = v.BlackVol(math.NaN(), 100, false); e == nil {
		t.Fatal("accepted NaN")
	}
	var wg sync.WaitGroup
	for i := 0; i < 16; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			x, e := v.BlackVol(1, 100, false)
			if e != nil || math.Abs(x-.2) > 1e-12 {
				t.Errorf("concurrent query %g %v", x, e)
			}
		}()
	}
	wg.Wait()
	if e = v.Close(); e != nil {
		t.Fatal(e)
	}
	if e = v.Close(); e != nil {
		t.Fatal(e)
	}
	if _, e = v.BlackVol(1, 100, false); e == nil {
		t.Fatal("released surface queried")
	}
}
func TestBlackVarianceGridAndExtrapolation(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	dc, _ := s.Actual365Fixed()
	ref := volDate(t, 1, 1, 2024)
	d1, _ := ref.AddDays(365)
	d2, _ := ref.AddDays(730)
	v, e := s.BlackVarianceCurve(BlackVarianceCurveConfig{ReferenceDate: ref, Dates: []Date{d1, d2}, Volatilities: []float64{.2, .3}, DayCounter: dc, ForceMonotoneVariance: true})
	if e != nil {
		t.Fatal(e)
	}
	x, e := v.BlackVariance(1.5, 100, false)
	volNear(t, x, .11, e)
	x, e = v.BlackVol(1.5, 100, false)
	volNear(t, x, math.Sqrt(.11/1.5), e)
	if _, e = v.BlackVol(3, 100, false); e == nil {
		t.Fatal("unapproved extrapolation")
	}
	if e = v.EnableExtrapolation(); e != nil {
		t.Fatal(e)
	}
	ok, e := v.AllowsExtrapolation()
	if e != nil || !ok {
		t.Fatal(ok, e)
	}
	x, e = v.BlackVol(3, 100, false)
	volNear(t, x, .3, e)
	if e = v.DisableExtrapolation(); e != nil {
		t.Fatal(e)
	}
	ok, e = v.AllowsExtrapolation()
	if e != nil || ok {
		t.Fatal(ok, e)
	}
	max, e := v.MaxDate()
	if e != nil || max != d2 {
		t.Fatal(max, e)
	}
	surf, e := s.BlackVarianceSurface(BlackVarianceSurfaceConfig{ReferenceDate: ref, Dates: []Date{d1, d2}, Strikes: []float64{90, 110}, Volatilities: [][]float64{{.2, .3}, {.2, .3}}, DayCounter: dc})
	if e != nil {
		t.Fatal(e)
	}
	x, e = surf.BlackVariance(1.5, 100, false)
	volNear(t, x, .11, e)
	x, e = surf.MinStrike()
	volNear(t, x, 90, e)
	x, e = surf.MaxStrike()
	volNear(t, x, 110, e)
	if _, e = surf.BlackVol(1, 120, false); e == nil {
		t.Fatal("out-of-range strike accepted")
	}
	if _, e = s.BlackVarianceSurface(BlackVarianceSurfaceConfig{ReferenceDate: ref, Dates: []Date{d1, d2}, Strikes: []float64{90, 110}, Volatilities: [][]float64{{.2}, {.2, .3}}, DayCounter: dc}); e == nil {
		t.Fatal("ragged grid accepted")
	}
	other, _ := NewSession()
	defer other.Close()
	if _, e = other.BlackConstantVol(BlackConstantVolConfig{ReferenceDate: ref, DayCounter: dc}); e == nil {
		t.Fatal("cross-session dependency accepted")
	}
	bad, e := s.BlackVarianceCurve(BlackVarianceCurveConfig{ReferenceDate: ref, Dates: []Date{d1, d2}, Volatilities: []float64{.2, .3}, DayCounter: dc, TimeExtrapolation: UseInterpolator})
	if e != nil {
		t.Fatal(e)
	}
	if _, e = bad.BlackVol(3, 100, true); e == nil {
		t.Fatal("unsupported extrapolator silently substituted")
	}
}
func TestSABRLognormalLimitAndParameters(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	v, e := s.SabrSmileSection(SabrSmileConfig{ExerciseTime: 2, Forward: 100, Alpha: .2, Beta: 1})
	if e != nil {
		t.Fatal(e)
	}
	for _, strike := range []float64{80, 100, 120} {
		x, e := v.Volatility(strike)
		volNear(t, x, .2, e)
		x, e = v.Variance(strike)
		volNear(t, x, .08, e)
	}
	for _, tt := range []struct {
		fn   func() (float64, error)
		want float64
	}{{v.ExerciseTime, 2}, {v.ATMLevel, 100}, {v.Alpha, .2}, {v.Beta, 1}, {v.Nu, 0}, {v.Rho, 0}} {
		x, e := tt.fn()
		volNear(t, x, tt.want, e)
	}
	if _, e = s.SabrSmileSection(SabrSmileConfig{ExerciseTime: 1, Forward: 100, Alpha: -1, Beta: 1}); e == nil {
		t.Fatal("invalid SABR alpha accepted")
	}
	if _, e = s.SabrSmileSection(SabrSmileConfig{ExerciseTime: 1, Forward: 100, Alpha: .2, Beta: 1, VolatilityType: Normal}); e == nil {
		t.Fatal("unsupported normal SABR accepted")
	}
}
func TestBlackLinearVarianceExtrapolation(t *testing.T) {
	s, e := NewSession()
	if e != nil {
		t.Fatal(e)
	}
	defer s.Close()
	dc, _ := s.Actual365Fixed()
	ref := volDate(t, 1, 1, 2024)
	d1, _ := ref.AddDays(365)
	d2, _ := ref.AddDays(730)
	v, e := s.BlackVarianceCurve(BlackVarianceCurveConfig{ReferenceDate: ref, Dates: []Date{d1, d2}, Volatilities: []float64{.2, .3}, DayCounter: dc, TimeExtrapolation: LinearVariance})
	if e != nil {
		t.Fatal(e)
	}
	x, e := v.BlackVariance(3, 100, true)
	volNear(t, x, .32, e)
}
