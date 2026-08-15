import pytest
import rynth


@pytest.fixture(scope="module")
def core() -> rynth.Core:
    return rynth.Core()


def test_core_plugins(core: rynth.Core) -> None:
    namespaces = [p.namespace for p in core.plugins() if p is not None]
    assert "std" in namespaces
    with pytest.raises(AttributeError):
        _ = core.definitely_not_a_plugin


def test_plugin_getattr(core: rynth.Core) -> None:
    assert any(f.name == "BlankClip" for f in core.std.functions() if f is not None)
    with pytest.raises(AttributeError):
        _ = core.std.DefinitelyNotAFunction


def test_module_level_core_proxy() -> None:
    assert isinstance(rynth.core, rynth.CoreProxy)
    clip = rynth.core.std.BlankClip(width=32, height=32, length=2)
    assert clip.num_frames == 2
    assert rynth.core.num_threads > 0
    assert rynth.core.core is rynth.core.core
