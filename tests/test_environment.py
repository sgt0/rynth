from collections.abc import Generator
from typing import override
from weakref import ReferenceType

import pytest
import rynth


class _Policy(rynth.EnvironmentPolicy):
    api: rynth.EnvironmentPolicyAPI
    current: rynth.EnvironmentData | None

    def __init__(self) -> None:
        self.current = None

    @override
    def on_policy_registered(self, api: rynth.EnvironmentPolicyAPI) -> None:
        self.api = api

    @override
    def get_current_environment(self) -> rynth.EnvironmentData | None:
        return self.current

    @override
    def set_environment(
        self, environment: rynth.EnvironmentData | None
    ) -> rynth.EnvironmentData | None:
        previous = self.current
        self.current = environment
        return previous

    def new_environment(self) -> rynth.EnvironmentData:
        return self.api.create_environment()


@pytest.fixture(autouse=True)
def _fresh_policy() -> Generator[None, None, None]:
    rynth.clear_policy()
    yield
    rynth.clear_policy()


def test_set_output_registers_clip() -> None:
    clip = rynth.core.std.BlankClip(width=32, height=32, length=3)
    clip.set_output(0)

    out = rynth.get_output(0)
    assert isinstance(out, rynth.VideoOutputTuple)
    assert out.clip is clip
    assert out.alpha is None
    assert out.alt_output == 0


def test_video_output_tuple_is_tuple_like() -> None:
    clip = rynth.core.std.BlankClip(width=16, height=16, length=1)
    clip.set_output(0, alt_output=2)

    out = rynth.get_output(0)
    assert isinstance(out, rynth.VideoOutputTuple)
    assert len(out) == 3
    assert out[0] is clip
    assert out[1] is None
    assert out[2] == 2

    unpacked_clip, unpacked_alpha, unpacked_alt = out
    assert unpacked_clip is clip
    assert unpacked_alpha is None
    assert unpacked_alt == 2

    with pytest.raises(IndexError):
        _ = out[3]


def test_set_output_uses_given_index() -> None:
    a = rynth.core.std.BlankClip(width=16, height=16, length=1, color=[10, 128, 128])
    b = rynth.core.std.BlankClip(width=16, height=16, length=1, color=[20, 128, 128])
    a.set_output(0)
    b.set_output(5)

    a_out = rynth.get_output(0)
    b_out = rynth.get_output(5)
    assert isinstance(a_out, rynth.VideoOutputTuple)
    assert isinstance(b_out, rynth.VideoOutputTuple)
    assert a_out.clip is a
    assert b_out.clip is b


def test_get_output_missing_raises_key_error() -> None:
    with pytest.raises(KeyError):
        rynth.get_output(0)


def test_get_outputs_is_read_only_mapping() -> None:
    clip = rynth.core.std.BlankClip(width=16, height=16, length=1)
    clip.set_output(0)
    clip.set_output(3)

    outputs = rynth.get_outputs()
    assert set(outputs) == {0, 3}
    assert outputs[0].clip is clip
    with pytest.raises(TypeError):
        outputs[7] = outputs[0]


def test_clear_output_and_clear_outputs() -> None:
    clip = rynth.core.std.BlankClip(width=16, height=16, length=1)
    clip.set_output(0)
    clip.set_output(1)

    rynth.clear_output(0)
    assert set(rynth.get_outputs()) == {1}
    # Clearing a missing index is a no-op.
    rynth.clear_output(42)

    rynth.clear_outputs()
    assert len(rynth.get_outputs()) == 0


def test_set_output_rejects_mismatched_alpha_dimensions() -> None:
    main = rynth.core.std.BlankClip(width=64, height=64, length=3)
    alpha = rynth.core.std.BlankClip(width=32, height=64, length=3)
    with pytest.raises(RuntimeError, match="dimensions must match"):
        main.set_output(0, alpha=alpha)


def test_set_output_rejects_mismatched_alpha_length() -> None:
    main = rynth.core.std.BlankClip(width=64, height=64, length=3)
    alpha = rynth.core.std.BlankClip(width=64, height=64, length=5)
    with pytest.raises(RuntimeError, match="length must match"):
        main.set_output(0, alpha=alpha)


def test_set_output_rejects_non_gray_alpha() -> None:
    main = rynth.core.std.BlankClip(width=64, height=64, length=3)
    alpha = rynth.core.std.BlankClip(width=64, height=64, length=3)
    with pytest.raises(RuntimeError, match="format must match"):
        main.set_output(0, alpha=alpha)


