#!/bin/sh
# cli одним файлом (#1): `brevier` без окна не тянет ни GTK, ни OpenSSL, и его
# можно отдать бинарником — для скриптов, CI с `--check`, машин без GTK.
# Linux — под musl, статически: иначе бинарник требует glibc не старее
# сборочной машины. macOS — один универсальный файл (arm64 + x86_64).
# Windows — со статической CRT, без DLL рядом. Шрифтов в cli нет (они вшиты
# только в окно), поэтому рядом — две наши лицензии и всё.
#
#     packaging/cli.sh <платформа> [куда]
#
# Платформы: x86_64-linux, aarch64-linux, macos, windows-x86_64. Имя архива
# без версии — `brevier-cli-<платформа>.tar.gz` (`.zip` для Windows), чтобы
# ссылка `releases/latest/download/…` не менялась от выпуска к выпуску.
# Нужны цели rustup, а для Linux — `musl-gcc` (пакет `musl-tools`): ring
# собирает свою часть на C.
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/.." && pwd)
platform=$1
out=${2:-"$root/packaging/dist"}
name="brevier-cli-$platform"
stage="$out/$name"
target_dir=${CARGO_TARGET_DIR:-"$root/target"}

# Символы отладки бинарнику для людей не нужны: без них он заметно легче.
build() {
    rustup target add "$1" >/dev/null
    CARGO_PROFILE_RELEASE_STRIP=symbols \
        cargo build --release --locked --features save --bin brevier --target "$1"
}

cd "$root"
case $platform in
    x86_64-linux | aarch64-linux)
        target="${platform%-linux}-unknown-linux-musl"
        build "$target"
        binary="$target_dir/$target/release/brevier"
        ;;
    macos)
        build aarch64-apple-darwin
        build x86_64-apple-darwin
        binary="$target_dir/brevier-universal"
        lipo -create -output "$binary" \
            "$target_dir/aarch64-apple-darwin/release/brevier" \
            "$target_dir/x86_64-apple-darwin/release/brevier"
        ;;
    windows-x86_64)
        RUSTFLAGS="${RUSTFLAGS:-} -C target-feature=+crt-static" build x86_64-pc-windows-msvc
        binary="$target_dir/x86_64-pc-windows-msvc/release/brevier.exe"
        ;;
    *)
        echo "unknown platform: $platform" >&2
        exit 1
        ;;
esac

rm -rf "$stage"
mkdir -p "$stage"
file=$(basename "$binary" | sed 's/-universal$//')
cp "$binary" "$stage/$file"
cp LICENSE-MIT LICENSE-APACHE "$stage/"
"$stage/$file" --version

cd "$out"
if [ "$platform" = windows-x86_64 ]; then
    rm -f "$name.zip"
    7z a -tzip "$name.zip" "$name" >/dev/null
    echo "$out/$name.zip"
else
    tar -czf "$name.tar.gz" "$name"
    echo "$out/$name.tar.gz"
fi
rm -rf "$stage"
