package itofin

import (
	"errors"
	"os"
	"regexp"
	"testing"
)

func TestVersionAndNativeError(t *testing.T) {
	version := Version()
	if !regexp.MustCompile(`^[0-9]+\.[0-9]+\.[0-9]+(-[0-9A-Za-z.-]+)?(\+[0-9A-Za-z.-]+)?$`).MatchString(version) {
		t.Fatalf("unexpected native version %q", version)
	}
	if want := os.Getenv("ITOFIN_EXPECTED_VERSION"); want != "" && version != want {
		t.Fatalf("native version %q, expected %q", version, want)
	}
	_, err := DateFromSerial(0)
	var native *Error
	if !errors.As(err, &native) || native.Code == 0 || native.Message == "" {
		t.Fatalf("missing structured error: %v", err)
	}
}
