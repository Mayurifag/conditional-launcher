# conditional-launcher

## Description

Cross-platform GUI application, which will help user to autostart apps after
system boot based on conditions.

## Features

- Launch app / if there is internet connection / there is partition mounted
- Works on Windows/MacOS/Linux (KDE Wayland dev env)
- Restart apps via cron or other systems alternatives
- Portable, single binary + config file. No tauri/electron/etc.
- Dark theme using Dracula colors, minimalistic style
- Uses ready library/crates to have less custom code
- Manage other autoruns in system, convert onto autorun and back (save "system"
  state optionally)
- Realized methods with Facade and other patterns, which will use
  implementations of Windows/Linux/Macos in various files.
