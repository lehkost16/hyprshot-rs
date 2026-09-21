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
- `src/config.rs`: one configuration owner, including `[editor]`; writes use
  same-directory temporary files and atomic replacement.

Recorders remain OS processes because recording outlives the invoking CLI.
Editing runs in-process on the main thread. PID-based recording state still
lacks process-start identity and concurrent-toggle locking; shared lifecycle
management does not by itself solve those remaining risks.

## Editor

`crates/editor` exposes input, settings, options and its entry point.

- `lib.rs`: input decoding before Wayland initialization.
- `config.rs`: serializable preferences shared in memory, with no file I/O.
- `annotator.rs` and `annotator/`: tools, selection and undo/redo.
- `ui/`: image panel, toolbars, layout, icons and system fonts.
- `platform/`: Wayland surfaces, input, scaling and GPU/window lifecycle.
- `export.rs`: shared save/copy path for shortcuts and buttons; close only after
  success. `image_save.rs` creates collision-safe lossless PNG output.

The editor returns preferences on normal exit. The host reloads the latest
configuration, updates the typed editor section and saves. No per-frame disk
access; `--no-config` does not save preferences. A crash before normal exit loses
session changes. The old independent annotator config is not auto-migrated.

The Wayland app ID remains `site.nullable.annotator` to retain compositor rules;
this does not introduce a separate executable or configuration dependency.

## Validation

```sh
cargo fmt --all --check
cargo test --workspace --offline
cargo build --workspace --offline
```

Tests cover input pixel preservation, settings serialization, atomic config
writes, export destinations, startup cleanup and existing toolbar/scaling/
stitching contracts. Multi-monitor interaction, clipboard ownership and real
recording completion still require interactive Wayland verification.
