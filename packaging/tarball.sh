#!/bin/sh
# Собрать архив, который можно отдать человеку: два бинарника, ярлык,
# иконка, лицензии и `install.sh`. Второй путь раздачи после flatpak —
# дешёвый и без песочницы, ценой одного условия: GTK 4 должен стоять
# в системе, а сам бинарник пойдёт только на glibc не старее сборочной.
#
#     packaging/tarball.sh            собрать в packaging/dist
#     packaging/tarball.sh /куда-то   …или туда, куда сказано
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
out=${1:-"$root/packaging/dist"}

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
name="brevier-$version-$(uname -m)-linux"
stage="$out/$name"

cd "$root"
cargo build --release --features ui

rm -rf "$stage"
mkdir -p "$stage"
install -m755 target/release/brevier    "$stage/brevier"
install -m755 target/release/brevier-ui "$stage/brevier-ui"
install -m755 packaging/install.sh      "$stage/install.sh"
install -m644 packaging/io.github.gurov.brevier.desktop      "$stage/"
install -m644 packaging/io.github.gurov.brevier.metainfo.xml "$stage/"
install -m644 assets/brevier.svg          "$stage/brevier.svg"
install -m644 assets/fonts/OFL-NotoSans.txt "$stage/OFL-NotoSans.txt"
install -m644 LICENSE-MIT LICENSE-APACHE  "$stage/"

tar -C "$out" -czf "$out/$name.tar.gz" "$name"
rm -rf "$stage"

echo "$out/$name.tar.gz"
