pkgname=ncm-desktop-for-linux-git
pkgver=1.0.0.r0
pkgrel=1
pkgdesc="A third-party NetEase Cloud Music client for Linux"
arch=('x86_64')
url="https://github.com/dddqmmx/ncm-desktop-for-linux"
license=('MIT')
depends=('electron')
makedepends=('nodejs' 'npm' 'git' 'python')
provides=('ncm-desktop-for-linux')
conflicts=('ncm-desktop-for-linux')

source=(
  "git+$url.git"
)
sha256sums=('SKIP')

pkgver() {
  cd "$srcdir/ncm-desktop-for-linux"
  git describe --tags --long | sed 's/^v//;s/-/.r/;s/-/./'
}

build() {
  cd "$srcdir/ncm-desktop-for-linux"

  export ELECTRON_SKIP_BINARY_DOWNLOAD=1
  npm ci
  npm run build:linux
}

package() {
  cd "$srcdir/ncm-desktop-for-linux"

  # 主程序（AppImage）
  install -Dm755 dist/*.AppImage \
    "$pkgdir/usr/bin/ncm-desktop-for-linux"

  # desktop 文件
  install -Dm644 packaging/ncm-desktop-for-linux.desktop \
    "$pkgdir/usr/share/applications/ncm-desktop-for-linux.desktop"

  # 图标（256x256 示例）
  install -Dm644 packaging/icon.png \
    "$pkgdir/usr/share/icons/hicolor/256x256/apps/ncm-desktop-for-linux.png"
}

