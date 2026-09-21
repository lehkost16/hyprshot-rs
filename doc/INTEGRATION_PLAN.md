# Planned integration

Design references: mark-shot commit 5a10ab0f3130b5a3ad63f93a9e8658873c8329db,
especially annotation_launch, capture_geometry, recording_session_manager,
recording_frame_queue and scroll/stitcher. Borrow ownership boundaries, not Qt,
multi-platform backends or automatic fallback chains.

Baselines: hyshot cf39e19, standalone annotator 4321d9b. The preliminary merge
is 4331db8; d91f0e6 preserves the experimental restructuring for comparison.

## Stages

- [x] 1. Shared logical geometry and image document contract; monitor scaling,
  rotation, negative origins and cross-output recording validation.
- [x] 2. Bounded longshot decoding and incremental stitching; explicit canvas
  limits, decoder failure handling and reproducible synthetic-video tests.
- [x] 3. Recording session ownership; concurrent-toggle locking, process identity,
  finalizing/failure states and recoverable output handling.
- [ ] 4. Separate explicit editor preferences from remembered tool styles. No
  implicit old-config migration. One host-owned state store and output policy.
- [ ] 5. Feed captures, files and stitched images through the document contract;
  preserve toolbar UX and validate ordinary capture/editor/export workflows.

Each stage is verified and committed separately. Unit tests are not desktop
acceptance. Do not install over the user's binaries until Wayland workflows are
verified. Large-image limits must reject explicitly, not silently resize output.

## Decisions

- Keep Rust/egui/Wayland and wl-screenrec; no mandatory tray daemon or new encoder.
- Original pixel dimensions and RGBA remain separate from view zoom.
- Only external OCR/editor adapters need temporary PNGs.
- Retain the independent top-right toolbar toggle and existing compositor app ID.
- Bounded input frame history does not make the resulting long image unbounded:
  both working frames and output canvas need separate limits.
- Real-time preview and recording pause are later features, not prerequisites for
  this structural integration. Do not promise pause by merely signalling a process.
