#!/usr/bin/env bash
# Build the Arch package inside the archlinux:base-devel image.
# makepkg itself runs as an unprivileged user. The image starts as root only
# so it can create that user and install sudo.
set -euo pipefail

work="${1:-/work}"
tag="${2:-}"

if [[ "$(id -u)" -ne 0 ]]; then
  echo "container-build.sh must start as root inside the image" >&2
  exit 1
fi

pacman-key --init
pacman-key --populate archlinux
pacman -Syu --noconfirm --disable-download-timeout archlinux-keyring
pacman -S --needed --noconfirm --disable-download-timeout base-devel sudo curl ca-certificates

if ! id builder >/dev/null 2>&1; then
  useradd --create-home --shell /bin/bash builder
fi
echo 'builder ALL=(ALL) NOPASSWD: /usr/bin/pacman' >/etc/sudoers.d/builder
chmod 440 /etc/sudoers.d/builder

chown -R builder:builder "$work"
chmod -R a+rX "$work"

if [[ -n "$tag" ]]; then
  version="${tag#v}"
  pkgbuild="$work/packaging/arch/PKGBUILD"
  pkgver="$(sed -n 's/^pkgver=//p' "$pkgbuild")"
  if [[ "$pkgver" != "$version" ]]; then
    echo "PKGBUILD pkgver=$pkgver does not match tag $tag" >&2
    exit 1
  fi
  archive="$work/packaging/arch/qingyin-${version}.tar.gz"
  curl -fsSL -o "$archive" \
    "https://github.com/Nook001/QingyinPlayer/archive/refs/tags/${tag}.tar.gz"
  sha="$(sha256sum "$archive" | awk '{print $1}')"
  echo "GitHub archive sha256: $sha"
  # The tagged tree cannot contain its own archive checksum. Patch the first sum.
  sed -i "0,/'[0-9a-f]\\{64\\}'/s//'${sha}'/" "$pkgbuild"
  chown builder:builder "$archive" "$pkgbuild"
fi

pkgdest="$(mktemp -d /tmp/qingyin-pkgdest.XXXXXX)"
chown builder:builder "$pkgdest"
su builder -s /bin/bash -c "cd '$work/packaging/arch' && PKGDEST='$pkgdest' makepkg -s --noconfirm --needed"
cp -a "$pkgdest"/qingyin-[0-9]*.pkg.tar.zst "$work/packaging/arch/"
chmod -R a+rX "$work/packaging/arch"
ls -l "$work"/packaging/arch/*.pkg.tar.zst
