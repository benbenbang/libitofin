package itofin

/*
#include "itofin.h"
*/
import "C"

// Version returns the version of the loaded native library.
func Version() string { return C.GoString(C.itofin_version()) }
