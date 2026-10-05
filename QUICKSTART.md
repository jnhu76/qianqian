# Qianqian — Quick Start

Qianqian (千千) is a local Windows music player. It plays music from
files and folders on your computer. It does not need a media library, a
database, or an account — point it at a folder and listen.

Qianqian runs in a terminal window. Every control is visible on screen —
core playback never requires memorizing a key. The keys below are
shortcuts for the same controls; the one worth knowing is that `Enter`
plays the selected playlist row.

## Start

Double-click:

```
qianqian.exe
```

or open PowerShell / Command Prompt / Windows Terminal and run:

```
qianqian.exe
```

The player opens with no music loaded. Press the visible `Open` button
(or the `O` key): a picker opens showing the current folder. Browse the
listing with the mouse or the arrow keys — the `[Enter folder]` button
(or `Enter` on a selected folder) descends into it, the `..` row moves
up, and `Enter` on a file row just marks that file as the selection.
Typing or pasting a path (for example `D:\Music`) into the path line
and pressing `Enter` selects that file or folder in the listing.

The picker commits exactly one subject: the selected row, or — once you
type into the path line, which clears the selection — the typed path.
The visible `[Open]` button plays that subject now (a file goes
straight to Now Playing; a folder seeds the track list with its
playable tracks), and `[Add to Playlist]` appends it to the track list
without playing it. If a path cannot be read, the picker stays open
with the diagnostic so you can correct it and retry.

## Play a folder

```
qianqian.exe play "D:\Music"
```

To start in shuffle order:

```
qianqian.exe play --shuffle "D:\Music"
```

You can name several files and folders in one command:

```
qianqian.exe play "D:\Music\song.flac" "E:\More Music"
```

The folder is scanned (including subfolders), the playable tracks become
the track list, and the first playable track starts. You will briefly
see a `scanning <path> ...` line while a large folder is checked.

## The screen

```
Now Playing              Playlist                 Audio                    Visualizer
┌ Qianqian Reference Player ────────────────────────┐
│ Source: D:\Music\夜曲.flac                        │
│ Format: 44100 Hz, 2 channels, mask 0x3            │
│ DSP (desired): off (bypass)                       │
│ Position: 01:42 / 03:58                           │
│ Terminal: pending   Stop requested: false ...     │
│ Track: 1/3                                        │
└───────────────────────────────────────────────────┘
01:42 ━━━━━━━━━━━╸──────────── 03:58
[ Back 5s ]                    [ Forward 5s ]
┌────────┐┌────────┐┌────────┐┌────────┐┌────────┐
│  Open  ││ ◀ Prev ││ Pause  ││ ■ Stop ││ Next ▶ │
└────────┘└────────┘└────────┘└────────┘└────────┘
┌───┐ 100/100 (desired) ┌───┐┌───────────────────┐┌──────────────┐
│ - │                   │ + ││ Order: Sequential ││ Repeat: Off  │
└───┘                   └───┘└───────────────────┘└──────────────┘
```

The `Playlist` tab shows the track list. Every button above is clickable
with the mouse and reachable with `Tab` + `Enter`; the
`[ Back 5s ]` / `[ Forward 5s ]` buttons seek in small steps (they are
visible while a track reports its position), the progress bar is
click-to-seek, and the `-` / `+` buttons change Qianqian's own volume.
The middle transport button is contextual — `Pause` while a track is
active, `Resume` while paused, `Play` with nothing active.

Two markers matter in the list:

- `▶` marks the track that is currently playing.
- `>` marks the row you have selected with Up / Down (with the list
  focused: `Tab` until the list is, or a mouse click on a row).

One row can carry both. The small `sel 2/3` counter in the list's title
is your selection; the `Track: 1/3` line below is the playing track.

**Up / Down only move the selection. They do NOT change what plays.
Enter plays the selected row.** This is the one distinction worth
remembering: browse all you like — nothing changes until you press
Enter.

## Keys

| Key                 | Action                                            |
| ------------------- | ------------------------------------------------- |
| `Tab / Shift+Tab`   | Move keyboard focus to the next / previous control |
| `Enter`             | Activate the focused control; on the playlist, play the selected row; in the picker, navigate or select (the visible buttons commit) |
| `Mouse`             | Click any visible control: tabs, buttons, the progress bar, playlist rows, picker rows |
| `↑ / ↓`             | Select previous / next playlist row (list focused) |
| `N / P`             | Next / previous track                             |
| `R`                 | Order: Sequential / Shuffle                       |
| `L`                 | Repeat: Off / All / One                           |
| `Space`             | Pause / resume                                    |
| `← / →`             | Seek 5 seconds back / forward                     |
| `Shift+← / Shift+→` | Seek 30 seconds back / forward                    |
| `G`                 | Go to an exact position you type                  |
| `+ / -`             | Volume up / down (steps of 5, range 0–100)        |
| `S`                 | Stop                                              |
| `O`                 | Open a file or folder                             |
| `?`                 | Help overlay                                      |
| `Esc`               | Cancel the current input / close the help         |
| `Q`                 | Quit                                              |
| `Ctrl+C`            | Quit (works everywhere)                           |

