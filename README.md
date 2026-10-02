# Mercury for the Odin 2

Find PC games, download them through Real-Debrid, install them, and add them to Steam with art and
Proton. Runs only on the AYN Odin 2 Portal under Armada OS, inside Steam Game Mode as a Decky plugin.

## Layout

| Path | What |
|---|---|
| `mercuryd/` | Native aarch64 Rust engine. HTTP API on `127.0.0.1:47800`. Sources, Real-Debrid, downloads, extraction, library. |
| `plugin/` | Decky plugin. React UI in Steam, Python backend that starts and stops `mercuryd`. Steam writes happen here. |
| `scripts/set-rd-key.sh` | Sends the Real-Debrid key from the Mac clipboard to the Odin. |
| `deploy.sh` | Syncs to the Odin, builds there in the `lsfg-vk-build` distrobox, installs into `~/homebrew/plugins/mercury`. |

## Setting the Real-Debrid key

Copy the token from real-debrid.com/apitoken on the Mac, then run `scripts/set-rd-key.sh`. It reads the
clipboard, clears it, checks the token with Real-Debrid from the Odin, and saves it in Mercury only if it
works. The token goes over SSH stdin, so it never appears in a command line, shell history or a file on the
Mac. You can also type it in Mercury > Settings on the device.

## How a game gets installed

1. Pick a game (Steam wishlist, search, or a store page through the Quick Access panel) and a source.
2. `mercuryd` adds the magnet to Real-Debrid and waits for it to cache, or unrestricts a direct link.
3. It downloads the files (pause and resume use HTTP Range), checks free space, and extracts them
   with 7z or unrar. OnlineFix's password-protected inner archives are extracted too.
4. Pre-installed games: it ranks the `.exe` files and picks the game. The plugin adds a Steam shortcut
   with portrait, hero, logo and wide art, the chosen Proton, and the launch options from settings.
5. Repacks (a top-level `setup.exe`): the plugin adds `setup.exe` as the shortcut and runs it through
   Proton. When the installer closes, `mercuryd` finds the game inside that shortcut's prefix and the
   plugin repoints the same shortcut at it.

## Device facts this depends on

- Big Picture renders pages in a 910x512 CSS viewport (2.11x). Size layouts for that.
- Proton internal names come from `SteamClient.Apps.GetAvailableCompatTools`. A wrong name runs the
  `.exe` natively with no error.
- `AddShortcut` takes plain paths and quotes the exe itself.
- `ProgressBarItem` overflows panels. Use `Field` with `childrenLayout="below"` and a `ProgressBar`.
- Decky's hot reload does not remount an open Quick Access panel; restart `plugin_loader` to test panel changes.
- Decky re-owns the plugin folder to root on load. `dist/`, `bin/` and `main.py` stay writable by armada.
- Fedora's 7zip has no RAR codec. unrar is built from RARLAB source into `~/.local/share/mercury/bin/unrar`.

## Building unrar

```sh
distrobox enter lsfg-vk-build -- bash -lc 'cd ~/mercury/build && curl -sSfL https://www.rarlab.com/rar/unrarsrc-7.3.1.tar.gz | tar xz && make -C unrar -j8'
cp ~/mercury/build/unrar/unrar ~/.local/share/mercury/bin/
```

## License

MIT, see [LICENSE](LICENSE). Anyone may use, copy, modify and redistribute this code.

`plugin/rollup.config.js`, `plugin/tsconfig.json`, `plugin/decky.pyi` and the starting point of
`plugin/package.json` come from [decky-plugin-template](https://github.com/SteamDeckHomebrew/decky-plugin-template),
used under the BSD 3-Clause License. Its notice is kept in
[plugin/LICENSE.decky-plugin-template](plugin/LICENSE.decky-plugin-template).
