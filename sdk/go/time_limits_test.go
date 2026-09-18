package itofin

import (
	"math"
	"strconv"
	"testing"
)

// These inputs become valid components if silently narrowed modulo 2^32.
func TestDateComponentsRejectWideIntegersBeforeCConversion(t *testing.T) {
	if strconv.IntSize < 64 {
		t.Skip("wide Go int test requires 64-bit architecture")
	}
	width := int64(1) << 32
	for _, sign := range []int64{-1, 1} {
		offset := int(sign * width)
		for _, parts := range [][3]int{{15 + offset, 6, 2026}, {15, 6 + offset, 2026}, {15, 6, 2026 + offset}} {
			if d, err := NewDate(parts[0], parts[1], parts[2]); err == nil {
				t.Fatalf("wide date narrowed: %v -> %v", parts, d)
			}
		}
	}
}

func TestDateShiftsAndCalendarAdvanceRejectExtremeLengths(t *testing.T) {
	d := sessionMust(NewDate(15, 6, 2026))
	for _, n := range []int64{math.MinInt64, math.MaxInt64, -1 << 32, 1 << 32} {
		if _, err := d.AddDays(n); err == nil {
			t.Fatalf("AddDays accepted %d", n)
		}
		if _, err := d.SubtractDays(n); err == nil {
			t.Fatalf("SubtractDays accepted %d", n)
		}
	}
	s := sessionMust(NewSession())
	defer s.Close()
	calendar := sessionMust(s.NullCalendar())
	for _, unit := range []TimeUnit{Days, Weeks, Months, Years} {
		for _, n := range []int32{math.MinInt32, math.MaxInt32} {
			if _, err := calendar.Advance(d, n, unit, Unadjusted, false); err == nil {
				t.Fatalf("Advance accepted %d %v", n, unit)
			}
			// A Period is a duration, independent of the supported date range. Keep
			// its full signed i32 value; only applying it to a date can overflow.
			p := sessionMust(NewPeriod(n, unit))
			if p.Length != n || p.Unit != unit {
				t.Fatalf("period narrowed: %+v", p)
			}
		}
	}
	if got := sessionMust(calendar.Advance(d, 1, Days, Unadjusted, false)); got != sessionMust(d.AddDays(1)) {
		t.Fatal("extreme input poisoned context")
	}
}

func TestScheduleIndexRejectsWideIntegers(t *testing.T) {
	if strconv.IntSize < 64 {
		t.Skip("wide Go int test requires 64-bit architecture")
	}
	s := sessionMust(NewSession())
	defer s.Close()
	calendar := sessionMust(s.NullCalendar())
	start := sessionMust(NewDate(1, 1, 2026))
	end := sessionMust(NewDate(1, 1, 2027))
	schedule := sessionMust(s.NewSchedule(ScheduleConfig{Start: start, End: end, Calendar: calendar, Frequency: Quarterly, Convention: Unadjusted}))
	width := int64(1) << 32
	for _, n := range []int64{-width, width, width + 1, -width + 1, math.MaxInt64, math.MinInt64} {
		if d, err := schedule.Date(int(n)); err == nil {
			t.Fatalf("wide index %d returned %v", n, d)
		}
	}
	if got := sessionMust(schedule.Date(0)); got != start {
		t.Fatal("invalid index affected schedule")
	}
}
