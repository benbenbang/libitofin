# C and Go binding implementation contract

The core stays FFI-agnostic and retains its settled Shared/Rc ownership design.
CI enforces both the full-current Python stub inventory and the historical
`bf6c5d640c1a0aac3184d8a24d77897e2df2b5ae` implementation baseline. Explicit
nonconstructible Python enum declarations have reviewed language-specific
classifications, counted separately from Go mappings. Go enums use typed integer
constants and explicit conversions; native APIs validate their discriminants.
Classifications cannot replace baseline implementations or hide new APIs.

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
`calendar_api::NativeCalendar`, retaining the core calendar, earliest supported year, and optional holiday
horizon; retrieve core calendars through `time_api::calendar`. Shared helper functions in
time_api are `date(serial:i32)->BindingResult<Date>` and
`day_counter(context:&Context,handle:u64)->BindingResult<DayCounter>`.
Ibor indexes are stored as `indexes_api::NativeIbor`, retaining the original
`NativeCalendar` for fixing-calendar inspectors; retrieve core indexes through
`indexes_api::ibor_index`. Built-in index families have unrestricted calendars.
Settings stored as Shared<Settings<Date>>. Ordinary curve adapters retrieve
Handle<dyn YieldTermStructure>, volatility Handle<dyn BlackVolTermStructure>.

## Go boundary

Go module path is github.com/benbenbang/libitofin/sdk/go, package itofin.
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

## Global bootstrap callbacks

`PiecewiseCurveConfig` accepts `AdditionalVariables`, `AdditionalDates`, and
`AdditionalPenalties` with global bootstrap on linear or log-linear discount
curves. `NewSimpleQuoteVariables` retains external quotes; optional guesses and
lower bounds follow quote order. Initial guesses must be finite and strictly
above supplied finite bounds. Variables require a penalty callback.

Penalty callbacks receive copied `BootstrapState` arrays: trial node times and
discount factors, variable quote values, and additional-helper residuals. Use
these snapshots for callback calculations. Callable dates return Go dates.
Callback errors, panics, invalid residuals, and changing residual counts surface
as ordinary errors; they do not poison the session.

Callbacks run synchronously on the session's worker. Calls into that session,
including `Close` from another goroutine while a callback is active, return
`ErrCallbackReentry`. Native context entry and release are also guarded before
borrowing the context. Callback code must not wait for an already queued session
operation. Close the session explicitly after callbacks return.

Rust retains an integer `cgo.Handle`, never a Go pointer. Callback state transfers
when the C constructor sets `adopted`; C callers initialize it to false before
each call. Exactly one release occurs at the last native owner, including failed
construction after adoption. Closing a Go curve wrapper does not release a
callback still needed by an index or another retained consumer.

Set `FuturesRateHelperConfig.DoNotObserveConvexity` when the convexity quote is a
jointly fitted variable. The default continues observing changes. Existing C
entrypoints and configuration layouts retain their original behavior.

## Review gates

Tests must cover native numerical oracles, invalid arguments, missing results,
dependency lifetimes, Close behavior, session isolation and concurrent callers.
Never weaken tolerances to make tests pass. Coverage mappings must identify exact
Python symbols and real C/Go implementations; unsupported symbols remain explicit.
Private consumer source and test fixtures must not be copied into this repository.
