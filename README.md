# echo

A program for extracting and processing audio ... for now.


To install and build the libraries
```
cargo build
```


To record audio.

```
cargo run --bin dual-capture
```

To process and get the number of speakers.

```
cargo run --bin diarize -- mic.wav
```

Output:

```
Input: sample_rate=48000 channels=1 format=Float bits=32
Prepared 43.9s of mono 16000 Hz audio
Loading diarization models (first run downloads them)...

== Speaker turns ==
      1.03s -     1.50s  SPEAKER_00
      1.74s -     4.86s  SPEAKER_00
      5.14s -     6.58s  SPEAKER_00
      6.98s -     7.73s  SPEAKER_00
      8.18s -    11.71s  SPEAKER_00
     12.18s -    12.89s  SPEAKER_00
     14.48s -    14.59s  SPEAKER_00
     15.34s -    15.39s  SPEAKER_00
     15.39s -    15.79s  SPEAKER_01
     17.07s -    25.06s  SPEAKER_01
     26.29s -    30.71s  SPEAKER_01
     31.20s -    31.86s  SPEAKER_01
     32.18s -    32.82s  SPEAKER_01
     33.07s -    34.73s  SPEAKER_01
     35.64s -    37.86s  SPEAKER_01
     39.05s -    43.16s  SPEAKER_01

== Speakers: 2 ==
  SPEAKER_00: 10.2s
  SPEAKER_01: 22.1s
```
