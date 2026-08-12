## Initial Commit

Decided to start with Tyscript + Playright.
Reason: 

- Rust's CDP clients work. chromiumoxide is real. But the honest assessment from people who've done this comparison: Rust gives roughly a 5–10% improvement at best for browser automation tasks — the Chrome/Chromium binary is the same in both cases, and neither chromiumoxide nor headless_chrome is as mature as Playwright or Puppeteer.

```
┌─────────────────────────────-┐       ┌──────────────────────────-┐
│ TypeScript + Playwright      │       │  Rust worker              │
│ • join / lobby / roster      │─────► │  • decode PCM/I420        │
│ • inject WebRTC hook (JS)    │ IPC   │  • per-participant mux    │
│ • participant events         │ (UDS  │  • encode, chunk, upload  │
│                              │ or    │  • STT streaming          │
└─────────────────────────────-┘ shmem)└──────────────────────────-┘
```



### Trial 1:

1. Implementing the first prototype.

Results: 

1. Google meet seems to be bit more complicated due to lack of support/documentation for bots. Hence gonna start with zoom bot then slowly move to google meet.



### Trial 2:

- Implementing zoom bot.



### Trial 3:

- Research FFmpeg

```
                   Google Meet
                         │
                         │ WebRTC
                         ↓
              ┌─────────────────────┐
              │   Meet Media API    │
              └─────────────────────┘
                         │
                  RTP audio/video
                         │
                         ↓
              ┌─────────────────────┐
              │      Rust App       │
              │                     │
              │  WebRTC client      │
              │  Media processing   │
              │  Session manager    │
              └─────────────────────┘
                         │
                  raw/decoded media
                         │
                         ↓
                     FFmpeg
                         │
                         ↓
                    recording
```

**Learnings**: 

- Won't work as all the participants need to be a part of google developer platform

### Trial 4:

```
Architecture

  -─────────────────────────────────────────┐
   Real Chrome (persistent, pre-authed      │
 │ profile; --user-data-dir + CDP port)     │
 │                                          │
    meet.google.com                         │
 │   └─ injected hook (pre-navigation)      │
 │       • wraps RTCPeerConnection          │
 │       • holds the 3 audio RTCRtpReceivers│
 │       • polls getContributingSources()   │
 │         @ ~20Hz → attribution events     │
 │       • AudioWorklet per slot → PCM      │
 │       • MutationObserver → roster/names  │
 └───────────────┬──────────────────────────┘
                 │ WebSocket (localhost)
                 │ binary PCM + JSON control
                 ▼
 ┌──────────────────────────────────────────┐
 │ Rust                                     │
 │  • ingest: framed WS, backpressure       │
 │  • attribution timeline (slot,csrc,t)    │
 │  • demux 3 slot streams → N participants │
 │  • FFmpeg subprocess per track (stdin)   │
 │  • STT streaming, storage, summarize     │
 └──────────────────────────────────────────┘
```

