#!/bin/sh
# Собрать Brevier для Windows: установщик и zip с той же папкой программы.
# Папка — два exe, библиотеки GTK, без которых окно не поднимется, и данные,
# которые GTK ищет рядом с собой: схемы настроек, тема значков, лицензии.
# Ни один exe не работает в одиночку, кроме `brevier.exe` (cli), поэтому
# раздаём папку, а не файл.
#
# Выпуск собирается в MSYS2 (UCRT64) на Windows: GTK оттуда же, откуда его
# берут GIMP и Inkscape. Тот же скрипт работает кросс-сборкой на Linux —
# так его проверяют под Wine, — если назвать префикс и инструменты:
#
#     packaging/windows/build.sh [куда]          в MSYS2: префикс — $MINGW_PREFIX
#
#     MINGW_PREFIX=/usr/x86_64-w64-mingw32/sys-root/mingw \
#     WINDRES=x86_64-w64-mingw32-windres OBJDUMP=x86_64-w64-mingw32-objdump \
#     STRIP=x86_64-w64-mingw32-strip PKG_CONFIG=x86_64-w64-mingw32-pkg-config \
#     CARGO_TARGET_X86_64_PC_WINDOWS_GNU_LINKER=x86_64-w64-mingw32-gcc \
#         packaging/windows/build.sh              Fedora, пакеты mingw64-*
set -eu

root=$(CDPATH= cd -- "$(dirname -- "$0")/../.." && pwd)
out=${1:-"$root/packaging/dist"}
prefix=${MINGW_PREFIX:?MINGW_PREFIX: where GTK for Windows is installed}
windres=${WINDRES:-windres}
objdump=${OBJDUMP:-objdump}
strip=${STRIP:-strip}
target=x86_64-pc-windows-gnu

version=$(sed -n 's/^version = "\(.*\)"/\1/p' "$root/Cargo.toml" | head -1)
name="brevier-$version-windows-x86_64"
work="$root/target/windows"
stage="$work/Brevier"

# Путь для программ Windows. В MSYS2 они не понимают /d/a/…, а сами
# переводят пути только в аргументах, похожих на путь целиком.
native() {
    if command -v cygpath > /dev/null; then cygpath -m "$1"; else printf '%s\n' "$1"; fi
}

mkdir -p "$out" "$work"
out=$(CDPATH= cd -- "$out" && pwd)

# Иконка и версия — ресурсом в оба exe: их показывают Проводник, панель
# задач и «Установленные приложения».
cp "$root/packaging/windows/brevier.ico" "$work/"
cat > "$work/brevier.rc" <<EOF
#include <winver.h>
1 ICON "brevier.ico"
1 VERSIONINFO
FILEVERSION $(echo "$version" | tr . ,),0
PRODUCTVERSION $(echo "$version" | tr . ,),0
FILEOS VOS_NT_WINDOWS32
FILETYPE VFT_APP
BEGIN
  BLOCK "StringFileInfo"
  BEGIN
    BLOCK "040904B0"
    BEGIN
      VALUE "FileDescription", "Brevier"
      VALUE "FileVersion", "$version"
      VALUE "ProductName", "Brevier"
      VALUE "ProductVersion", "$version"
      VALUE "LegalCopyright", "MIT OR Apache-2.0"
    END
  END
  BLOCK "VarFileInfo"
  BEGIN
    VALUE "Translation", 0x409, 1200
  END
END
EOF
(cd "$work" && "$windres" brevier.rc -O coff -o brevier-rc.o)

# Цель названа явно, хотя в MSYS2 она и так родная: с `--target` флаги
# из переменной не достаются сборочным скриптам, и ресурс ложится только в exe.
cd "$root"
export CARGO_TARGET_X86_64_PC_WINDOWS_GNU_RUSTFLAGS="-C link-arg=$(native "$work/brevier-rc.o")"
cargo build --release --locked --features ui --target "$target" --bins

rm -rf "$stage"
mkdir -p "$stage/bin"
for exe in brevier.exe brevier-ui.exe; do
    install -m755 "${CARGO_TARGET_DIR:-target}/$target/release/$exe" "$stage/bin/"
    "$strip" "$stage/bin/$exe"
done
# Через `gdbus.exe` GLib на Windows поднимает сессионную шину, а на ней
# держится единственный экземпляр: адрес из второго запуска уходит вкладкой
# в уже открытое окно.
install -m755 "$prefix/bin/gdbus.exe" "$stage/bin/"

