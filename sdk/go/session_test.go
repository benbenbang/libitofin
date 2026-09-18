package itofin

import (
	"errors"
	"sync"
	"testing"
)

func sessionMust[T any](value T, err error) T {
	if err != nil {
		panic(err)
	}
	return value
}

func TestSessionCloseWaitsForAdmittedWork(t *testing.T) {
	s := sessionMust(NewSession())
	entered, proceed := make(chan struct{}), make(chan struct{})
	workDone, closeDone := make(chan error, 1), make(chan error, 1)
	events := make(chan string, 2)
	go func() { workDone <- s.invoke(func() error { close(entered); <-proceed; events <- "work"; return nil }) }()
	<-entered
	closing := make(chan struct{})
	go func() { close(closing); err := s.Close(); events <- "close"; closeDone <- err }()
	<-closing
	close(proceed)
	if err := <-workDone; err != nil {
		t.Fatal(err)
	}
	if err := <-closeDone; err != nil {
		t.Fatal(err)
	}
	if <-events != "work" || <-events != "close" {
		t.Fatal("session closed before admitted work finished")
	}
	called := false
	if err := s.invoke(func() error { called = true; return nil }); !errors.Is(err, ErrClosed) || called {
		t.Fatalf("post-close invoke: called=%v err=%v", called, err)
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestSessionConcurrentCallsAndClose(t *testing.T) {
	s := sessionMust(NewSession())
	quote := sessionMust(s.NewSimpleQuote(42))
	start := make(chan struct{})
	results := make(chan error, 65)
	var wg sync.WaitGroup
	for range 64 {
		wg.Add(1)
		go func() {
			defer wg.Done()
			<-start
			value, err := quote.Value()
			if err == nil && value != 42 {
				err = errors.New("incorrect quote value")
			}
			results <- err
		}()
	}
	wg.Add(1)
	go func() { defer wg.Done(); <-start; results <- s.Close() }()
	close(start)
	wg.Wait()
	close(results)
	for err := range results {
		if err != nil && !errors.Is(err, ErrClosed) {
			t.Fatal(err)
		}
	}
	if err := quote.Close(); err != nil {
		t.Fatal(err)
	}
	if err := s.Close(); err != nil {
		t.Fatal(err)
	}
}

func TestObjectCloseCopiesAndConcurrentDoubleClose(t *testing.T) {
	s := sessionMust(NewSession())
	defer s.Close()
	quote := sessionMust(s.NewSimpleQuote(42))
	copied := *quote
	var wg sync.WaitGroup
	errs := make(chan error, 64)
	for range 32 {
		wg.Add(2)
		go func() { defer wg.Done(); errs <- quote.Close() }()
		go func() { defer wg.Done(); errs <- copied.Close() }()
	}
	wg.Wait()
	close(errs)
	for err := range errs {
		if err != nil {
			t.Fatal(err)
		}
	}
	if _, err := quote.Value(); err == nil {
		t.Fatal("closed quote usable")
	}
	if _, err := copied.Value(); err == nil {
		t.Fatal("copy resurrected closed quote")
	}
	// The next handle must remain independent after the old one was released.
	next := sessionMust(s.NewSimpleQuote(7))
	if err := quote.Close(); err != nil {
		t.Fatal(err)
	}
	if got := sessionMust(next.Value()); got != 7 {
		t.Fatalf("new quote value %g", got)
	}
}

func TestClosedDependencyAndForeignObject(t *testing.T) {
	s := sessionMust(NewSession())
	defer s.Close()
	other := sessionMust(NewSession())
	defer other.Close()
	date := sessionMust(NewDate(15, 6, 2026))
	expiry := sessionMust(date.AddDays(360))
	dc := sessionMust(s.Actual360())
	settings := sessionMust(s.NewSettings())
	if err := settings.SetEvaluationDate(date); err != nil {
		t.Fatal(err)
	}
	cfg := BlackScholesConfig{Spot: 100, RiskFreeRate: .05, DividendYield: .02, Volatility: .2, ReferenceDate: date, DayCounter: dc}
	process := sessionMust(s.NewBlackScholesProcess(cfg))
	option := sessionMust(s.NewVanillaOption(Call, 100, expiry, settings))
	value := sessionMust(option.Price(process))
	if err := dc.Close(); err != nil {
		t.Fatal(err)
	}
	if _, err := s.NewBlackScholesProcess(cfg); err == nil {
		t.Fatal("constructor accepted closed dependency")
	}
	if got := sessionMust(option.NPV()); got != value {
		t.Fatal("closing dependency invalidated retained native graph")
	}
	if _, err := other.NewVanillaOption(Call, 100, expiry, settings); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign settings: %v", err)
	}
	otherDC := sessionMust(other.Actual360())
	cfg.DayCounter = otherDC
	foreignProcess := sessionMust(other.NewBlackScholesProcess(cfg))
	if err := option.SetEngine(foreignProcess); !errors.Is(err, ErrSessionMismatch) {
		t.Fatalf("foreign engine: %v", err)
	}
	if got := sessionMust(option.NPV()); got != value {
		t.Fatal("rejected foreign engine changed option")
	}
}
