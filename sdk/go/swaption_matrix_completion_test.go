package itofin

import (
	"encoding/csv"
	"math"
	"os"
	"strconv"
	"testing"
)

func matrixOracle(t *testing.T) [][]string {
	t.Helper()
	file, err := os.Open("../../crates/libitofin/tests/fixtures/swaption_matrix/quantlib.csv")
	if err != nil {
		t.Fatal(err)
	}
	defer file.Close()
	rows, err := csv.NewReader(file).ReadAll()
	if err != nil {
		t.Fatal(err)
	}
	return rows[1:]
}

func TestSwaptionMatrixFiveConstructorOracles(t *testing.T) {
	for _, flat := range []bool{false, true} {
		s, err := NewSession()
		if err != nil {
			t.Fatal(err)
		}
		t.Cleanup(func() { s.Close() })
		cal, _ := s.Target()
		dc, _ := s.Actual365Fixed()
		settings, _ := s.NewSettings()
		today := volDate(t, 15, 6, 2026)
		if err := settings.SetEvaluationDate(today); err != nil {
			t.Fatal(err)
		}
		options := []Period{{1, Months}, {6, Months}, {1, Years}, {5, Years}, {10, Years}, {30, Years}}
		swaps := []Period{{1, Years}, {5, Years}, {10, Years}, {30, Years}}
		dates := make([]Date, len(options))
		for i, p := range options {
			dates[i], err = cal.Advance(today, p.Length, p.Unit, ModifiedFollowing, false)
			if err != nil {
				t.Fatal(err)
			}
		}
		values := make([][]float64, len(options))
		quotes := make([][]*SimpleQuote, len(options))
		for i := range options {
			values[i] = make([]float64, len(swaps))
			quotes[i] = make([]*SimpleQuote, len(swaps))
		}
		oracle := matrixOracle(t)
		for _, row := range oracle {
			if row[0] != "node" || row[1] != "0" {
				continue
			}
			i, _ := strconv.Atoi(row[2])
			j, _ := strconv.Atoi(row[3])
			values[i][j], _ = strconv.ParseFloat(row[4], 64)
			quotes[i][j], err = s.NewSimpleQuote(values[i][j])
			if err != nil {
				t.Fatal(err)
			}
		}
		surfaces := make([]*SwaptionVolatilityStructure, 5)
		for form := range surfaces {
			cfg := RateVolGridConfig{ReferenceDate: today, Calendar: cal, Convention: ModifiedFollowing,
				DayCounter: dc, OptionTenors: options, SwapTenors: swaps, Volatilities: values, FlatExtrapolation: flat}
			if form == 0 || form == 2 {
				cfg.Settings = settings
			}
			if form < 2 {
				cfg.Quotes = quotes
			}
			if form == 4 {
				cfg.OptionTenors = nil
				surfaces[form], err = s.SwaptionVolatilityMatrixDates(cfg, dates)
			} else {
				surfaces[form], err = s.SwaptionVolatilityMatrix(cfg)
			}
			if err != nil {
				t.Fatalf("form %d: %v", form, err)
			}
		}
		values[0][0] = 99
		near := func(got, want float64, err error) {
			t.Helper()
			if err != nil || math.Abs(got-want) > 1e-16 {
				t.Fatalf("got %.17g want %.17g: %v", got, want, err)
			}
		}
		count := 0
		for _, row := range oracle {
			if row[0] != "node" {
				continue
			}
			form, _ := strconv.Atoi(row[1])
			i, _ := strconv.Atoi(row[2])
			j, _ := strconv.Atoi(row[3])
			want, _ := strconv.ParseFloat(row[4], 64)
			got, err := surfaces[form].VolatilityDate(dates[i], float64(swaps[j].Length), .05, false)
			near(got, want, err)
			got, err = surfaces[form].Volatility(options[i], swaps[j], .05, false)
			near(got, want, err)
			count++
		}
		if count != 120 {
			t.Fatal(count)
		}
		for _, row := range oracle {
			if row[0] != "observe" {
				continue
			}
			form, _ := strconv.Atoi(row[1])
			expected := make([]float64, 3)
			for i := range expected {
				expected[i], _ = strconv.ParseFloat(row[i+2], 64)
			}
			if err := settings.SetEvaluationDate(volDate(t, 15, 6, 2025)); err != nil {
				t.Fatal(err)
			}
			got, err := surfaces[form].VolatilityDate(dates[0], 1, .02, false)
			near(got, expected[1], err)
			if err := settings.SetEvaluationDate(today); err != nil {
				t.Fatal(err)
			}
			if err := quotes[0][0].SetValue(.2); err != nil {
				t.Fatal(err)
			}
			got, err = surfaces[form].VolatilityDate(dates[0], 1, .02, false)
			near(got, expected[2], err)
			if err := quotes[0][0].SetValue(.13); err != nil {
				t.Fatal(err)
			}
		}
		for _, row := range quotes {
			for _, quote := range row {
				if err := quote.Close(); err != nil {
					t.Fatal(err)
				}
			}
		}
		for _, dependency := range []object{cal.object, dc.object, settings.object} {
			if err := dependency.Close(); err != nil {
				t.Fatal(err)
			}
		}
		for _, surface := range surfaces {
			got, err := surface.VolatilityDate(dates[0], 1, .02, false)
			near(got, .13, err)
			if err := surface.Close(); err != nil {
				t.Fatal(err)
			}
			if _, err := surface.VolatilityDate(dates[0], 1, .02, false); err == nil {
				t.Fatal("closed surface accepted")
			}
		}
	}
}

