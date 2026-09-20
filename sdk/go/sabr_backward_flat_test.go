package itofin

import (
	"encoding/csv"
	"math"
	"os"
	"strconv"
	"testing"
)

func sabrValue(t *testing.T, cube *SwaptionVolatilityCube) float64 {
	t.Helper()
	v, err := cube.Volatility(Period{2, Years}, Period{5, Years}, .05, false)
	if err != nil {
		t.Fatal(err)
	}
	return v
}

func sabrBuild(t *testing.T, s *Session, cfg SwaptionVolatilityCubeConfig) *SwaptionVolatilityCube {
	t.Helper()
	cube, err := s.SabrSwaptionVolatilityCube(cfg)
	if err != nil {
		t.Fatal(err)
	}
	t.Cleanup(func() { cube.Close() })
	return cube
}

func TestSABRBackwardFlatQuantLibOracle(t *testing.T) {
	f, err := os.Open("../../crates/libitofin/tests/fixtures/sabr_backward_flat/oracle.csv")
	if err != nil {
		t.Fatal(err)
	}
	defer f.Close()
	rows, err := csv.NewReader(f).ReadAll()
	if err != nil || len(rows) != 73 {
		t.Fatal(len(rows), err)
	}
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	cfg, _, _, _ := sabrOracleConfig(t, s)
	var cubes [2][2]*SwaptionVolatilityCube
	for dense := range 2 {
		cfg.IsATMCalibrated = dense == 1
		for flag := range 2 {
			cfg.BackwardFlat = flag == 1
			cubes[dense][flag] = sabrBuild(t, s, cfg)
		}
	}
	parse := func(text string) float64 {
		v, err := strconv.ParseFloat(text, 64)
		if err != nil {
			t.Fatal(err)
		}
		return v
	}
	for _, row := range rows[1:] {
		dense := int(parse(row[0]))
		o := Period{int32(parse(row[1][:len(row[1])-1])), Years}
		w := Period{int32(parse(row[2][:len(row[2])-1])), Years}
		for flag, cube := range cubes[dense] {
			got, err := cube.Volatility(o, w, parse(row[3]), true)
			want := parse(row[4+flag])
			if err != nil || !(math.Abs(got-want) < 1e-6) {
				t.Fatalf("row %v flag %d: %.16g want %.16g: %v", row, flag, got, want, err)
			}
		}
	}
}

func TestSABRBackwardFlatUpdatesAndRetainedInputs(t *testing.T) {
	for _, dense := range []bool{false, true} {
		s, err := NewSession()
		if err != nil {
			t.Fatal(err)
		}
		defer s.Close()
		cfg, _, _, _ := sabrOracleConfig(t, s)
		cfg.IsATMCalibrated, cfg.BackwardFlat = dense, true
		cube := sabrBuild(t, s, cfg)
		verify := func() float64 {
			got := sabrValue(t, cube)
			fresh := sabrBuild(t, s, cfg)
			if !(math.Abs(got-sabrValue(t, fresh)) < 1e-14) {
				t.Fatal("stale backward-flat cube")
			}
			control := cfg
			control.BackwardFlat = false
			if !(math.Abs(got-sabrValue(t, sabrBuild(t, s, control))) > 1e-3) {
				t.Fatal("backward-flat flag lost")
			}
			return got
		}
		initial := verify()
		if err := cfg.VolSpreads[3][0].SetValue(.0465 + .01); err != nil {
			t.Fatal(err)
		}
		afterQuote := verify()
		if !(math.Abs(afterQuote-initial) > 1e-7) {
			t.Fatal("quote update ignored")
		}
		if err := cfg.Settings.SetEvaluationDate(volDate(t, 16, 6, 2026)); err != nil {
			t.Fatal(err)
		}
		afterDate := verify()
		if !(math.Abs(afterDate-afterQuote) > 1e-10) {
			t.Fatal("date update ignored")
		}
		cold := sabrBuild(t, s, cfg)
		for _, grid := range [][][]*SimpleQuote{cfg.VolSpreads, cfg.ParametersGuess} {
			for _, row := range grid {
				for _, q := range row {
					if err := q.Close(); err != nil {
						t.Fatal(err)
					}
				}
			}
		}
		for _, close := range []func() error{cfg.ATMVol.Close, cfg.SwapIndexBase.Close, cfg.ShortSwapIndexBase.Close, cfg.Settings.Close} {
			if err := close(); err != nil {
				t.Fatal(err)
			}
		}
		if sabrValue(t, cube) != afterDate || sabrValue(t, cold) != afterDate {
			t.Fatal("retained dependencies changed")
		}
		if _, err := s.SabrSwaptionVolatilityCube(cfg); err == nil {
			t.Fatal("closed dependencies accepted")
		}
		if sabrValue(t, cube) != afterDate {
			t.Fatal("failed construction poisoned session")
		}
		if err := cube.Close(); err != nil {
			t.Fatal(err)
		}
		if _, err := cube.Volatility(Period{2, Years}, Period{5, Years}, .05, false); err == nil {
			t.Fatal("closed cube accepted")
		}
	}
}

func TestSABRBackwardFlatInvalidInputs(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	cfg, _, _, _ := sabrOracleConfig(t, s)
	cfg.BackwardFlat = true
	if _, err := s.InterpolatedSwaptionVolatilityCube(cfg); err == nil {
		t.Fatal("interpolated cube silently accepted backward-flat flag")
	}
	for _, option := range []bool{false, true} {
		bad := cfg
		if option {
			bad.OptionTenors = cfg.OptionTenors[:1]
		} else {
			bad.SwapTenors = cfg.SwapTenors[:1]
		}
		bad.VolSpreads, bad.ParametersGuess = cfg.VolSpreads[:3], cfg.ParametersGuess[:3]
		if _, err := s.SabrSwaptionVolatilityCube(bad); err == nil {
			t.Fatal("singleton axis accepted")
		}
	}
	other, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer other.Close()
	foreign, err := other.NewSimpleQuote(.0465)
	if err != nil {
		t.Fatal(err)
	}
	original := cfg.VolSpreads[3][0]
	cfg.VolSpreads[3][0] = foreign
	if _, err := s.SabrSwaptionVolatilityCube(cfg); err == nil {
		t.Fatal("foreign quote accepted")
	}
	cfg.VolSpreads[3][0] = original
	if !(sabrValue(t, sabrBuild(t, s, cfg)) > 0) {
		t.Fatal("failed construction poisoned session")
	}
}
