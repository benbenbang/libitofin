// Package itofin exposes the native libitofin quantitative finance library.
// Live objects belong to a Session and must be explicitly closed. A Session
// serializes native calls on one OS thread; separate sessions are independent.
package itofin

/*
#include "itofin.h"
*/
import "C"

import (
	"errors"
	"fmt"
	"runtime"
	"sync"
	"sync/atomic"
)

var (
	ErrCallbackReentry = errors.New("itofin: session calls are not allowed during bootstrap callbacks")
	ErrClosed          = errors.New("itofin: session is closed")
	ErrSessionMismatch = errors.New("itofin: objects belong to different sessions")
)

// Error carries a stable native status code and the diagnostic message.
type Error struct {
	Code    int32
	Message string
}

func (e *Error) Error() string { return fmt.Sprintf("itofin (%d): %s", e.Code, e.Message) }
func ffiError(status C.int32_t, e *C.ItofinError) error {
	if status == 0 {
		return nil
	}
	return &Error{Code: int32(status), Message: C.GoString(&e.message[0])}
}
func errNilArgument(name string) error { return fmt.Errorf("itofin: %s must not be nil", name) }

type request struct {
	fn   func() error
	done chan error
}

// Session owns a native object graph. Its methods may be called concurrently;
// calls execute serially. Close waits for active calls, then releases the graph.
// Calls during a bootstrap callback, including Close, return ErrCallbackReentry.
// Do not copy a Session. No finalizer accesses thread-confined Rust state.
type Session struct {
	callbackActive atomic.Bool
	ctx            *C.ItofinContext
	queue          chan request
	stopped        chan struct{}
	gate           sync.RWMutex
	closed         bool
	closeErr       error
}

// NewSession creates a native context on its dedicated OS thread.
func NewSession() (*Session, error) {
	s := &Session{queue: make(chan request), stopped: make(chan struct{})}
	ready := make(chan error, 1)
	go func() {
		runtime.LockOSThread()
		defer runtime.UnlockOSThread()
		defer close(s.stopped)
		var e C.ItofinError
		if C.itofin_abi_version() != 1 {
			ready <- errors.New("itofin: incompatible native ABI version")
			return
		}
		err := ffiError(C.itofin_context_new(&s.ctx, &e), &e)
		ready <- err
		if err != nil {
			return
		}
		for r := range s.queue {
			r.done <- r.fn()
		}
	}()
	if err := <-ready; err != nil {
		<-s.stopped
		return nil, err
	}
	return s, nil
}

func (s *Session) invoke(fn func() error) error {
	if s == nil {
		return errNilArgument("session")
	}
	if s.callbackActive.Load() {
		return ErrCallbackReentry
	}
	s.gate.RLock()
	defer s.gate.RUnlock()
	if s.closed || s.queue == nil {
		return ErrClosed
	}
	r := request{fn: fn, done: make(chan error, 1)}
	s.queue <- r
	return <-r.done
}

// Close is idempotent and waits for every previously admitted call.
func (s *Session) Close() error {
	if s == nil {
		return nil
	}
	if s.callbackActive.Load() {
		return ErrCallbackReentry
	}
	s.gate.Lock()
	defer s.gate.Unlock()
	if s.closed {
		return s.closeErr
	}
	s.closed = true
	if s.queue == nil {
		return nil
	}
	r := request{done: make(chan error, 1), fn: func() error {
		var e C.ItofinError
		err := ffiError(C.itofin_context_free(s.ctx, &e), &e)
		s.ctx = nil
		return err
	}}
	s.queue <- r
	s.closeErr = <-r.done
	close(s.queue)
	<-s.stopped
	return s.closeErr
}

type object struct {
	session *Session
	id      uint64
}

func sameSession(s *Session, objects ...object) error {
	if s == nil {
		return errNilArgument("session")
	}
	for _, o := range objects {
		if o.session == nil || o.id == 0 {
			return errors.New("itofin: uninitialized object")
		}
		if o.session != s {
			return ErrSessionMismatch
		}
	}
	return nil
}

// Close releases this object's external handle. Other native objects retain
// dependencies they own. Subsequent operations on this handle return an error.
func (o object) Close() error {
	if o.session == nil || o.id == 0 {
		return nil
	}
	err := o.session.invoke(func() error {
		var e C.ItofinError
		err := ffiError(C.itofin_handle_release(o.session.ctx, C.uint64_t(o.id), &e), &e)
		// Handles are globally unique and never reused. An unknown handle
		// owned by this wrapper was already released, including by a copy.
		// Keep no permanent Go tombstones for completed native objects.
		var native *Error
		if errors.As(err, &native) && native.Code == int32(C.ITOFIN_INVALID_HANDLE) {
			return nil
		}
		return err
	})
	if errors.Is(err, ErrClosed) {
		return nil
	}
	return err
}
