import idiomatic
import pytest


def test_lint_reports_compare_none():
    hits = idiomatic.lint("if x == None:\n    pass\n", "python")
    assert any(h.id == "compare-none" for h in hits)
    h = next(h for h in hits if h.id == "compare-none")
    assert h.start < h.end  # a real byte range


def test_autofix_rewrites_in_place():
    fixed, n = idiomatic.autofix("if x == None:\n    pass\n", "python")
    assert fixed == "if x is None:\n    pass\n"
    assert n == 1


def test_autofix_leaves_good_code_untouched():
    fixed, n = idiomatic.autofix("if x is None:\n    pass\n", "python")
    assert n == 0
    assert fixed == "if x is None:\n    pass\n"


def test_render_skill_python():
    skill = idiomatic.render_skill("python")
    assert "name: idiomatic-python" in skill
    assert "Use `is None`" in skill


def test_typescript_supported():
    fixed, _ = idiomatic.autofix("const x = a;\n", "typescript")  # valid lang, no-op
    assert fixed == "const x = a;\n"
    skill = idiomatic.render_skill("typescript")
    assert "name: idiomatic-typescript" in skill


def test_unknown_language_raises():
    with pytest.raises(ValueError):
        idiomatic.lint("x", "cobol")
    with pytest.raises(ValueError):
        idiomatic.autofix("x", "cobol")


def test_render_skill_unknown_language_raises():
    with pytest.raises(ValueError):
        idiomatic.render_skill("cobol")


def test_linter_handle_reuses_cascade():
    lint = idiomatic.Linter()
    hits = lint.lint("if x == None:\n    pass\n", "python")
    assert any(h.id == "compare-none" for h in hits)
    fixed, n = lint.autofix("if x == None:\n    pass\n", "python")
    assert fixed == "if x is None:\n    pass\n"
    assert n == 1
    assert "name: idiomatic-python" in lint.render_skill("python")


def test_linter_unknown_language_raises():
    lint = idiomatic.Linter()
    with pytest.raises(ValueError):
        lint.lint("x", "cobol")
