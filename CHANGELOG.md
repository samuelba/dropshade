# Changelog

## [0.3.0](https://github.com/samuelba/dropshade/compare/v0.2.0...v0.3.0) (2026-11-10)


### Features

* add a tab width setting to size tabs to their names ([6eeaad5](https://github.com/samuelba/dropshade/commit/6eeaad5b4f48a40b52c20532b50afbc6d2a1a9eb))
* **gnome-extension:** notify when the Dropshade app is not installed ([5ee26b6](https://github.com/samuelba/dropshade/commit/5ee26b6a3b689c9e7ff20f08e7cad8ad28f749ff))
* **gnome-extension:** notify when the Dropshade app is not installed ([a3b8490](https://github.com/samuelba/dropshade/commit/a3b84906661f94eb32f8dc9f8e15fa552f6358c6))

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
