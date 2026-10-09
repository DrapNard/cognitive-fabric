import pytest

from cogfabric.security import SecurityKernel, SecurityError, safe_arithmetic


def test_math_evaluation_allowed():
    assert safe_arithmetic("(7+5)*3") == 36
    assert safe_arithmetic("2**4") == 16
    assert safe_arithmetic("18/3") == 6


@pytest.mark.parametrize("expression", [
    "__import__('os').system('id')", "(1).__class__", "[1,2][0]", "sum([1,2])",
    "1/0", "2**999999999", "10**10**10", "1 << 16", "x+1",
])
def test_math_prevents_code_execution(expression):
    with pytest.raises((SecurityError, SyntaxError, ZeroDivisionError, OverflowError)):
        safe_arithmetic(expression)


def test_privilege_separation():
    security = SecurityKernel()
    security.authorize("jev-r", "knowledge.search")
    with pytest.raises(SecurityError):
        security.authorize("jev-r", "candidate.promote")
    with pytest.raises(SecurityError):
        security.authorize("unknown", "math.evaluate")
