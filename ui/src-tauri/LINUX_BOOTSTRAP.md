# Tauri Linux Bootstrap

Use one of the package sets below, then verify with `pkg-config` and `cargo check`.

## Ubuntu / Debian

```bash
sudo apt update
sudo apt install -y \
  build-essential pkg-config curl wget file \
  libgtk-3-dev libwebkit2gtk-4.1-dev \
  libayatana-appindicator3-dev librsvg2-dev \
  libssl-dev
```

## Fedora

```bash
sudo dnf install -y \
  @development-tools pkgconf-pkg-config curl wget file \
  gtk3-devel webkit2gtk4.1-devel \
  libappindicator-gtk3-devel librsvg2-devel \
  openssl-devel
```

## Arch Linux

```bash
sudo pacman -Syu --needed \
  base-devel pkgconf curl wget file \
  gtk3 webkit2gtk-4.1 libappindicator-gtk3 \
  librsvg openssl
```

## Verify

```bash
pkg-config --modversion glib-2.0 gio-2.0 gobject-2.0 gdk-3.0 cairo
cargo check --manifest-path ui/src-tauri/Cargo.toml
```

If `libwebkit2gtk-4.1-dev` is unavailable on your distro release, install the closest available `webkit2gtk` dev package and re-run the verify commands.
