# Testing a Dropshade build

```sh
./install.sh                    # or ./install.sh --gnome-extension when not on GNOME right now
./uninstall.sh                  # what a user gets; ~/.config/dropshade stays
./prune.sh                      # back to a clean account for the next run
```

Dropshade needs a Wayland session: `echo $XDG_SESSION_TYPE` must print `wayland`.

## Every desktop

- [ ] `dropshade --version` prints the version; the app menu shows Dropshade with its icon.
- [ ] F12 drops the terminal down and hides it again; `dropshade toggle` does the same.
- [ ] New tab, split right and below, close a pane, rename a tab (Ctrl+Shift+T/R/D/W, F2).
- [ ] Quit Dropshade with tabs and splits open, start it again: the tabs come back.
- [ ] *Settings…* in Dropshade's menu opens; changing the width or height applies at once.
- [ ] `./uninstall.sh` removes `~/.local/bin/dropshade` and the menu entry.

## GNOME 51

- [ ] After `./install.sh`, log out and in: `gnome-extensions info dropshade@samuelba.dev` says
      ENABLED, without errors.
- [ ] F12 starts Dropshade on the first press; it slides down above every window, stays out of
      Alt+Tab and the dash, and slides up on the next press.
- [ ] The top bar icon shows while it runs: a click toggles it, a right click opens the menu with
      *Settings…* and *About Dropshade*.
- [ ] With the overview open, F12 closes the overview and shows Dropshade.
- [ ] Lock the screen and unlock it: Dropshade keeps its tabs and the icon comes back.
- [ ] `gnome-extensions disable dropshade@samuelba.dev`, then `enable`: the window is taken over
      again, still shown or hidden as it was.
- [ ] `journalctl --user -b | grep -i dropshade` shows no errors from the extension.
- [ ] After `./prune.sh` and a fresh login, the extension and its settings are gone.

## KDE Plasma and COSMIC

- [ ] The first start asks for the F12 shortcut (Plasma) or adds it to the custom shortcuts
      (COSMIC); pressing it toggles Dropshade.
- [ ] The tray icon shows while it runs; a click toggles the terminal.

## When something fails

- App log: `RUST_LOG=dropshade=debug dropshade 2>&1 | tee dropshade.log`
- A blank or crashing window in a VM without 3D acceleration: retry with
  `ICED_BACKEND=tiny-skia`.
