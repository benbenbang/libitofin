package itofin

import (
	"errors"
	"testing"
)

func TestVersionAndNativeError(t *testing.T) {
	if Version() != "0.20.0" {
		t.Fatalf("unexpected native version %q", Version())
	}
	_, err := DateFromSerial(0)
	var native *Error
	if !errors.As(err, &native) || native.Code == 0 || native.Message == "" {
		t.Fatalf("missing structured error: %v", err)
	}
}
