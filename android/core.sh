#!/bin/sh
# Ядро под Android: общая библиотека на каждую ABI — туда, где её ждёт Gradle.
#
#     android/core.sh <папка> [abi…]      # abi: arm64-v8a, x86_64
#
# Без списка — обе: телефон arm64, эмулятор x86_64. Команда та же, что гоняет
# канарейка в CI. NDK ищется в ANDROID_NDK_HOME, иначе самый свежий в SDK.
set -e
out=$1
shift
[ -n "$out" ] || { echo "usage: core.sh <dir> [abi…]" >&2; exit 2; }
[ $# -gt 0 ] || set -- arm64-v8a x86_64

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
    cargo rustc --manifest-path "$repo/Cargo.toml" --release --lib --crate-type cdylib \
        --target "$target" --features typeset,save
    mkdir -p "$out/$abi"
    cp "$repo/target/$target/release/libbrevier.so" "$out/$abi/"
    "$bin/llvm-strip" "$out/$abi/libbrevier.so"
done