def test_set_output_accepts_matching_gray_alpha() -> None:
    main = rynth.core.std.BlankClip(width=64, height=64, length=3)
    alpha = rynth.core.std.ShufflePlanes(clips=main, planes=0, colorfamily=1)
    assert alpha.format is not None
    assert alpha.format.name == "Gray8"

    main.set_output(0, alpha=alpha)
    out = rynth.get_output(0)
    assert isinstance(out, rynth.VideoOutputTuple)
    assert out.clip is main
    assert out.alpha is alpha


def test_default_environment_is_single() -> None:
    env = rynth.get_current_environment()
    assert env.single is True
    assert env.alive is True
    assert env.active is True
    assert env.env_id == -1
    assert repr(env) == "<Environment (default)>"


def test_environment_data_is_opaque_and_weak_referenceable() -> None:
    import gc
    import weakref

    policy = _Policy()
    rynth.register_policy(policy)
    data = policy.api.create_environment()

    assert type(data) is rynth.EnvironmentData
    with pytest.raises(TypeError):
        rynth.EnvironmentData()
    with pytest.raises(AttributeError):
        _ = data.alive  # type: ignore[attr-defined]

    finalized = False

    def on_finalize(_reference: ReferenceType[rynth.EnvironmentData]) -> None:
        nonlocal finalized
        finalized = True

    reference: ReferenceType[rynth.EnvironmentData] = weakref.ref(data, on_finalize)
    policy.api.destroy_environment(data)
    del data
    gc.collect()

    assert reference() is None
    assert finalized


def test_wrap_environment_returns_handle_for_same_data() -> None:
    policy = _Policy()
    rynth.register_policy(policy)
    data = policy.api.create_environment()
    environment = policy.api.wrap_environment(data)

    assert type(environment) is rynth.Environment
    assert environment.alive
    assert not environment.active

    with environment.use():
        assert policy.current is data
        assert environment.active
        assert rynth.get_current_environment() == environment

    assert policy.current is None
    assert not environment.active


def test_wrap_environment_does_not_check_liveness() -> None:
    policy = _Policy()
    rynth.register_policy(policy)
    data = policy.api.create_environment()
    policy.api.destroy_environment(data)

    environment = policy.api.wrap_environment(data)
    assert type(environment) is rynth.Environment
    assert not environment.alive


def test_wrap_environment_rejects_non_environment_data() -> None:
    policy = _Policy()
    rynth.register_policy(policy)

    with pytest.raises(TypeError):
        policy.api.wrap_environment(object())  # type: ignore[arg-type]


def test_wrap_environment_rejects_inactive_policy_api() -> None:
    first = _Policy()
    rynth.register_policy(first)
    data = first.api.create_environment()
    rynth.clear_policy()

    second = _Policy()
    rynth.register_policy(second)
    with pytest.raises(ValueError, match="currently activated policy does not match"):
        first.api.wrap_environment(data)


def test_environment_policy_api_does_not_keep_policy_alive() -> None:
    import gc
    import weakref

    policy = _Policy()
    rynth.register_policy(policy)
    api = policy.api
    data = policy.api.create_environment()
    reference: ReferenceType[_Policy] = weakref.ref(policy)

    rynth.clear_policy()
    del policy
    gc.collect()

    assert reference() is None
    with pytest.raises(ValueError, match="currently activated policy does not match"):
        api.wrap_environment(data)


def test_register_policy_twice_raises() -> None:
    rynth.register_policy(_Policy())
    assert rynth.has_policy()
    with pytest.raises(RuntimeError, match="already a policy"):
        rynth.register_policy(_Policy())


def test_custom_policy_isolates_environments() -> None:
    policy = _Policy()
    rynth.register_policy(policy)

    first = policy.new_environment()
    second = policy.new_environment()

    policy.set_environment(first)
    first_env = rynth.get_current_environment()
    assert first_env.single is False
    assert first_env.env_id != -1
    assert first_env.active is True

    first_core = rynth.core.core
    rynth.core.std.BlankClip(width=16, height=16, length=1).set_output(0)

    # A different environment has its own core and empty output registry.
    policy.set_environment(second)
    assert rynth.core.core is not first_core
    assert len(rynth.get_outputs()) == 0
    assert first_env.active is False

    # `use()` temporarily reactivates the first environment.
    with first_env.use():
        assert first_env.active is True
        assert 0 in rynth.get_outputs()
    assert first_env.active is False

    # Clearing the policy tears the environments down.
    rynth.clear_policy()
    assert not rynth.has_policy()
    assert first_env.alive is False
