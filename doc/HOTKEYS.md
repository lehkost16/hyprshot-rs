# Hyshot Hotkeys

Hyshot does not install keybindings automatically. Add bindings to your Hyprland
configuration, then reload the compositor.

```conf
bind = SUPER, Print, exec, hyshot now
bind = SUPER SHIFT, Print, exec, hyshot area
bind = SUPER SHIFT, S, exec, hyshot annotate
bind = SUPER CTRL, S, exec, hyshot edit
bind = SUPER, T, exec, hyshot external ocr
bind = SUPER SHIFT, T, exec, hyshot external translate
bind = SUPER SHIFT, L, exec, hyshot longshot --edit
bind = SUPER, R, exec, hyshot record
```

```bash
hyprctl reload config-only
```

`hyshot --print-binds` prints a smaller generic set of Hyprland bindings. The
external names must exist in `~/.config/hyshot/config.toml` before their hotkeys
are useful.