# Библиотеки — по таблицам импорта, до замыкания. Чего нет в префиксе
# (kernel32, user32…), то система.
todo=$(ls "$stage/bin")
while [ -n "$todo" ]; do
    next=
    for file in $todo; do
        for dll in $("$objdump" -p "$stage/bin/$file" | sed -n 's/^[[:space:]]*DLL Name: //p'); do
            found=$(ls "$prefix/bin" | grep -i -x -F "$dll" | head -n 1) || true
            if [ -n "$found" ] && [ ! -e "$stage/bin/$found" ]; then
                install -m755 "$prefix/bin/$found" "$stage/bin/"
                next="$next $found"
            fi
        done
    done
    todo=$next
done

# Схемы настроек GTK: без них диалог файлов падает, не открывшись.
schemas="$stage/share/glib-2.0/schemas"
mkdir -p "$schemas"
cp "$prefix"/share/glib-2.0/schemas/org.gtk.gtk4.*.gschema.xml "$schemas/"
glib-compile-schemas "$(native "$schemas")"

# Тема значков: кнопки окна берут значки по имени, как на Linux.
mkdir -p "$stage/share/icons/hicolor"
cp -R "$prefix/share/icons/Adwaita" "$stage/share/icons/"
cp "$prefix/share/icons/hicolor/index.theme" "$stage/share/icons/hicolor/"
cache=$(command -v gtk4-update-icon-cache || command -v gtk-update-icon-cache || true)
for theme in Adwaita hicolor; do
    [ -z "$cache" ] || "$cache" -q -t -f "$(native "$stage/share/icons/$theme")"
done

# Лицензии: наша, шрифтов (OFL требует, чтобы её текст ехал с ними)
# и каждой библиотеки — по пакету, из которого она взята.
licenses="$stage/licenses"
mkdir -p "$licenses"
cp LICENSE-MIT LICENSE-APACHE assets/fonts/OFL-NotoSans.txt "$licenses/"
owner() {
    if command -v pacman > /dev/null; then
        pacman -Qqo "$1" 2> /dev/null || true
    else
        rpm -qf --qf '%{NAME}\n' "$1" 2> /dev/null | grep -v ' ' || true
    fi
}
packages=$(
    for file in $(ls "$stage/bin"); do
        if [ -e "$prefix/bin/$file" ]; then owner "$prefix/bin/$file"; fi
    done
    owner "$prefix/share/icons/Adwaita/index.theme"
    owner "$prefix/share/glib-2.0/schemas/org.gtk.gtk4.Settings.FileChooser.gschema.xml"
)
packages=$(printf '%s\n' $packages | sort -u)
{
    echo "Brevier $version for Windows ships these libraries and data unchanged,"
    if command -v pacman > /dev/null; then
        echo "as built by MSYS2. Their sources: https://github.com/msys2/MINGW-packages"
        echo "and https://repo.msys2.org/mingw/sources/"
    else
        echo "as built by the Fedora MinGW project."
    fi
    echo
    for package in $packages; do
        if command -v pacman > /dev/null; then pacman -Q "$package"; else rpm -q "$package"; fi
    done
} > "$licenses/COMPONENTS.txt"
for package in $packages; do
    if command -v pacman > /dev/null; then
        files=$(pacman -Qlq "$package" | grep '/share/licenses/.*[^/]$' || true)
    else
        files=$(rpm -qL "$package" || true)
    fi
    for file in $files; do
        mkdir -p "$licenses/$package"
        cp "$file" "$licenses/$package/"
    done
done

cat > "$stage/README.txt" <<EOF
Brevier $version for Windows

Start bin\\brevier-ui.exe for the window; bin\\brevier.exe is the command line.
History, bookmarks and settings live in %LOCALAPPDATA%\\Brevier.

Screen readers do not see the window on Windows yet: GTK's accessibility
works only on Linux.

https://github.com/gurov/brevier
EOF

rm -f "$out/$name.zip" "$out/$name-setup.exe"
(cd "$work" && zip -q -r -9 "$(native "$out/$name.zip")" Brevier)
makensis -V2 -DVERSION="$version" -DSTAGE="$(native "$stage")" \
    -DOUTFILE="$(native "$out/$name-setup.exe")" "$(native "$root/packaging/windows/brevier.nsi")"

echo "$out/$name-setup.exe"
echo "$out/$name.zip"
