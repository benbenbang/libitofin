package itofin

import (
	"crypto/sha256"
	"encoding/csv"
	"encoding/hex"
	"encoding/json"
	"math"
	"os"
	"strconv"
	"strings"
	"sync"
	"testing"
)

func cpiCapFloorMarket(t *testing.T) (*Session, CpiPriceSurfaceConfig) {
	t.Helper()
	s, e := NewSession()
	inflProductsCheck(t, e)
	t.Cleanup(func() { s.Close() })
	settings, e := s.NewSettings()
	inflProductsCheck(t, e)
	today := testDate(t, 15, 6, 2025)
	inflProductsCheck(t, settings.SetEvaluationDate(today))
	index, e := s.NewUKRPI(settings)
	inflProductsCheck(t, e)
	for _, f := range []struct {
		y, m  int
		value float64
	}{{2024, 10, 99.5}, {2024, 11, 99.8}, {2024, 12, 100.0}, {2025, 1, 100.2}, {2025, 2, 100.4}, {2025, 3, 100.6}, {2025, 4, 100.8}, {2025, 5, 101.0}} {
		inflProductsCheck(t, index.AddFixing(testDate(t, 1, f.m, f.y), f.value))
	}
	dc, e := s.Actual365Fixed()
	inflProductsCheck(t, e)
	curve, e := s.NewInterpolatedZeroInflationCurve(InflationCurveConfig{ReferenceDate: today, Frequency: Monthly, DayCounter: dc}, []Date{testDate(t, 1, 1, 2025), testDate(t, 1, 1, 2035)}, []float64{.025, .025})
	inflProductsCheck(t, e)
	inflProductsCheck(t, index.LinkTo(curve))
	discount, e := s.NewFlatForward(today, .04, dc)
	inflProductsCheck(t, e)
	cal, e := s.NullCalendar()
	inflProductsCheck(t, e)
	return s, CpiPriceSurfaceConfig{Nominal: 100000, BaseRate: .025, ObservationLag: Period{3, Months}, Calendar: cal, Convention: Unadjusted, DayCounter: dc, Index: index, Interpolation: CpiFlat, Discount: discount, CapStrikes: []float64{.03, .04}, FloorStrikes: []float64{.01, .02}, MaturityYears: []int32{1, 3, 5}, CapPrices: [][]float64{{.08, .15, .24}, {.04, .10, .16}}, FloorPrices: [][]float64{{.02, .04, .06}, {.035, .065, .095}}, Settings: settings}
}
func cpiCapFloorFixture(t *testing.T) map[string]float64 {
	t.Helper()
	bytes, e := os.ReadFile("testdata/cpi-capfloor-native.tsv")
	inflProductsCheck(t, e)
	hash := sha256.Sum256(bytes)
	if hex.EncodeToString(hash[:]) != "e05bf46a6d323939ace0b267bb58c415ea8a4b0ac26839e673d393f0901f6b29" {
		t.Fatal("nativefixture hash mismatch")
	}
	metadata, e := os.ReadFile("testdata/cpi-capfloor-native.provenance.json")
	inflProductsCheck(t, e)
	var provenance struct {
		FixtureSHA string `json:"fixture_sha256"`
		RowCount   int    `json:"row_count"`
	}
	inflProductsCheck(t, json.Unmarshal(metadata, &provenance))
	if provenance.FixtureSHA != hex.EncodeToString(hash[:]) || provenance.RowCount != 120 {
		t.Fatal("nativefixture provenance mismatch")
	}
	f, e := os.Open("testdata/cpi-capfloor-native.tsv")
	inflProductsCheck(t, e)
	defer f.Close()
	r := csv.NewReader(f)
	r.Comma = '\t'
	rows, e := r.ReadAll()
	inflProductsCheck(t, e)
	out := map[string]float64{}
	for _, row := range rows[1:] {
		v, e := strconv.ParseFloat(row[1], 64)
		inflProductsCheck(t, e)
		if !isFiniteCpi(v) {
			t.Fatal("nonfinite fixture")
		}
		out[row[0]] = v
	}
	if len(out) != 120 {
		t.Fatal("nativefixture rowcount")
	}
	return out
}
func cpiCapFloorConfig(t *testing.T, a CpiPriceSurfaceConfig, engine *InterpolatingCpiCapFloorEngine) CpiCapFloorConfig {
	return CpiCapFloorConfig{Type: Call, Nominal: 100000, BaseCPI: 100, Start: testDate(t, 15, 6, 2025), Maturity: testDate(t, 15, 6, 2027), FixingCalendar: a.Calendar, PaymentCalendar: a.Calendar, FixingConvention: Unadjusted, PaymentConvention: Unadjusted, Strike: .025, Index: a.Index, ObservationLag: Period{3, Months}, Interpolation: CpiFlat, Settings: a.Settings, Engine: engine}
}
func TestCpiCapFloorNativePriceSurfaceAndEngine(t *testing.T) {
	s, a := cpiCapFloorMarket(t)
	surface, e := s.NewCpiCapFloorTermPriceSurface(a)
	inflProductsCheck(t, e)
	engine, e := s.NewInterpolatingCpiCapFloorEngine(surface)
	inflProductsCheck(t, e)
	expected := cpiCapFloorFixture(t)
	count := 0
	for name, value := range expected {
		parts := strings.Split(name, "_")
		if len(parts) != 3 || (parts[0] != "cap" && parts[0] != "floor") {
			continue
		}
		y, e := strconv.Atoi(parts[1])
		inflProductsCheck(t, e)
		k, e := strconv.ParseFloat(parts[2], 64)
		inflProductsCheck(t, e)
		var actual float64
		if parts[0] == "cap" {
			actual, e = surface.CapPrice(testDate(t, 15, 6, 2025+y), k)
		} else {
			actual, e = surface.FloorPrice(testDate(t, 15, 6, 2025+y), k)
		}
		inflProductsCheck(t, e)
		inflProductsNear(t, actual, value, 2e-14)
		count++
	}
	if count != 90 {
		t.Fatalf("oracle grid %d", count)
	}
	cfg := cpiCapFloorConfig(t, a, engine)
	for _, mode := range []struct {
		value CpiInterpolationType
		label string
	}{{CpiFlat, "flat"}, {CpiLinear, "linear"}} {
		cfg.Interpolation = mode.value
		for _, lag := range []int32{3, 5} {
			cfg.ObservationLag = Period{lag, Months}
			for _, kind := range []struct {
				value OptionType
				label string
			}{{Call, "cap"}, {Put, "floor"}} {
				cfg.Type = kind.value
				instrument, e := s.NewCpiCapFloor(cfg)
				inflProductsCheck(t, e)
				value, e := instrument.NPV()
				inflProductsCheck(t, e)
				inflProductsNear(t, value, expected[mode.label+"_"+kind.label+"_lag"+strconv.Itoa(int(lag))], 2e-14)
				inflProductsCheck(t, instrument.Close())
			}
		}
		cfg.Type = Call
		cfg.ObservationLag = Period{3, Months}
		cfg.Nominal = 17
		cfg.BaseCPI = 321
		cfg.Start = testDate(t, 15, 6, 2024)
		instrument, e := s.NewCpiCapFloor(cfg)
		inflProductsCheck(t, e)
		value, e := instrument.NPV()
		inflProductsCheck(t, e)
		inflProductsNear(t, value, expected[mode.label+"_cap_lag3"], 2e-14)
	}
}
func TestCpiCapFloorOwnershipErrorsSnapshotAndConcurrency(t *testing.T) {
	s, a := cpiCapFloorMarket(t)
	surface, e := s.NewCpiCapFloorTermPriceSurface(a)
	inflProductsCheck(t, e)
	engine, e := s.NewInterpolatingCpiCapFloorEngine(surface)
	inflProductsCheck(t, e)
	cfg := cpiCapFloorConfig(t, a, engine)
	instrument, e := s.NewCpiCapFloor(cfg)
	inflProductsCheck(t, e)
	fix, pay, e := instrument.Dates()
	inflProductsCheck(t, e)
	if fix.Serial() != testDate(t, 15, 3, 2027).Serial() || pay.Serial() != cfg.Maturity.Serial() {
		t.Fatal("incorrect CPI dates")
	}
	expired, e := instrument.IsExpired()
	inflProductsCheck(t, e)
	if expired {
		t.Fatal("premature expiry")
	}
	reference, e := surface.ReferenceDate()
	inflProductsCheck(t, e)
	if reference.Serial() != testDate(t, 15, 6, 2025).Serial() {
		t.Fatal("wrong snapshot reference")
	}
	inflProductsCheck(t, a.Settings.SetEvaluationDate(testDate(t, 16, 6, 2025)))
	reference, e = surface.ReferenceDate()
	inflProductsCheck(t, e)
	if reference.Serial() != testDate(t, 15, 6, 2025).Serial() {
		t.Fatal("snapshot reference rolled")
	}
	inflProductsCheck(t, a.Settings.SetEvaluationDate(testDate(t, 15, 6, 2025)))
	expected := cpiCapFloorFixture(t)["flat_cap_lag3"]
	inflProductsCheck(t, surface.Close())
	if _, e = surface.ReferenceDate(); e == nil {
		t.Fatal("closed surface readback accepted")
	}
	var nilSurface *CpiCapFloorTermPriceSurface
	if _, e = nilSurface.ReferenceDate(); e == nil {
		t.Fatal("nil surface readback accepted")
	}
	inflProductsCheck(t, engine.Close())
	inflProductsCheck(t, a.Index.Close())
	inflProductsCheck(t, a.Discount.Close())
	inflProductsCheck(t, a.Calendar.Close())
	inflProductsCheck(t, a.DayCounter.Close())
	var group sync.WaitGroup
	for range 6 {
		group.Add(1)
		go func() {
			defer group.Done()
			v, e := instrument.NPV()
			if e != nil || !isFiniteCpi(v) || math.Abs(v-expected) > 2e-14 {
				t.Errorf("retained NPV %g %v", v, e)
			}
		}()
	}
	group.Wait()
	inflProductsCheck(t, a.Settings.SetEvaluationDate(testDate(t, 16, 6, 2025)))
	value, e := instrument.NPV()
	inflProductsCheck(t, e)
	inflProductsNear(t, value, expected, 2e-14)
	inflProductsCheck(t, a.Settings.SetEvaluationDate(testDate(t, 15, 6, 2027)))
	expired, e = instrument.IsExpired()
	inflProductsCheck(t, e)
	if expired {
		t.Fatal("expired on raw maturity")
	}
	inflProductsCheck(t, a.Settings.SetEvaluationDate(testDate(t, 16, 6, 2027)))
	value, e = instrument.NPV()
	inflProductsCheck(t, e)
	inflProductsNear(t, value, 0, 0)
	expired, e = instrument.IsExpired()
	inflProductsCheck(t, e)
	if !expired {
		t.Fatal("missing expiry")
	}
	inflProductsCheck(t, instrument.Close())
	if _, e = instrument.IsExpired(); e == nil {
		t.Fatal("closed expiry readback accepted")
	}
	if _, e = instrument.NPV(); e == nil {
		t.Fatal("closed instrument accepted")
	}
	var nilInstrument *CpiCapFloor
	if _, e = nilInstrument.IsExpired(); e == nil {
		t.Fatal("nil expiry readback accepted")
	}
	if _, e = nilInstrument.NPV(); e == nil {
		t.Fatal("nil instrument accepted")
	}
}
func TestCpiCapFloorInputAndSessionGuards(t *testing.T) {
	s, a := cpiCapFloorMarket(t)
	for _, change := range []func(*CpiPriceSurfaceConfig){func(x *CpiPriceSurfaceConfig) { x.Nominal = math.NaN() }, func(x *CpiPriceSurfaceConfig) { x.ObservationLag = Period{30, Days} }, func(x *CpiPriceSurfaceConfig) { x.MaturityYears = []int32{1, 3, math.MaxInt32} }, func(x *CpiPriceSurfaceConfig) { x.CapPrices = [][]float64{{1}, {2}} }} {
		wrong := a
		change(&wrong)
		if _, e := s.NewCpiCapFloorTermPriceSurface(wrong); e == nil {
			t.Fatal("invalid surface accepted")
		}
	}
	surface, e := s.NewCpiCapFloorTermPriceSurface(a)
	inflProductsCheck(t, e)
	if _, e = surface.CapPrice(Date{}, .03); e == nil {
		t.Fatal("null date accepted")
	}
	engine, e := s.NewInterpolatingCpiCapFloorEngine(surface)
	inflProductsCheck(t, e)
	cfg := cpiCapFloorConfig(t, a, engine)
	cfg.ObservationLag = Period{30, Days}
	if _, e = s.NewCpiCapFloor(cfg); e == nil {
		t.Fatal("mixed lag unit accepted")
	}
	cfg.ObservationLag = Period{1, Months}
	wrong, e := s.NewCpiCapFloor(cfg)
	inflProductsCheck(t, e)
	if _, e = wrong.NPV(); e == nil {
		t.Fatal("negative lag difference accepted")
	}
	foreign, e := NewSession()
	inflProductsCheck(t, e)
	defer foreign.Close()
	if _, e = foreign.NewInterpolatingCpiCapFloorEngine(surface); e == nil {
		t.Fatal("mixed sessions accepted")
	}
	cfg.ObservationLag = Period{3, Months}
	valid, e := s.NewCpiCapFloor(cfg)
	inflProductsCheck(t, e)
	value, e := valid.NPV()
	inflProductsCheck(t, e)
	inflProductsNear(t, value, cpiCapFloorFixture(t)["flat_cap_lag3"], 2e-14)
}

func isFiniteCpi(v float64) bool { return !math.IsNaN(v) && !math.IsInf(v, 0) }
