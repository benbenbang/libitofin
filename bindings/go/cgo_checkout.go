//go:build !itofin_external

package itofin

/*
#cgo CFLAGS: -I${SRCDIR}/../../crates/libitofin-ffi/include
#cgo LDFLAGS: -L${SRCDIR}/../../target/release -litofin_ffi
*/
import "C"