func TestSwaptionMatrixAdditionalConstructorErrors(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	cal, _ := s.Target()
	dc, _ := s.Actual365Fixed()
	settings, _ := s.NewSettings()
	today := volDate(t, 15, 6, 2026)
	dates := []Date{volDate(t, 15, 7, 2026), volDate(t, 15, 12, 2026)}
	cfg := RateVolGridConfig{ReferenceDate: today, Calendar: cal, DayCounter: dc,
		SwapTenors: []Period{{1, Years}, {5, Years}}, Volatilities: [][]float64{{.13, .15}, {.14, .16}}}
	for _, invalid := range [][]Date{nil, {dates[0], dates[0]}, {dates[1], dates[0]}, {today, dates[1]}, {dates[0]}} {
		if _, err := s.SwaptionVolatilityMatrixDates(cfg, invalid); err == nil {
			t.Fatal("invalid dates accepted", invalid)
		}
	}
	cfg.Shifts = [][]float64{{.01}, {.02}}
	if _, err := s.SwaptionVolatilityMatrixDates(cfg, dates); err == nil {
		t.Fatal("ragged shifts accepted")
	}
	cfg.Shifts = nil
	cfg.Settings = settings
	if _, err := s.SwaptionVolatilityMatrixDates(cfg, dates); err == nil {
		t.Fatal("moving dates accepted")
	}
	cfg.OptionTenors = []Period{{1, Months}, {6, Months}}
	if _, err := s.SwaptionVolatilityMatrix(cfg); err == nil {
		t.Fatal("missing evaluation date accepted")
	}
	if err := settings.SetEvaluationDate(today); err != nil {
		t.Fatal(err)
	}
	cfg.Volatilities[0][0] = math.NaN()
	if _, err := s.SwaptionVolatilityMatrix(cfg); err == nil {
		t.Fatal("nonfinite matrix accepted")
	}
	cfg.Volatilities[0][0] = .13
	other, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer other.Close()
	if _, err := other.SwaptionVolatilityMatrix(cfg); err == nil {
		t.Fatal("foreign dependencies accepted")
	}
	cfg.Settings = nil
	quote, _ := s.NewSimpleQuote(.13)
	cfg.Quotes = [][]*SimpleQuote{{quote, quote}, {quote, quote}}
	if err := quote.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.SwaptionVolatilityMatrix(cfg); err == nil {
		t.Fatal("closed quote accepted")
	}
}
