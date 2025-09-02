#!/bin/bash
set -e

# Use user directories that are properly integrated with desktop environments
BIN_DIR="$HOME/.local/bin"
XDG_DATA_HOME="${XDG_DATA_HOME:-$HOME/.local/share}"
APPLICATIONS_DIR="$XDG_DATA_HOME/applications"
ICONS_DIR="$XDG_DATA_HOME/icons/hicolor/scalable/apps"

BINARY_PATH="$BIN_DIR/conditional-launcher"
DESKTOP_FILE_PATH="$APPLICATIONS_DIR/conditional-launcher.desktop"
ICON_FILE_PATH="$ICONS_DIR/conditional-launcher.svg"

REPO_BASE_URL="https://raw.githubusercontent.com/Mayurifag/conditional-launcher/main"

echo "Creating necessary directories..."
mkdir -p "$BIN_DIR"
mkdir -p "$APPLICATIONS_DIR"
mkdir -p "$ICONS_DIR"

echo "Downloading the latest binary..."
curl -L "https://github.com/Mayurifag/conditional-launcher/releases/latest/download/conditional-launcher-linux-x86_64" -o "$BINARY_PATH"
chmod +x "$BINARY_PATH"

echo "Creating desktop file..."
cat << EOF > "$DESKTOP_FILE_PATH"
[Desktop Entry]
Name=Conditional Launcher
Exec=$BINARY_PATH
Icon=conditional-launcher
Type=Application
Categories=Utility;
Comment=Autostart apps after boot on your conditions.
EOF

echo "Downloading icon file..."
curl -sL "$REPO_BASE_URL/assets/icon.svg" -o "$ICON_FILE_PATH"

echo "Updating desktop and icon caches..."
if command -v update-desktop-database &> /dev/null; then
    update-desktop-database "$APPLICATIONS_DIR"
fi
if command -v gtk-update-icon-cache &> /dev/null; then
    gtk-update-icon-cache "$XDG_DATA_HOME/icons/hicolor"
fi

echo ""
echo "Installation complete."

# Only show PATH instructions if ~/.local/bin is not in PATH
if [[ ":$PATH:" != *":$HOME/.local/bin:"* ]]; then
    echo ""
    echo "To run from terminal, add ~/.local/bin to your PATH by adding this line to ~/.bashrc or ~/.zshrc:"
    echo 'export PATH="$HOME/.local/bin:$PATH"'
fi
