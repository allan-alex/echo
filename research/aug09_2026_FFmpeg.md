## FFmpeg

FFmpeg is essentially a **toolkit for working with audio and video**.

It can:

- Read video files
- Read audio files
- Convert one video format to another
- Cut videos
- Extract audio from video
- Record audio
- Record your screen
- Compress video
- Combine audio + video
- Process live audio/video streams

**The main command-line programs are**

- ffmpeg ( processes media )
- ffmprob ( inspecting media )
- ffplay ( simple media player )

### Containers vs Codecs

Container has video and audio in it.

```
MP4 container
│
├── Video → H.264
│
└── Audio → AAC

┌─────────────────────────┐
│        movie.mp4        │
│                         │
│  ┌───────────────────┐  │
│  │ Video: H.264      │  │
│  └───────────────────┘  │
│                         │
│  ┌───────────────────┐  │
│  │ Audio: AAC        │  │
│  └───────────────────┘  │
│                         │
└─────────────────────────┘
```

Codecs is the encoding type of video and audio

```
# Common video codecs
H.264
H.265 / HEVC
AV1
VP9

# Common audio
AAC
MP3
Opus
FLAC

# Common containers
MP4
MKV
WebM
MOV
AVI
```

## What does FFmpeg actually do?

example for the below scenario 

```
Camera
   ↓
raw video
   ↓
H.264 encoder
   ↓
MP4 container
   ↓
video.mp4
```



ffmpeg can:

```
Camera
   ↓
FFmpeg
   ↓
H.264
   ↓
MP4

------------------------ OR ------------------------

video.mp4
   ↓
FFmpeg
   ↓
extract audio
   ↓
audio.wav

------------------------ OR ------------------------

video.mp4
   ↓
FFmpeg
   ↓
resize
   ↓
720p.mp4
```



##  Basic Commands 

```
ffmpeg -i input.mp4 output.mp4
```

```
ffmpeg -i input.mkv output.mp4 # ( Converting a video )
```



Here FFmpeg may need to **re-encode** the video.  
That's potentially expensive

There are actually two different operations

### Remuxing

**Change the container without changing the encoded media.**

```
MKV
 ↓
H.264 + AAC
 ↓
MP4
```

The H.264 and AAC streams stay the same. This is fast.



### Transcoding

**Actually decode and encode again.**

```
H.264
 ↓
decode
 ↓
raw video
 ↓
encode
 ↓
H.265
```

This takes considerably more CPU/GPU time

***( When to use Transcoding or Remuxing ) ??***

***( How is Transcoding done ) ??***



-c is extremely important.

```
ffmpeg -i input.mkv -c copy output.mp4
```

Don't re-encode the streams. Just copy them. Basically this is Remuxing 

```
input.mkv
│
├── H.264 ──────────────┐
│                       │
└── AAC ────────────────┤
                        ↓
                     output.mp4
```



### Extracting Audio

Suppose meeting.mp4 contains Video ( H.264 ) and Audio ( AAC )

```
ffmpeg -i meeting.mp4 -vn audio.wav
```

-vn ( no video )



```
ffmpeg -i meeting.mp4 -an video.mp4
```

-an ( no audio )

## FFmpeg can also receive live data

```
Camera
   ↓
FFmpeg
   ↓
recording.mp4
```

```
Microphone
   ↓
FFmpeg
   ↓
audio.wav
```

```
Screen
   ↓
FFmpeg
   ↓
recording.mp4
```



### Why FFmpeg needs codecs?

Suppose FFmpeg receives raw video.  
Raw video is HUGE.

```
1920 × 1080
30 frames/sec
24 bits/pixel
```

That's an enormous amount of data.  
If you record raw video, your disk would fill very quickly.

```
Raw video
    ↓
H.264 encoder
    ↓
compressed video
```

```
Raw video

████████████████████████████████████████

          ↓ H.264
██████
```



## FFmpeg Arch

```
                 FFmpeg
                    │
        ┌───────────┼───────────┐
        ↓           ↓           ↓
     Demuxer     Decoder     Filter
        ↓           ↓           ↓
      streams     raw media   modified
        │           │           │
        └───────────┼───────────┘
                    ↓
                  Encoder
                    ↓
                  Muxer
                    ↓
               output file
```



### Demuxer

The **demuxer** opens the container and separates the streams

```
MP4
 │
 ↓
demuxer
 │
 ├── video stream
 │
 └── audio stream
```



### Decoder

Converts compressed data to raw streams

```
H.264
 ↓
decode
 ↓
raw video frames
```



### Filter

For manipulating raw streams.

```
1920 × 1080
     ↓
 resize
     ↓
1280 × 720

------------------------ OR ------------------------

video
 ↓
crop
 ↓
cropped video

------------------------ OR ------------------------

video
 ↓
brightness adjustment
 ↓
modified video
```



### Encoder

After processing encode it back.

```
raw video
    ↓
H.264 encoder
    ↓
compressed H.264
```



### Muxer

FFmpeg needs to put everything back into a container.

```
H.264 video
       +
AAC audio
       ↓
     muxer
       ↓
     MP4
```

```
                INPUT
                  │
                  ↓
              Container
                  │
                  ↓
               Demuxer
              ↙       ↘
         Video        Audio
           ↓            ↓
        Decoder      Decoder
           ↓            ↓
        Filters      Filters
           ↓            ↓
        Encoder      Encoder
              ↘      ↙
                Muxer
                  ↓
              MP4 file
```



### Command for selecting video/audio devices.

```
ffmpeg -f x11grab ...
```

or

```
ffmpeg -f pulse ...
```



The `-f` option specifies an **input/output format or device interface**.

Capture video from an X11 display.

```
Take video from Xvfb :99
             +
Take audio from PulseAudio
             ↓
           FFmpeg
             ↓
        output.mp4
```



