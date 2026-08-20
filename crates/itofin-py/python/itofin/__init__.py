"""Python bindings for libitofin, a Rust port of QuantLib.

Re-exports the compiled ``itofin`` extension module so ``import itofin``
resolves to the native bindings (classes, functions and submodules).
"""

from .itofin import *

if hasattr(itofin, "__all__"):
    __all__ = itofin.__all__

if hasattr(itofin, "__doc__"):
    __doc__ = itofin.__doc__
