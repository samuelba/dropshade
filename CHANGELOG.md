# Changelog

## [0.9.0](https://github.com/samuelba/dropshade/compare/v0.8.0...v0.9.0) (2026-11-14)


### Features

* add a setting to start Dropshade hidden when the user logs in (@samuelba) ([2eb87f1](https://github.com/samuelba/dropshade/commit/2eb87f106946719d533d089feec52a35d077aae0))
* open the desktop's shortcut editor for the portal hotkey (@samuelba) ([f1eac9b](https://github.com/samuelba/dropshade/commit/f1eac9b940f0b474fd11d9fb138fc616e7fdcafb))
* show a tray icon on COSMIC and KDE Plasma (@samuelba) ([6024e3e](https://github.com/samuelba/dropshade/commit/6024e3ecf1a7e307faa1dcd08fa66fda748c3b54))


### Bug Fixes

* focus the pane that a middle click pastes into (@samuelba) ([3780b37](https://github.com/samuelba/dropshade/commit/3780b375b9df51da84cc92d907bb254be4a4ff9e))
* **gnome:** hide the dropdown from Alt+Tab and the dash on GNOME 46 (@samuelba) ([ccd7949](https://github.com/samuelba/dropshade/commit/ccd794925de418b233b6cf45167b0bb965825670))

## [0.8.0](https://github.com/samuelba/dropshade/compare/v0.7.0...v0.8.0) (2026-11-14)


### Features

* add the COSMIC hotkey shortcut on the first start (@samuelba) ([4fbe118](https://github.com/samuelba/dropshade/commit/4fbe1188c7f90e07b31f7367155c338879774dea))
* restore each pane's scrollback on start (opt-in) (@samuelba) ([89cca5b](https://github.com/samuelba/dropshade/commit/89cca5b729a61af1ca43ce782e7d1dbced83982f))

## [0.7.0](https://github.com/samuelba/dropshade/compare/v0.6.0...v0.7.0) (2026-11-13)


### Features

* add settings for the window's vertical position and animation (@samuelba) ([b8a4e0c](https://github.com/samuelba/dropshade/commit/b8a4e0cf7580867c3ad97f589d9675f494fa98d3))
* add settings to jump to the newest output on output or keystroke (@samuelba) ([e9b60aa](https://github.com/samuelba/dropshade/commit/e9b60aaf7dd492482484d516d04e2ec9cd46692f))
* add vi mode to select output with the keyboard (Ctrl+Shift+Space) (@samuelba) ([0c5fbd9](https://github.com/samuelba/dropshade/commit/0c5fbd96810421d14a8e0580d8213795e9dc471c))
* open settings with Ctrl+, (rebindable as open-settings) (@samuelba) ([1f00334](https://github.com/samuelba/dropshade/commit/1f00334e2a5183c05f238643c13df209aca6ea37))
* open the terminal menu from the empty part of the tab bar (@samuelba) ([c52ea30](https://github.com/samuelba/dropshade/commit/c52ea30f9123c3c3b49603971c86d39702db1aad))
* save and restore split panes in sessions and layouts (@samuelba) ([4424ba8](https://github.com/samuelba/dropshade/commit/4424ba832156839a59c5992a1f3670c7a36dc933))
* split tabs into panes (Ctrl+Shift+R right, Ctrl+Shift+D down) (@samuelba) ([38a20c9](https://github.com/samuelba/dropshade/commit/38a20c99a2a2be52a9f85175a381814ba966c017))


### Bug Fixes

* **gnome:** use the desktop's cursor theme before GNOME 48 (@samuelba) ([fb9ccd1](https://github.com/samuelba/dropshade/commit/fb9ccd181568a32f7a5742407a28c2cbdd429043))

## [0.6.0](https://github.com/samuelba/dropshade/compare/v0.5.0...v0.6.0) (2026-11-12)


### Features

* add shell integration for bash and zsh with OSC 133 marks (@samuelba) ([789dbc8](https://github.com/samuelba/dropshade/commit/789dbc854785cfc0747732a29225bb614d55bc16))
* make the visual bell optional and off by default (@samuelba) ([a581134](https://github.com/samuelba/dropshade/commit/a581134a7a28251e82504e88d495312f7e241fc2))
* toggle maximize with F11 or the terminal menu (@samuelba) ([62917df](https://github.com/samuelba/dropshade/commit/62917dfb853478cb800965ec3f97709887ee127d))


### Bug Fixes

* draw emoji with the color emoji font (@samuelba) ([17901c3](https://github.com/samuelba/dropshade/commit/17901c3138da67baf55710bbb7c1f1340cb88a27))
* **gnome-extension:** icon for gnome appindicator in deb and rpm (@samuelba) ([c4a5ffe](https://github.com/samuelba/dropshade/commit/c4a5ffeb51eb505737f205bffd3c464d471fe5e6))
* **gnome-extension:** keep the app running when the extension reloads (@samuelba) ([49e6652](https://github.com/samuelba/dropshade/commit/49e665263b988c8a744e89d76c2dc0f115cfd1fb))

## [0.5.0](https://github.com/samuelba/dropshade/compare/v0.4.1...v0.5.0) (2026-11-12)


### Features

* mark tabs on bell or output and notify when long commands finish (@samuelba) ([f8ed0b9](https://github.com/samuelba/dropshade/commit/f8ed0b98478baec6c499a3a53c7a6ecb2640bb1a))
* paste files dropped onto the window as quoted paths (@samuelba) ([237ce36](https://github.com/samuelba/dropshade/commit/237ce3643edc1cb3ac0fb8c3ab502bdce92f32fc))
* **tabs:** shorten long tab labels with an ellipsis (@samuelba) ([dd90f0f](https://github.com/samuelba/dropshade/commit/dd90f0fb11181efca2e74bd5a2101e5579f0606c))
* **term:** add zoom shortcuts (@samuelba) ([6041f64](https://github.com/samuelba/dropshade/commit/6041f642fe35355bba51f9b15b1ea74b450dc7d7))
* **term:** show shortcut hints in the context menu (@samuelba) ([22db49f](https://github.com/samuelba/dropshade/commit/22db49fd482600fb51420f1b8aadacdd32a5c476))

## [0.4.1](https://github.com/samuelba/dropshade/compare/v0.4.0...v0.4.1) (2026-11-11)


### Bug Fixes

* give the shell TERM=xterm-256color instead of alacritty (@samuelba) ([6d92e71](https://github.com/samuelba/dropshade/commit/6d92e71a2fd1a11cdd7e3d57bf71d4ee24c59e9b))
* record a window's size when it opens (@samuelba) ([f572eb1](https://github.com/samuelba/dropshade/commit/f572eb1627b6e21ed01adcd20382b4b7ab91f503))
* **term:** treat input as regaining focus after an unfocus (@samuelba) ([31f37b5](https://github.com/samuelba/dropshade/commit/31f37b5466bb643df66d7d1680907bb730eea98a))

## [0.4.0](https://github.com/samuelba/dropshade/compare/v0.3.0...v0.4.0) (2026-11-11)


### Features

* align a narrower drop-down to the left, center or right (@samuelba) ([a2e6d0f](https://github.com/samuelba/dropshade/commit/a2e6d0fd1cc74b3da76ab03881e40da2eca483ed))
* bind the hotkey through the GlobalShortcuts portal (@samuelba) ([dfb6bc5](https://github.com/samuelba/dropshade/commit/dfb6bc5c3ae1cbb6e6b3d842c88cede282496c38))
* check GitHub for a newer release on start (@samuelba) ([4460477](https://github.com/samuelba/dropshade/commit/44604778181b7e83a9bfb71767dae4600c777189))
* drop down as a layer-shell surface on COSMIC and other Wayland compositors (@samuelba) ([2d52cf7](https://github.com/samuelba/dropshade/commit/2d52cf74bcf02cd4985cac9722638ef2632ba538))
* **gnome-extension:** support GNOME 46 to 49 (@samuelba) ([ce15739](https://github.com/samuelba/dropshade/commit/ce157394805c440a4cd442911d0522d14d182ac8))
* override single terminal colors on top of the theme (@samuelba) ([130c63d](https://github.com/samuelba/dropshade/commit/130c63d48d28a5468445d0ddb1f44d7ed8774428))
* pick the screen from an ordered list of monitors (@samuelba) ([121ad10](https://github.com/samuelba/dropshade/commit/121ad1058145d8e93513c30b7df3bd7f52ddc878))
* set the hotkey in COSMIC's custom shortcuts (@samuelba) ([509cb4a](https://github.com/samuelba/dropshade/commit/509cb4a2db0f9a8f2c12f70c75731345967ef535))


### Bug Fixes

* copy and paste through the compositor's data-control protocol (@samuelba) ([48d43a6](https://github.com/samuelba/dropshade/commit/48d43a6c8085000ca13e4bfc40dfff15a33e51dc))
* **gnome-extension:** hide on the hotkey after focus moves elsewhere (@samuelba) ([b1799c9](https://github.com/samuelba/dropshade/commit/b1799c99de0292f4f4625115d395f088c1c1cca3))
* render the About icon from the app's SVG instead of a canvas copy (@samuelba) ([5dffea8](https://github.com/samuelba/dropshade/commit/5dffea82359be43777dd8980d46de29c3ba2d974))

## [0.3.0](https://github.com/samuelba/dropshade/compare/v0.2.0...v0.3.0) (2026-11-10)


### Features

* **gnome-extension:** notify when the Dropshade app is not installed (@samuelba) ([a3b8490](https://github.com/samuelba/dropshade/commit/a3b84906661f94eb32f8dc9f8e15fa552f6358c6))

## [0.2.0](https://github.com/samuelba/dropshade/compare/v0.1.0...v0.2.0) (2026-11-10)


### ⚠ BREAKING CHANGES

* the extension installed under the old UUID must be uninstalled and the new one installed (dropshade gnome-extension install).

### Features

* add an overlay scrollbar for the scrollback (@samuelba) ([aca5e58](https://github.com/samuelba/dropshade/commit/aca5e587219f0dfcc7a0e89eadb4a10cbb8ffe3e))
* add configuration file, themes, keybindings and live reload (@samuelba) ([59b4007](https://github.com/samuelba/dropshade/commit/59b4007029aff1963b847caacc5358b1887d667d))
* add scrollback search and a settings panel (@samuelba) ([dcc261a](https://github.com/samuelba/dropshade/commit/dcc261afdfa0dc45baf40a0d3d8a727e36ffa503))
* add tabs with a configurable tab bar (@samuelba) ([11cbef4](https://github.com/samuelba/dropshade/commit/11cbef460abc0e2cda5463526dd0f9bbaca2a136))
* add terminal core input, selection, links and IME (@samuelba) ([2bf993b](https://github.com/samuelba/dropshade/commit/2bf993b52d31cfa17b5d9771586616019967e670))
* add zoom, reset and About to the terminal context menu (@samuelba) ([e0ab191](https://github.com/samuelba/dropshade/commit/e0ab19102a2b9ef687328d24664d7f7bc54ac073))
* choose the drop-down's screen in the settings panel (@samuelba) ([07b1c61](https://github.com/samuelba/dropshade/commit/07b1c6119cafb87712e503d97561fa0abf553c97))
* connect the app and GNOME extension over D-Bus (@samuelba) ([e8eabce](https://github.com/samuelba/dropshade/commit/e8eabce7cfc9c865a22afb3f86522e449b71bd5a))
* cover every setting in the settings panel (@samuelba) ([01695a7](https://github.com/samuelba/dropshade/commit/01695a7df86e21784939e02669090e0f29b105eb))
* **gnome-extension:** add a top bar icon while Dropshade runs (@samuelba) ([1e8e2d4](https://github.com/samuelba/dropshade/commit/1e8e2d43fda3039c434e12df53dc6572666a5d84))
* restore sessions and add tab layouts (@samuelba) ([ab4cac3](https://github.com/samuelba/dropshade/commit/ab4cac3387530c4e4de0a3cb84acaed8ad1b2466))
* scaffold Dropshade terminal and GNOME Shell extension (@samuelba) ([4e2cff1](https://github.com/samuelba/dropshade/commit/4e2cff14f9616b21d961cef57cd09d4534b6edc2))


### Bug Fixes

* **gnome-extension:** stop Dropshade when the extension is disabled (@samuelba) ([dba2d4d](https://github.com/samuelba/dropshade/commit/dba2d4d83044d58d0689fdb423a945c6c5010c71))
* **release:** match Cargo.lock packages by name.value in release-please (@samuelba) ([d7701d8](https://github.com/samuelba/dropshade/commit/d7701d81cb445b34d1eb52beae9fe08a51aa4b97))


### Code Refactoring

* rename the app ID to dev.samuelba.Dropshade (@samuelba) ([8f2a097](https://github.com/samuelba/dropshade/commit/8f2a097200db667b5abfbe586210d3af5b9557f6))
