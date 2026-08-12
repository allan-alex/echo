# Architecture Decision — Real-Time Google Meet Capture

**Date:** 2026-08-10
**Status:** Accepted. Supersedes the TypeScript + Playwright decision in `init.md`.

## What was ruled out, and why

**Meet Media API — blocked.** It requires the Cloud project, the OAuth principal, *and
every participant in the conference* to be enrolled in the Workspace Developer Preview
Program. It also refuses encrypted meetings, watermarked meetings, and any call with an
underage account. Enrolling every participant is not achievable, so this is not a
shippable path regardless of language or effort.

**Meet REST API artifacts — ruled out by requirements.** `conferenceRecords.recordings`
and `.transcripts` are GA, sanctioned, and give speaker-attributed transcripts with
timestamps plus an MP4 in Drive. This would have been the cheap path, but it is
**post-meeting only** and echo requires real-time.

**Consequence:** Google offers no sanctioned real-time media path for Meet. Therefore
the bot joins in **real Chrome**, because Chrome is the only WebRTC stack Google will
talk to without preview enrollment.

## The finding that shapes everything

Meet does **not** send one audio track per participant. Its SFU forwards only the ~3
loudest speakers over **exactly three** audio tracks, reassigning which participant sits
in which slot as speaker activity changes:

> *"If any of the original streams in the conference are no longer one of the loudest
> streams, Meet switches the RTP packets that make up the SSRC to the loudest."*

Per-participant audio is not received. It is **reconstructed**. Slot→participant
attribution is therefore the core engineering problem of this project, not FFmpeg.

This cap is identical under the Meet Media API, so nothing is lost by taking the browser
path.

### The attribution chain

```
RTCRtpReceiver.getContributingSources()
        │
        ├─ source     → CSRC integer, stable per participant for the session
        ├─ audioLevel → 0.0 .. 1.0
        └─ timestamp  → up to 10s of history (freshness gate required)
        │
        ▼
   CSRC.toString()  ==  streamId  in Meet's `collections` data channel protobuf
        │
        ▼
   streamId → deviceId → fullName
```

Verified against `attendee-labs/attendee`, a production OSS meeting bot, which does
exactly `getUserByStreamId(source.source.toString())`.

**Robustness split.** CSRC and slot audio energy are standard WebRTC and essentially
never break. The `collections` protobuf field numbers and the `captions` channel are
reverse-engineered and are the realistic breakage. So the fragile part is the
**CSRC→name map, not the slot→CSRC map** — meaning total protobuf breakage still leaves
perfect *diarization* and costs only *names*. That graceful degradation is the main
reason this design is defensible against Meet UI churn.

## Architecture

```
┌──────────────────────────────────────────┐
│ Real Chrome                              │
│ persistent, manually-authed profile      │
│ --user-data-dir + --remote-debugging-port│
│                                          │
│  meet.google.com                         │
│   └─ injected hook (pre-navigation)      │
│       • wraps RTCPeerConnection          │
│       • holds the 3 audio receivers      │
│       • getContributingSources() @ 20Hz  │
│       • MediaStreamTrackProcessor → PCM  │
│       • taps `collections` + `captions`  │
│                                          │
│      HOOK, TAG, SHIP — nothing more      │
└───────────────┬──────────────────────────┘
                │ WebSocket (127.0.0.1, binary)
                ▼
┌──────────────────────────────────────────┐
│ Rust                                     │
│  • ingest + JSONL event log (replayable) │
│  • attribution engine  ← all the tests   │
│  • demux 3 slots → N participants        │
│  • FFmpeg subprocess per track           │
│  • STT, storage, summarization           │
└──────────────────────────────────────────┘
```

**Attribution lives in Rust, not in the injected JS.** The injected script is the part
Google breaks, so it stays dumb and small. Attribution is the risky, iterate-heavy part,
so it belongs where it gets unit tests, a type system, and offline replay against
recorded JSONL.

## Reversing the `init.md` decision on Rust vs Playwright

`init.md` chose Playwright over Rust CDP because chromiumoxide is less mature. That was
correct **for heavy browser automation**. It does not apply here: everything that matters
happens inside injected JS, which is identical in both worlds. The remaining automation
surface is six operations — attach, inject, set display name, click Join, poll for
admission, click Leave.

Playwright's value (selector engine, auto-waiting, tracing, cross-browser) buys nothing
here, while a TS orchestrator costs a second runtime, a second dependency tree, an extra
IPC hop, and a bifurcated error model. **Decision: Rust + chromiumoxide, no TypeScript.**

Do **not** use `chromiumoxide::Browser::launch` — it injects its own default flags.
Spawn Chrome with `std::process::Command` so the flag vector is fully owned, then
`Browser::connect`. CDP attachment alone does not set `navigator.webdriver`; that comes
from the WebDriver path and `--enable-automation`, which Puppeteer adds by default.
Owning the flags is a genuine advantage of raw CDP over any framework.

`--user-data-dir` must be **unique per session** and gated on `DevToolsActivePort`
appearing — Chrome silently ignores `--remote-debugging-port` if it merges into a running
instance.

## On the login problem from `jul30_2026.md`

Those notes correctly found that Google fingerprints and blocks Playwright/Puppeteer.
Crucially, that is a **login** problem, not a **join** problem.

**We never automate the Google login form.** The profile is authenticated by hand once
and reused, which sidesteps the entire detection surface. Ship an `echo login` subcommand
that opens the profile headful for a human. Detect session expiry at join (redirect to
`accounts.google.com`) and fail with a clear message.

