# Hotkeys Guide - hyshot

Minimal keybinding examples for supported compositors.

## Quick Start

For usage and flags, see [README.md](../README.md) and [CLI.md](CLI.md).

---

## Hyprland

Add to `~/.config/hypr/hyprland.conf`:

```conf
bind = SUPER, Print, exec, hyshot -m window
bind = SUPER SHIFT, Print, exec, hyshot -m region
bind = SUPER CTRL, Print, exec, hyshot -m output
bind = , Print, exec, hyshot -m output -m active
```

Reload Hyprland:
```bash
hyprctl reload
```

---

## Sway

Add to `~/.config/sway/config`:

```conf
bindsym $mod+Print exec hyshot -m window
bindsym $mod+Shift+Print exec hyshot -m region
bindsym $mod+Ctrl+Print exec hyshot -m output
bindsym Print exec hyshot -m output -m active
```

Reload Sway:
```bash
swaymsg reload
```
