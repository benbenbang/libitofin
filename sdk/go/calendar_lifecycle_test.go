package itofin

import (
	"errors"
	"sync"
	"testing"
)

func TestCalendarJointLifetimeAndKeys(t *testing.T) {
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
	us, _ := s.UnitedStates()
	uk, _ := s.UnitedKingdom()
	foreign, _ := other.UnitedKingdom()
	if equal, err := uk.Equal(foreign); err != nil || !equal {
		t.Fatal(equal, err)
	}
	key, _ := uk.Key()
	otherKey, _ := foreign.Key()
	if map[string]string{key: "GBP"}[otherKey] != "GBP" {
		t.Fatal(key, otherKey)
	}
	if equal, err := uk.Equal(us); err != nil || equal {
		t.Fatal(equal, err)
	}
	if equal, err := uk.Equal(nil); err != nil || equal {
		t.Fatal(equal, err)
	}
	if _, err := s.JointCalendar([]*Calendar{us, foreign}); !errors.Is(err, ErrSessionMismatch) {
		t.Fatal(err)
	}
	for _, calendars := range [][]*Calendar{nil, {nil}} {
		if _, err := s.JointCalendar(calendars); err == nil {
			t.Fatal("accepted invalid members")
		}
	}
	if _, err := s.JointCalendar([]*Calendar{us}, "JoİnHolidays"); err == nil {
		t.Fatal("accepted Unicode case folding")
	}
	if _, err := s.JointCalendar([]*Calendar{us}, "bad"); err == nil {
		t.Fatal("accepted invalid rule")
	}
	if _, err := s.JointCalendar([]*Calendar{us}, "JoinHolidays", "JoinHolidays"); err == nil {
		t.Fatal("accepted extra rules")
	}
	intersection, _ := s.JointCalendar([]*Calendar{us, uk})
	union, _ := s.JointCalendar([]*Calendar{us, uk}, "joinbusinessdays")
	reverse, _ := s.JointCalendar([]*Calendar{uk, us})
	if equal, err := intersection.Equal(reverse); err != nil || equal {
		t.Fatal("joint ordering", equal, err)
	}
	if name, err := intersection.Name(); err != nil || name != "JoinHolidays(US settlement, UK settlement)" {
		t.Fatal(name, err)
	}
	us.Close()
	uk.Close()
	july := testDate(t, 4, 7, 2025)
	if got, err := intersection.IsHoliday(july); err != nil || !got {
		t.Fatal(got, err)
	}
	if got, err := union.IsBusinessDay(july); err != nil || !got {
		t.Fatal(got, err)
	}
	if _, err := s.JointCalendar([]*Calendar{us}); err == nil {
		t.Fatal("accepted closed member")
	}
	var wg sync.WaitGroup
	for i := 0; i < 10; i++ {
		wg.Go(func() {
			if got, err := intersection.IsHoliday(july); err != nil || !got {
				t.Error(got, err)
			}
		})
	}
	wg.Wait()
	intersection.Close()
	if _, err := intersection.Key(); err == nil {
		t.Fatal("closed key")
	}
	if _, err := intersection.IsHoliday(july); err == nil {
		t.Fatal("closed query")
	}
	other.Close()
	if _, err := foreign.Key(); !errors.Is(err, ErrClosed) {
		t.Fatal(err)
	}
}

func TestCalendarHorizonDoesNotPoisonSession(t *testing.T) {
	s, err := NewSession()
	if err != nil {
		t.Fatal(err)
	}
	defer s.Close()
	for _, tc := range []struct {
		build func(...string) (*Calendar, error)
		year  int
	}{{s.Indonesia, 2014}, {s.SaudiArabia, 2022}} {
		cal, _ := tc.build()
		inside, outside := testDate(t, 1, 6, tc.year), testDate(t, 1, 1, tc.year+1)
		joint, _ := s.JointCalendar([]*Calendar{cal})
		union, _ := s.JointCalendar([]*Calendar{cal}, "JoinBusinessDays")
		for _, c := range []*Calendar{cal, joint, union} {
			for _, query := range []func(Date) (bool, error){c.IsHoliday, c.IsBusinessDay, c.IsWeekend} {
				if _, err := query(outside); err == nil {
					t.Fatal("accepted outside horizon")
				}
			}
			if _, err := c.HolidayList(inside, outside); err == nil {
				t.Fatal("horizon list")
			}
			if _, err := c.BusinessDaysBetween(inside, outside); err == nil {
				t.Fatal("horizon count")
			}
			if _, err := c.Adjust(outside, Unadjusted); err == nil {
				t.Fatal("horizon adjust")
			}
			if _, err := c.Advance(testDate(t, 31, 12, tc.year), 1, Days, Following, false); err == nil {
				t.Fatal("crossed horizon")
			}
			if _, err := c.Advance(testDate(t, 31, 12, tc.year), 1, Years, Unadjusted, true); err == nil {
				t.Fatal("unadjusted month-end crossed horizon")
			}
			if _, err := c.IsBusinessDay(inside); err != nil {
				t.Fatal("poisoned session", err)
			}
		}
	}
	saudi, _ := s.SaudiArabia()
	for _, date := range []Date{testDate(t, 27, 6, 2013), testDate(t, 29, 6, 2013)} {
		if got, err := saudi.IsWeekend(date); err != nil || !got {
			t.Fatal(got, err)
		}
	}
}
