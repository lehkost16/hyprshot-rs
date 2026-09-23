# Hyshot Architecture

Hyshot is one application. The root binary owns CLI dispatch, configuration,
screen capture, session lifecycle, and external-process execution. The editor is
an internal library, not a separate Annotator process.

## Root application

- `src/app.rs`: parses CLI intent and dispatches commands.
- `src/workflow.rs`: selected-region acquisition and routing to editor or an
  external tool.
- `src/external.rs`: creates a temporary PNG, substitutes external-tool
  placeholders, captures output, and handles clipboard and notifications.
- `src/config.rs` and `src/config_cmds.rs`: TOML ownership, defaults, and
  configuration commands.
- `src/capture_session.rs`: atomic recording/longshot session state and process
  lifecycle.
- `src/record/`: region recording and finalization.
- `src/longshot/`: scrolling-capture orchestration, decoding, matching, canvas,
  and overlay.

## Shared image and editor libraries

- `crates/core`: immutable original-pixel image documents and logical geometry.
- `crates/editor`: annotation tools, undo/redo, image export, Wayland surfaces,
  GPU rendering, and editor preferences.
- `src/editor_state.rs`: persists remembered per-tool styles independently from
  explicit `config.toml` preferences.

## Data boundaries

Normal capture can use its configured image format. Editor and external-tool
workflows always capture PNG so pixel data and external input are predictable.
`ImageDocument` keeps original RGBA pixels immutable while annotations and zoom
remain editor state.

Longshot decodes its recording twice: first for bounded fixed-header/footer
sampling, then for incremental matching. Source frames are limited to 64 MiB,
the RGB canvas to 128 MiB, and editor input to 256 MiB decoded RGBA. These are
explicit failure limits rather than silent resizing.

## Verification

```bash
cargo fmt --check
cargo test --workspace --offline
cargo build --release --offline
```

Interactive Wayland verification remains necessary for compositor scaling,
clipboard ownership, recording completion, and external-tool network behavior.
