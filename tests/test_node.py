import asyncio
from fractions import Fraction

import numpy as np
import pytest
import rynth


@pytest.fixture(scope="module")
def core() -> rynth.Core:
    return rynth.Core()


def test_node_props(core: rynth.Core) -> None:
    clip = core.std.BlankClip(width=320, height=240, length=10, fpsnum=30, fpsden=1)
    assert clip.width == 320
    assert clip.height == 240
    assert clip.num_frames == 10
    assert len(clip) == 10
    assert clip.fps == Fraction(30, 1)


def test_get_frame_and_zero_copy(core: rynth.Core) -> None:
    clip = core.std.BlankClip(width=320, height=240, length=3, color=[235, 128, 128])
    frame = clip.get_frame(0)
    plane = frame[0]
    arr = np.asarray(plane)
    assert arr.shape == (240, 320)
    assert arr.dtype == np.uint8
    assert (arr == 235).all()
    assert not arr.flags.owndata
    assert not arr.flags.writeable


def test_invert(core: rynth.Core) -> None:
    clip = core.std.BlankClip(width=64, height=64, length=1, color=[0, 128, 128])
    inverted = core.std.Invert(clip=clip)
    orig = np.asarray(clip.get_frame(0)[0])
    inv = np.asarray(inverted.get_frame(0)[0])
    assert (orig == 0).all()
    assert (inv == 255).all()


def test_frames_iterator(core: rynth.Core) -> None:
    clip = core.std.BlankClip(width=16, height=16, length=5)
    frames = list(clip.frames())
    assert len(frames) == 5
    assert all(f.width == 16 for f in frames)


def test_frames_prefetch_preserves_order(core: rynth.Core) -> None:
    a = core.std.BlankClip(width=16, height=16, length=4, color=[10, 128, 128])
    b = core.std.BlankClip(width=16, height=16, length=4, color=[200, 128, 128])
    clip = core.std.Splice(clips=[a, b])
    values = [int(np.asarray(f[0])[0, 0]) for f in clip.frames(prefetch=3, backlog=4)]
    assert values == [10] * 4 + [200] * 4


def test_frames_partial_consumption(core: rynth.Core) -> None:
    clip = core.std.BlankClip(width=16, height=16, length=100)
    it = clip.frames()
    assert next(it).width == 16
    del it


def test_get_frame_async(core: rynth.Core) -> None:
    a = core.std.BlankClip(width=16, height=16, length=4, color=[10, 128, 128])
    b = core.std.BlankClip(width=16, height=16, length=4, color=[200, 128, 128])
    clip = core.std.Splice(clips=[a, b])

    async def main() -> None:
        frame = await clip.get_frame_async(0)
        assert int(np.asarray(frame[0])[0, 0]) == 10
        frames = await asyncio.gather(*(clip.get_frame_async(n) for n in [7, 0, 4]))
        values = [int(np.asarray(f[0])[0, 0]) for f in frames]
        assert values == [200, 10, 200]
        with pytest.raises(RuntimeError, match="Invalid frame number"):
            await clip.get_frame_async(9999)

    asyncio.run(main())


def test_frame_outlives_node_and_core() -> None:
    core = rynth.Core()
    arr = np.asarray(
        core.std.BlankClip(width=16, height=16, color=[42, 128, 128]).get_frame(0)[0]
    )
    del core
    assert (arr == 42).all()


def test_add(core: rynth.Core) -> None:
    a = core.std.BlankClip(width=64, height=64, length=7, color=[10, 128, 128])
    b = core.std.BlankClip(width=64, height=64, length=3, color=[200, 128, 128])
    c = a + b
    assert isinstance(c, rynth.VideoNode)
    assert c.num_frames == 10
    assert c.width == 64
    assert np.array(c.get_frame(0)[0])[0, 0] == 10
    assert np.array(c.get_frame(9)[0])[0, 0] == 200


def test_mul(core: rynth.Core) -> None:
    clip = core.std.BlankClip(width=64, height=64, length=5)
    assert (clip * 3).num_frames == 15
    with pytest.raises(ValueError, match="Loop count"):
        clip * 0
    with pytest.raises(TypeError, match="integer factors"):
        clip * "x"
