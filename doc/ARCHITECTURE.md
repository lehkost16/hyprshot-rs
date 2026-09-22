# Architecture

Hyshot is one application with an internal editor library, not two executables.

## Application

- `src/app.rs`: CLI/config dispatch and ordinary screenshot commands.
- `src/workflow.rs`: selection/freeze/capture lifecycle and routing to editing
  or OCR. Built-in editing receives PNG bytes without a temporary file.
- `src/compositor.rs`: monitor metadata shared by recording, scrolling capture
  and external-command placeholders; no dependency on editor/OCR execution.
- `src/external.rs`: explicitly configured external processes; temporary images
  remain alive until their commands finish. Failed OCR never copies stdout.
- `src/capture_session.rs`: recorder/overlay startup, atomic state publication,
  startup rollback and bounded graceful stop. A stop timeout retains state and
  prevents processing an unfinished video.
- `src/record/`: video-specific arguments and finalization.
- `src/longshot/`: scrolling-capture orchestration, matching and stitching.
- `crates/core`: logical rectangles and immutable original-pixel documents shared
  by capture and editing; no UI or compositor dependencies.
- `src/config.rs`: one configuration owner, including `[editor]`; writes use
  same-directory temporary files and atomic replacement.

Recorders remain OS processes because recording outlives the invoking CLI.
Editing runs in-process on the main thread. Recording and longshot each hold an
exclusive toggle/finalization lock under XDG_RUNTIME_DIR/hyshot. Process identities
include boot ID and start ticks; pidfds bind signals to the verified process.
Recording, Finalizing and Failed states keep failed conversions retryable without
signalling another process. No PID-only fallback is used. Final outputs refuse
overwrite; conversion failures retain the source and session state.

## Editor

`crates/editor` exposes input, settings, options and its entry point.

- `lib.rs`: input decoding before Wayland initialization.
- `config.rs`: serializable preferences shared in memory, with no file I/O.
- `annotator.rs` and `annotator/`: tools, selection and undo/redo.
- `ui/`: image panel, toolbars, layout, icons and system fonts.
- `platform/`: Wayland surfaces, input, scaling and GPU/window lifecycle.
- `export.rs`: shared save/copy path for shortcuts and buttons; close only after
  success. `image_save.rs` creates collision-safe lossless PNG output.

The editor returns tool styles on normal exit. `src/editor_state.rs` merges changed
styles into a separate editor-state.toml under a lock and atomically replaces it.
Explicit config.toml preferences are not rewritten. No per-frame disk access;
`--no-config` neither loads nor saves style memory. A crash before normal exit
loses session changes. The old independent annotator config is not auto-migrated.

The Wayland app ID remains `site.nullable.annotator` to retain compositor rules;
this does not introduce a separate executable or configuration dependency.

## Validation

Longshot uses two decoder passes: a five-frame luminance reservoir for fixed
header/footer detection, followed by incremental matching. It retains a reference
frame rather than the whole video. Input RGB frames are limited to 64 MiB and the
RGB canvas to 128 MiB; these are data limits, not a whole-process RSS guarantee.
Reallocation and decoder buffers still contribute to peak memory. Export consumes
the canvas without duplicating it. Decode errors, truncated frames and canvas
overflow are explicit errors; the source recording is retained for recovery.

`stitch` gets actual pixel dimensions from ffprobe. The old width/height/scale
overrides are removed rather than used to guess dimensions after probe failure.
Published longshot files must not already exist.

```sh
cargo fmt --all --check
cargo test --workspace --offline
cargo build --workspace --offline
```

Tests cover input pixel preservation, settings serialization, atomic config
writes, export destinations, startup cleanup and existing toolbar/scaling/
stitching contracts. Multi-monitor interaction, clipboard ownership and real
recording completion still require interactive Wayland verification.
