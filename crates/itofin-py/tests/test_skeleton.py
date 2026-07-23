import itofin


def test_version_matches_workspace():
    assert itofin.__version__ == "0.1.0"


def test_itofin_error_is_exception_subclass():
    assert issubclass(itofin.ItofinError, Exception)
