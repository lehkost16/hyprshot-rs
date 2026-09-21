# Annotator integration

Imported from /home/nana/Projects/personal/annotator at commit
4321d9b before integrating into hyshot. Original license: GPL-3.0;
see LICENSE. Original source: https://github.com/lehkost16/annotator.

The original repository remains independent. This code is now the hyshot-editor
library, exposing run(EditorInput, EditorOptions) and returning EditorSettings.
The host owns configuration persistence and output policy. platform/ contains
Wayland/GPU integration; ui/ contains layout and toolbars. Captured images arrive
in memory. Existing toolbar expand/collapse behavior and GPL attribution remain.