**Hard-won detail:** the login browser must be launched **without** `--remote-debugging-port`.
Google refuses to sign in to a CDP-enabled browser — *"Couldn't sign you in / This browser
or app may not be secure"* — because any local process could attach to that port and read
live credentials. The check runs at **sign-in**, not at join, so authenticating portless
leaves a session cookie the CDP runs reuse. `echo login` and `echo run` therefore need
different flag vectors, which is another reason to own the flags rather than let a
framework supply them.

**Guest join is a real fallback worth keeping.** Meet permits unauthenticated "Ask to
join" with a typed display name when the host allows it, which needs no Google account
and no login flow at all. Many orgs disable it, so it cannot be the only path — but it
removes the credential problem entirely for testing, and for any meeting that permits it.

## Other decisions

| Decision | Choice | Why |
|---|---|---|
| Audio in page | `MediaStreamTrackProcessor` → Int16 PCM 48kHz | 3 × 96 KB/s over loopback is nothing. Int16 is what STT wants. `AudioData.timestamp` is the sync clock. |
| Not MediaRecorder | — | It re-encodes (track is already decoded PCM), and a *slot's* stream is not a *participant's* stream — you must cut at handoff boundaries, which you cannot do to a WebM cluster without re-encoding. The `-c copy` win is illusory for this data. |
| Video | msid lookup, not slot inference | Meet sets the video MediaStream id to the numeric stream id, so `streams[0].id` at `ontrack` resolves identity directly. Video's real problem is different: Meet only sends video for tiles it thinks are rendered. |
| Transport | WebSocket to 127.0.0.1 | Binary frames, lifecycle independent of CDP, and `ws.bufferedAmount` as a real backpressure signal. `Runtime.addBinding` is string-only (+33% base64), shares the CDP session, and has no backpressure. |
| FFmpeg | Subprocess + stdin pipes | Fault isolation, a pasteable repro command, one-string encoder swaps. `ffmpeg-next` means bindgen + libclang + an ABI that churns, for a large `unsafe` surface. |

**Subprocess correctness requirements:** always drain stderr on a dedicated task (a full
stderr pipe deadlocks FFmpeg mid-recording); pass `-nostdin`; shut down by closing stdin
and awaiting exit, never `SIGKILL` (MP4 needs its moov atom); use
`-movflags +frag_keyframe+empty_moov+default_base_moof` so a killed process still leaves
a playable file.

## Crate layout

```
echo/
├── crates/                  # platform-neutral — reusable for a future Zoom bot
│   ├── echo-core/           # domain types, trait MeetingPlatform, zero I/O
│   ├── echo-ingest/         # local WS server + binary wire codec
│   ├── echo-attribution/    # slot→participant timeline; pure, deterministic, all tests
│   ├── echo-media/          # FFmpeg driver, PCM ring buffers, boundary-aware segmenter
│   ├── echo-stt/            # trait SpeechToText + backends
│   ├── echo-store/          # SQLite + artifact layout
│   ├── echo-session/        # orchestration state machine
│   └── echo-cli/            # the binary
└── google-meet/             # Meet-specific
    ├── probe/               # M0 GO/NO-GO console probe (this exists now)
    ├── meet-browser/        # Chrome supervisor, CDP, join/lobby/leave
    ├── meet-inject/         # injected JS, bundled at build time
    ├── meet-proto/          # tolerant decoder for Meet's protobuf shapes
    └── meet-adapter/        # impl MeetingPlatform for Meet
```

Zoom's SDK gives per-participant audio directly, so `echo-attribution` simply is not in
Zoom's path — which is the correct test that the abstraction boundary is in the right
place. `PlatformEvent` must therefore carry `AttributedAudio { participant_id, pcm, ts }`,
with slot mechanics entirely internal to Meet.

## Attribution engine requirements

1. **Freshness gate.** Filter to entries newer than ~200ms before `argmax(audioLevel)`.
   `getContributingSources()` returns 10s of history; attendee gates only on
   `audioLevel >= 0.01` and therefore mis-attributes. Do not copy that.
2. **Hysteresis.** Require 2 consecutive agreeing ticks before committing an owner change.
3. **~300ms delay line.** A handoff is detected *after* the new speaker's first packets
   arrive, so without buffering you mis-assign the leading 50–150ms of every utterance —
   exactly the phonemes that hurt STT most. Apply owner changes retroactively.
4. **Boundary refinement.** Snap cuts to the nearest RMS minimum within ±80ms, 10ms fade.
   This is the correct, narrow use of computed energy — not for identity.
5. **Cross-slot conflict.** A participant owns at most one slot at any instant. On
   conflict the higher-recent-energy slot wins; the loser goes to a quarantine track.
   Silently duplicating audio into one participant's file is worse than dropping it.

## Validation strategy

Log every event to JSONL and make the engine replayable from day one.

Then use Meet's own `captions` channel (`{deviceId, text, isFinal}`) as **ground truth**:
for every caption interval attributed to device D, assert the engine had a slot owned by
D overlapping it. Emit `attribution_agreement_pct` as a first-class per-session metric.
This turns "did Google break us?" from a support ticket into a number, and gives a CI
gate — replay a recording, require >95%.

## Known limitations

- **More than 3 simultaneous speakers:** the 4th is not forwarded by the SFU. Unfixable
  from the browser. Detect it (captions attribute text to a device that never held a
  slot) and record it as a known gap rather than silently losing it. Recall.ai advertises
  16 concurrent speakers, which strongly suggests they are not using the web-client path.
- **Screenshare audio** is likely not captured in separate streams. Verify in M1.
- **Consent:** the bot is a visible participant. Give it an unambiguous display name
  ("Echo Notetaker"). Two-party-consent jurisdictions apply.
- **ToS:** automating a Google account for meeting attendance is a gray area. Use a
  dedicated, disposable bot account — never a personal one.

## Next step

Run `google-meet/probe/` M0. Nothing else gets built until it returns GO.
