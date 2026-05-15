# abar

Minimalistic wayland-native bar, created with pango and cairo and inspired by ashell/waybar

## Icon theme

Custom modules use FreeDesktop icon names. abar reads the `XDG_ICON_THEME` environment variable
to select which theme to search (PNG preferred, SVG supported via the `svg` feature).

Set it in your shell profile or compositor environment:

```sh
export XDG_ICON_THEME=candy-icons
```

If `XDG_ICON_THEME` is unset, abar falls back to `hicolor`. Theme inheritance chains defined in
`index.theme` are **not** followed — set the theme that directly contains your icons.

Icons that cannot be resolved are displayed as text (the module name) with a warning logged.
