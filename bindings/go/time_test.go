package itofin

import (
	"math"
	"sync"
	"testing"
)

func testDate(t *testing.T, day, month, year int) Date {
	t.Helper()
	d, err := NewDate(day, month, year)
	if err != nil {
		t.Fatal(err)
	}
	return d
}
func TestDateLeapRangeAndPeriodEquality(t *testing.T) {
	if _, err := NewDate(29, 2, 2100); err == nil {
		t.Fatal("accepted non-leap century")
	}
	d := testDate(t, 29, 2, 2000)
	if d.Day() != 29 || d.Month() != 2 || d.Year() != 2000 {
		t.Fatal(d)
	}
	shifted, err := d.AddDays(1)
	if err != nil || shifted != testDate(t, 1, 3, 2000) {
		t.Fatal(shifted, err)
	}
	for _, n := range []int64{math.MaxInt64, math.MinInt64} {
		if _, err := d.AddDays(n); err == nil {
			t.Fatal("accepted overflow")
		}
	}
	if _, err := testDate(t, 31, 12, 2199).AddDays(1); err == nil {
		t.Fatal("accepted upper overflow")
	}
	back, err := shifted.SubtractDays(1)
	if err != nil || back != d || shifted.DaysSince(d) != 1 {
		t.Fatal(back, err)
	}
	if d.String() != "Date(29, 2, 2000)" {
		t.Fatal(d.String())
	}
	values := map[Date]string{d: "leap"}
	if values[testDate(t, 29, 2, 2000)] != "leap" {
		t.Fatal("date map key")
	}
	week, _ := NewPeriod(1, Weeks)
	days, _ := NewPeriod(7, Days)
	month, _ := NewPeriod(1, Months)
	thirty, _ := NewPeriod(30, Days)
	if week.String() != "Period(1, Weeks)" {
		t.Fatal(week.String())
	}
	if !week.Equal(days) || month.Equal(thirty) {
		t.Fatal("period semantic equality")
	}
	if map[Period]bool{week.Key(): true}[days.Key()] != true {
		t.Fatal("period canonical key")
	}
}
func TestIMMAndCalendarOracles(t *testing.T) {
	d := testDate(t, 20, 3, 2024)
	ok, err := IsIMMDate(d, true)
	if err != nil || !ok {
		t.Fatal(ok, err)
	}
	next, err := NextIMMDate(d, true)
	if err != nil || next != testDate(t, 19, 6, 2024) {
		t.Fatal(next, err)
	}
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	cal, err := s.WeekendsOnly()
	if err != nil {
		t.Fatal(err)
	}
	adjusted, err := cal.Adjust(testDate(t, 31, 8, 2024), ModifiedFollowing)
	if err != nil || adjusted != testDate(t, 30, 8, 2024) {
		t.Fatal(adjusted, err)
	}
	if _, err := cal.Advance(testDate(t, 31, 12, 2199), 1, Days, Following, false); err == nil {
		t.Fatal("accepted date overflow")
	}
	// An ordinary input error must not poison the session.
	if _, err := cal.Adjust(d, Following); err != nil {
		t.Fatal(err)
	}
	target, _ := s.Target()
	uk, _ := s.UnitedKingdom()
	null, _ := s.NullCalendar()
	for _, c := range []*Calendar{target, uk, null, cal} {
		if name, err := c.Name(); err != nil || name == "" {
			t.Fatal(name, err)
		}
	}
}
func TestSchedulesDayCountersAndLifetimes(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	other, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer other.Close()
	cal, err := s.NullCalendar()
	if err != nil {
		t.Fatal(err)
	}
	cfg := ScheduleConfig{Start: testDate(t, 1, 1, 2024), End: testDate(t, 1, 1, 2025), Calendar: cal, Frequency: Quarterly, Convention: Unadjusted}
	if _, err := other.NewSchedule(cfg); err != ErrSessionMismatch {
		t.Fatal(err)
	}
	schedule, err := s.NewSchedule(cfg)
	if err != nil {
		t.Fatal(err)
	}
	if err := cal.Close(); err != nil {
		t.Fatal(err)
	}
	dates, err := schedule.Dates()
	if err != nil || len(dates) != 5 || dates[1] != testDate(t, 1, 4, 2024) {
		t.Fatal(dates, err)
	}
	if _, err := schedule.Date(-1); err == nil {
		t.Fatal("accepted negative index")
	}
	if _, err := schedule.Date(5); err == nil {
		t.Fatal("accepted invalid index")
	}
	dc, err := s.ActualActualISDA()
	if err != nil {
		t.Fatal(err)
	}
	v, err := dc.YearFraction(cfg.Start, cfg.End)
	if err != nil || v != 1 {
		t.Fatal(v, err)
	}
	a, _ := s.Actual360()
	b, _ := other.Actual360()
	equal, err := a.Equal(b)
	if err != nil || !equal {
		t.Fatal(equal, err)
	}
	v, err = a.YearFraction(cfg.Start, cfg.End)
	if err != nil || math.Abs(v-366./360.) > 1e-14 {
		t.Fatal(v, err)
	}
	var wg sync.WaitGroup
	for i := 0; i < 10; i++ {
		wg.Add(1)
		go func() {
			defer wg.Done()
			if _, err := dc.YearFraction(cfg.Start, cfg.End); err != nil {
				t.Error(err)
			}
		}()
	}
	wg.Wait()
	if err := dc.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := dc.YearFraction(cfg.Start, cfg.End); err == nil {
		t.Fatal("released handle usable")
	}
}

func TestAllTimeConventions(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	start, end := testDate(t, 1, 1, 2024), testDate(t, 1, 1, 2025)
	factories := []func() (*DayCounter, error){s.Actual360, s.Actual365Fixed, s.ActualActualISDA, s.Thirty360BondBasis}
	expected := []float64{366. / 360., 366. / 365., 1, 1}
	for i, f := range factories {
		dc, err := f()
		if err != nil {
			t.Fatal(err)
		}
		value, err := dc.YearFraction(start, end)
		if err != nil || math.Abs(value-expected[i]) > 1e-14 {
			t.Fatal(value, err)
		}
		if key, err := dc.Key(); err != nil || key == "" {
			t.Fatal(key, err)
		}
		if repr, err := dc.Repr(); err != nil || repr == "" {
			t.Fatal(repr, err)
		}
	}
	cal, err := s.WeekendsOnly()
	if err != nil {
		t.Fatal(err)
	}
	if repr, err := cal.Repr(); err != nil || repr == "" {
		t.Fatal(repr, err)
	}
	for rule := ModifiedFollowing; rule <= Nearest; rule++ {
		if _, err := cal.Adjust(start, rule); err != nil {
			t.Fatal(err)
		}
	}
	for frequency := Annual; frequency <= Monthly; frequency++ {
		for rule := Backward; rule <= CDS2015; rule++ {
			schedule, err := s.NewSchedule(ScheduleConfig{Start: start, End: end, Frequency: frequency, Calendar: cal, Convention: Following, Rule: &rule})
			if err != nil {
				t.Fatalf("frequency %d rule %d: %v", frequency, rule, err)
			}
			size, err := schedule.Size()
			if err != nil || size < 2 {
				t.Fatal(size, err)
			}
			schedule.Close()
		}
	}
}