For `G`, type a position like `1:35` (minutes:seconds) or `95`
(seconds), then press Enter.

### Typing a path: Q is a normal character

While you are typing into the `Open:` line, every character — including
`q` and `Q` — is part of the path. That is what makes paths like
`Q:\Music` typeable. To quit from inside the input line, use `Ctrl+C`.

## Order: Sequential and Shuffle

- **Sequential** plays the tracks in the order the list shows them.
- **Shuffle** builds one shuffled order of the whole list and walks it:
  Next / Previous move through that same order, and the list pane shows
  exactly the order that will play. It is not a fresh random pick every
  time you press Next.

`qianqian.exe play --shuffle "D:\Music"` starts in shuffle order; the
`R` key switches at any time without restarting the current track.

## Repeat

- **Repeat Off** — when the last track of the list finishes, playback
  stops there.
- **Repeat All** — when the last track finishes, the list starts another
  pass (under Shuffle, a freshly shuffled pass).
- **Repeat One** — when a track finishes naturally, that same track
  plays again. Manual `N` / `P` still move to other tracks normally.

## What happens with bad files

When you open a folder, Qianqian checks every candidate file and keeps
the playable ones. You may see a summary like:

```
opened D:\Music\a-song.flac (347 candidates, 12 skipped, 2 unplayable)
not playable: broken take.flac
```

- Ordinary non-audio files (cover art, lyrics, notes) are skipped
  quietly and counted as `skipped`.
- Files that look like audio but cannot be opened are counted as
  `unplayable` and listed (a few names, then `(+N more)`).
- The same path passed twice is kept once.

If a track fails **while it is playing** (for example the file vanished
after the scan), the player shows `Terminal: Failed` and the failure
diagnostic. Qianqian does **not** silently skip failed tracks in this
release: press `N` or select another row and press `Enter`.

## Supported audio files

Qianqian ships with a built-in decoder. Verified formats:

- MP3 (`.mp3`)
- FLAC (`.flac`)
- AAC and ALAC in M4A / MP4 (`.m4a`, `.m4b`)
- AAC in ADTS (`.aac`)
- Ogg Vorbis (`.ogg`, `.oga`)
- Ogg Opus (`.opus`, `.ogg`)
- PCM WAV (`.wav`)

Other common audio extensions are checked too, and files the decoder
cannot open are reported as unplayable. No format is converted, and no
claim is made beyond the list above.

## Volume

The visible `-` / `+` buttons (or the `+` and `-` keys) change
Qianqian's own volume in steps of 5 (0–100). The scale is perceptual:
each step is about 3 dB, so a press feels alike near the top and near
the bottom (100 is full, 0 is silence). The number between the buttons,
like `75/100 (desired)`, is the player's own setting — Windows' own
mixer and output device still apply on top of it.

## Troubleshooting

### Player opens but no music is loaded

Press the `Open` button (or the `O` key) and paste a file or folder
path, or browse to it in the picker, then press the visible `Open`
button.

### A folder says "no playable audio files found"

Check that the folder contains files in the supported formats above and
that Qianqian can read the folder. If the message counts `N unplayable`,
those files were checked and refused by the decoder.

### A track fails during playback

The player does not silently skip failed tracks in this release. Press
`N` or select another track and press `Enter`.

### No sound

Check the usual suspects, in this order: the Windows output device, the
system volume mixer, Qianqian's own `Volume:` setting, and whether
another audio application on the same device works.

### The terminal looks wrong after a crash

Close and reopen the terminal window. If a key appears stuck, press
`Esc` once.

## What Qianqian does not do (yet)

- No media library, database, favorites, ratings or play history.
- The track list lives only as long as the player runs — nothing is
  saved.
- No playlist files (M3U or others), no import/export.
- No lyrics, cover art, spectrum or skins.
- No gapless playback claim: a short gap between tracks is normal.
- Windows only, x86_64.

## Quit

Press `Q` (or `Ctrl+C`). The terminal is restored and the window closes
normally when you launched it by double-click.

---

Qianqian is local-first software: it plays your files and does not talk
to the network.
