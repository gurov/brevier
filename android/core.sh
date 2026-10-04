#!/bin/sh
# Ядро под Android: общая библиотека на каждую ABI — туда, где её ждёт Gradle.
#
#     android/core.sh <папка> [abi…]      # abi: arm64-v8a, x86_64
#
# Без списка — обе: телефон arm64, эмулятор x86_64. NDK ищется
# в ANDROID_NDK_HOME (Gradle передаёт сюда свой `ndkVersion`), иначе самый
# свежий в SDK. Rust — из `android/rust-toolchain.toml`.
#
# Сборка воспроизводима: F-Droid собирает то же самое у себя и сверяет
# побайтно с APK из Releases. Поэтому версии закреплены, а пути машины
# в библиотеку не попадают.
set -e
out=$1
shift
[ -n "$out" ] || { echo "usage: core.sh <dir> [abi…]" >&2; exit 2; }
[ $# -gt 0 ] || set -- arm64-v8a x86_64
mkdir -p "$out"
out=$(cd "$out" && pwd)

repo=$(cd "$(dirname "$0")/.." && pwd)
ndk=${ANDROID_NDK_HOME:-${ANDROID_NDK_ROOT:-}}
if [ -z "$ndk" ]; then
    sdk=${ANDROID_HOME:-${ANDROID_SDK_ROOT:-$HOME/Android/Sdk}}
    ndk=$(ls -d "$sdk"/ndk/* 2>/dev/null | sort -V | tail -n 1)
fi
[ -d "$ndk" ] || { echo "no NDK found: set ANDROID_NDK_HOME" >&2; exit 1; }
bin=$ndk/toolchains/llvm/prebuilt/$(uname -s | tr 'A-Z' 'a-z')-x86_64/bin
# Нижняя граница та же, что у приложения: ниже системный проверяющий
# сертификаты не знает отзыва.
api=24

# Gradle, запущенный не из оболочки (F-Droid, IDE), профиля не читал,
# и ~/.cargo/bin в его PATH нет.
cargo_home=${CARGO_HOME:-$HOME/.cargo}
command -v cargo >/dev/null 2>&1 || PATH=$cargo_home/bin:$PATH

# rustup выбирает тулчейн по файлу в текущем каталоге.
cd "$repo/android"

# Места паник — это пути к исходникам, и без замены в библиотеку попадают
# ~/.cargo/registry, корень репозитория и исходники std (если стоит rust-src,
# rustc подставляет их вместо /rustc/<хеш>). Заменяем их одинаковыми везде.
# Флаги — через CARGO_ENCODED_RUSTFLAGS (разделитель 0x1f), чтобы пробел
# в пути их не разрезал; при --target они не трогают build-скрипты.
sysroot=$(rustc --print sysroot)
commit=$(rustc -vV | sed -n 's/^commit-hash: //p')
sep=$(printf '\037')
CARGO_ENCODED_RUSTFLAGS="--remap-path-prefix=$cargo_home=/cargo$sep--remap-path-prefix=$repo=/brevier$sep--remap-path-prefix=$sysroot/lib/rustlib/src/rust=/rustc/$commit"
export CARGO_ENCODED_RUSTFLAGS

for abi in "$@"; do
    case $abi in
        arm64-v8a) target=aarch64-linux-android ;;
        x86_64) target=x86_64-linux-android ;;
        *) echo "unknown abi: $abi" >&2; exit 2 ;;
    esac
    var=$(echo "$target" | tr '-' '_')
    upper=$(echo "$var" | tr 'a-z' 'A-Z')
    export "CC_$var=$bin/$target$api-clang"
    export "AR_$var=$bin/llvm-ar"
    export "CARGO_TARGET_${upper}_LINKER=$bin/$target$api-clang"
    # То же для C-кода `ring`.
    export "CFLAGS_$var=-ffile-prefix-map=$cargo_home=/cargo -ffile-prefix-map=$repo=/brevier"
    cargo rustc --locked --manifest-path "$repo/Cargo.toml" --release --lib --crate-type cdylib \
        --target "$target" --features typeset,save
    mkdir -p "$out/$abi"
    cp "$repo/target/$target/release/libbrevier.so" "$out/$abi/"
    "$bin/llvm-strip" "$out/$abi/libbrevier.so"
done
