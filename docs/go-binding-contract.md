# C and Go binding implementation contract

The core stays FFI-agnostic and retains its settled Shared/Rc ownership design.
The coverage target is the committed Python stubs at baseline revision
`bf6c5d640c1a0aac3184d8a24d77897e2df2b5ae`. CI enforces this with
`scripts/check_go_coverage.py --strict --baseline`, reporting newer unmapped
Python APIs separately. Invalid mapping references remain errors in both modes.

## Native boundary

`crates/libitofin-ffi` produces `libitofin_ffi` as a static/shared native library,
distinct from the Python extension's `itofin` artifact.
Modules use `crate::boundary::{Context, ItofinError, BindingError, BindingResult,
with_context, without_context, input_slice, output, check_ptr}`.

Every export is prefixed `itofin_`, uses `#[unsafe(no_mangle)]` and `unsafe extern
"C"`, returns an i32 status, and takes a final `*mut ItofinError`. It must enter
`with_context(ctx,error,|context| ...)` (or `without_context` for stateless work).
Unsafe pointer operations inside the wrapper require an explicit unsafe block.
Validate output pointers before creating/inserting objects. Scalars and repr(C)
records cross by value; arrays use pointer + length, caller-owned output buffers.
Do not return Rust strings, vectors, references, enums with unchecked discriminants,
or trait object pointers. A null pointer is allowed only for an empty slice.

`Context::insert<T: 'static>(value) -> BindingResult<u64>` retains an object.
`Context::get<T: Clone + 'static>(id) -> BindingResult<T>` retrieves a clone.
Use existing Shared<T>, SharedMut<T> aliases. Instruments store SharedMut<T>;
polymorphic curves store Handle<dyn YieldTermStructure> etc, so every concrete
constructor exposes the same native base representation. Handle IDs are globally
unique, type checked, context scoped, and explicitly released. No core graph is
shared across contexts. Handle zero represents None only where documented.

Date inputs use the core serial number (i32), validated by the time module.
Native day counters/calendars/settings use handles. Calendars are stored as
`calendar_api::NativeCalendar`, retaining the core calendar and optional holiday
horizon; retrieve core calendars through `time_api::calendar`. Shared helper functions in
time_api are `date(serial:i32)->BindingResult<Date>` and
`day_counter(context:&Context,handle:u64)->BindingResult<DayCounter>`.
Ibor indexes are stored as `indexes_api::NativeIbor`, retaining the original
`NativeCalendar` for fixing-calendar inspectors; retrieve core indexes through
`indexes_api::ibor_index`. Built-in index families have unrestricted calendars.
Settings stored as Shared<Settings<Date>>. Ordinary curve adapters retrieve
Handle<dyn YieldTermStructure>, volatility Handle<dyn BlackVolTermStructure>.

## Go boundary

Go module path is github.com/benbenbang/libitofin/bindings/go, package itofin.
Each Go feature file uses a cgo preamble `#include "itofin.h"`.
`Session` owns a native context created, used and destroyed on one locked OS
thread. Feature methods call `s.invoke(func() error { ... })`; `s.ctx` is
accessible only inside this closure. `ffiError(status C.int32_t, e *C.ItofinError)
error` converts errors. Each object embeds `object` with `session *Session` and
`id uint64`. `object.Close() error` releases it; reject cross-session arguments
before native calls with `sameSession(s, objects ...object) error`.
Objects are always returned as pointers. Constructors are Session methods.
Prefer explicit Go configuration structs to long positional argument lists.
Use Go-owned scalar arrays for synchronous native calls only; retain no Go memory
in Rust. Batch large numerical work across the boundary.

## Review gates

Tests must cover native numerical oracles, invalid arguments, missing results,
dependency lifetimes, Close behavior, session isolation and concurrent callers.
Never weaken tolerances to make tests pass. Coverage mappings must identify exact
Python symbols and real C/Go implementations; unsupported symbols remain explicit.
Private consumer source and test fixtures must not be copied into this repository.
