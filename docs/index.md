# rynth

Python bindings for [VapourSynth](https://www.vapoursynth.com/).

## Installation

```
pip install rynth
```

## Usage

```python
import rynth as vs

core = vs.core

# Create a clip.
clip = core.std.BlankClip(width=1920, height=1080)

# Retrieve a single frame.
frame = clip.get_frame(0)
print(frame.format.name, frame.width, frame.height)

# Iterate all frames concurrently.
for frame in clip.frames():
    luma = frame[0]  # memoryview over luma plane.
```

Or go async:

```python
import asyncio
import rynth as vs

core = vs.core

async def main():
    clip = core.std.BlankClip(width=1920, height=1080, length=100)
    frame = await clip.get_frame_async(42)
    print(f"Frame 42: {frame.width}x{frame.height}")

asyncio.run(main())
```
