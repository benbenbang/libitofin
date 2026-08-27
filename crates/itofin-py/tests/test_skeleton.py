# itofin library
from importlib.metadata import version

import itofin


def test_version_matches_workspace():
    assert itofin.__version__ == version("itofin")


def test_itofin_error_is_exception_subclass():
    assert issubclass(itofin.ItofinError, Exception)
