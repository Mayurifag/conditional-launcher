# conditional-launcher

Autostart apps after boot on **your** conditions.

![Screenshot](Screenshot.webp)

## Features

- Autolaunch your desktop apps if there is internet connection or there is some
  partition mounted.
- Keeps autostart shortcuts in place, but makes them no-op — apps think they are
  autostarted like nothing happened and won't mess.
- Edit launch params — add "silent" options, "startintray" or similar.
- In system settings of KDE in Autostart page you will clearly see whats managed
- Portable, native, no ads, no bs, no electron. Just single binary and configs.
- Dark theme, minimalistic style.
- Currently tested on x64 KDE Wayland. Others (MacOS/Windows/arm64) incoming if
  needed.

## How it works

- Writes itself to autostart if there is at least one app managed by it and
  removes itself if not. Backups original shortcuts in app config dir.
- Checks internet via request to
  `http://connectivitycheck.gstatic.com/generate_204`. Thats 99.99% not blocked,
  fast (no TLS handshaking). Also checks DNS resolution.
- On system boot launches in background, checks conditions and launches apps.
  Exits after that with notification. Dead simple, just works.

## Installation

To download and install the latest release for x86_64 Linux, run the following
command. This will place the binary in `~/.local/bin`.

    curl -sL https://raw.githubusercontent.com/Mayurifag/conditional-launcher/main/install.sh | bash

If you want to remove binary sometime later, unmanage all apps — this will get
your autostart shortcuts back like nothing happened to them, so you will be
free to delete the binary after that.

## Personal example of usage

I created this app to have universal and convenient way across my systems to:

- Launch Nextcloud and messengers only if there is internet connection
- Launch Steam only when disk with games is mounted

Why would I need those apps if conditions aren't met yet? They waste resources!

## Roadmap

- Migrate from `egui` to [something with *retaining* mode](https://github.com/emilk/egui?tab=readme-ov-file#why-immediate-mode).
  Use Dracula colors
- Uninstall script including getting back original desktop files automatically.
- Add macos support + release
- Add windows support + release
- Add custom commands functionality. Add possibility to cron them. That way
  ayugram/espanso might be restarted easily daily to prevent their memory leaks
  and other problems.
- Release cargo and think about simpler installation (brew/aur?)
- I probably might add 'status' CLI command to see current status debug info.
