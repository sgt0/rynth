import asyncio

import numpy as np
import pytest
import rynth

# <https://github.com/vapoursynth/vapoursynth/blob/R79/include/VapourSynth4.h#L38>
SAMPLES_PER_FRAME = 3072


@pytest.fixture(scope="module")
def core() -> rynth.Core:
    return rynth.Core()


def test_audio_node_props(core: rynth.Core) -> None:
    clip = core.std.BlankAudio(length=SAMPLES_PER_FRAME * 10)
    assert isinstance(clip, rynth.AudioNode)
    assert clip.sample_rate > 0
    assert clip.num_channels >= 1
    assert clip.channel_layout != 0
    assert clip.num_samples == SAMPLES_PER_FRAME * 10
    assert clip.num_frames == 10
    assert len(clip) == 10
    assert clip.bytes_per_sample * 8 >= clip.bits_per_sample


def test_audio_frame_is_raw_frame(core: rynth.Core) -> None:
    clip = core.std.BlankAudio(length=SAMPLES_PER_FRAME)
    frame = clip.get_frame(0)

    assert isinstance(frame, rynth.AudioFrame)
    assert issubclass(type(frame), rynth.RawFrame)
    assert frame.readonly
    assert not frame.closed
    assert len(frame) == clip.num_channels
    assert frame.num_channels == clip.num_channels
    assert frame.sample_type == clip.sample_type
    assert frame.get_read_ptr(0) != 0
    with pytest.raises(RuntimeError, match="writable frames"):
        frame.get_write_ptr(0)


def test_audio_frame_channel_zero_copy(core: rynth.Core) -> None:
    clip = core.std.BlankAudio(length=SAMPLES_PER_FRAME)
    frame = clip.get_frame(0)
    channel = np.asarray(frame[0])

    assert channel.ndim == 1
    assert not channel.flags.owndata
    assert not channel.flags.writeable
    assert (channel == 0).all()
    # Negative index addresses the last channel.
    assert bytes(frame[-1]) == bytes(frame[clip.num_channels - 1])
    with pytest.raises(IndexError):
        _ = frame[clip.num_channels]


def test_audio_frame_context_manager_closes(core: rynth.Core) -> None:
    frame = core.std.BlankAudio(length=SAMPLES_PER_FRAME).get_frame(0)

    with frame as entered:
        assert entered is frame
        assert not frame.closed

    assert frame.closed
    with pytest.raises(RuntimeError, match="already been released"):
        _ = frame.props


def test_audio_frames_iterator(core: rynth.Core) -> None:
    clip = core.std.BlankAudio(length=SAMPLES_PER_FRAME * 5)
    frames = list(clip.frames())
    assert len(frames) == 5
    assert all(isinstance(f, rynth.AudioFrame) for f in frames)


def test_audio_get_frame_async(core: rynth.Core) -> None:
    clip = core.std.BlankAudio(length=SAMPLES_PER_FRAME * 4)

    async def main() -> None:
        frame = await clip.get_frame_async(0)
        assert isinstance(frame, rynth.AudioFrame)
        frames = await asyncio.gather(*(clip.get_frame_async(n) for n in [3, 0, 1]))
        assert all(isinstance(f, rynth.AudioFrame) for f in frames)

    asyncio.run(main())


def test_audio_set_output_registers_node(core: rynth.Core) -> None:
    rynth.clear_outputs()
    clip = core.std.BlankAudio(length=SAMPLES_PER_FRAME)
    clip.set_output(5)

    assert rynth.get_output(5) is clip
    assert rynth.get_outputs()[5] is clip
    rynth.clear_outputs()
