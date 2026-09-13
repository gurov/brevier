#!/bin/sh
# Поставить Brevier для одного пользователя: ни root, ни пакетного
# менеджера. Кладёт то же, что положил бы пакет, только в ~/.local.
#
#     ./install.sh              поставить
#     ./install.sh --uninstall  убрать
#
# Ярлык нужен не для красоты: в меню и в «Открыть с помощью» программа
# попадает именно им, а на Wayland по нему же выбирается иконка окна.
set -eu

here=$(CDPATH= cd -- "$(dirname -- "$0")" && pwd)
bin="${XDG_BIN_HOME:-$HOME/.local/bin}"
data="${XDG_DATA_HOME:-$HOME/.local/share}"
apps="$data/applications"
icons="$data/icons/hicolor/scalable/apps"
meta="$data/metainfo"
docs="$data/licenses/io.github.gurov.brevier"

if [ "${1:-}" = "--uninstall" ]; then
    rm -f "$bin/brevier" "$bin/brevier-ui" \
          "$apps/io.github.gurov.brevier.desktop" \
          "$icons/io.github.gurov.brevier.svg" \
          "$meta/io.github.gurov.brevier.metainfo.xml"
    rm -rf "$docs"
    command -v update-desktop-database >/dev/null 2>&1 &&
        update-desktop-database "$apps" 2>/dev/null || true
    echo "Brevier removed. Your history and bookmarks are still in"
    echo "  ${XDG_DATA_HOME:-$HOME/.local/share}/brevier"
    exit 0
fi

install -Dm755 "$here/brevier"    "$bin/brevier"
install -Dm755 "$here/brevier-ui" "$bin/brevier-ui"
install -Dm644 "$here/io.github.gurov.brevier.desktop" \
    "$apps/io.github.gurov.brevier.desktop"
install -Dm644 "$here/brevier.svg" \
    "$icons/io.github.gurov.brevier.svg"
install -Dm644 "$here/io.github.gurov.brevier.metainfo.xml" \
    "$meta/io.github.gurov.brevier.metainfo.xml"
# Текст OFL обязан ехать с дистрибутивом: гарнитуры вшиты в бинарник,
# и это условие их лицензии, отдельное от нашей MIT+Apache на код.
install -Dm644 "$here/OFL-NotoSans.txt" "$docs/OFL-NotoSans.txt"
install -Dm644 "$here/LICENSE-MIT"      "$docs/LICENSE-MIT"
install -Dm644 "$here/LICENSE-APACHE"   "$docs/LICENSE-APACHE"

command -v update-desktop-database >/dev/null 2>&1 &&
    update-desktop-database "$apps" 2>/dev/null || true

echo "Installed:"
echo "  $bin/brevier-ui    the window"
echo "  $bin/brevier       the command line"

# GTK приезжает от системы — единственное, чего в бинарнике нет.
if ldd "$bin/brevier-ui" 2>/dev/null | grep -q 'not found'; then
    echo
    echo "Missing libraries — GTK 4 is not installed:"
    ldd "$bin/brevier-ui" | grep 'not found' | sed 's/^/  /'
    echo "  Debian/Ubuntu: sudo apt install libgtk-4-1"
    echo "  Fedora:        sudo dnf install gtk4"
    echo "  Arch:          sudo pacman -S gtk4"
fi

case ":$PATH:" in
    *":$bin:"*) ;;
    *) echo; echo "Note: $bin is not in your PATH — the menu entry works anyway." ;;
esac

# Урок с живой машины: панель и меню строят список значков при своём
# запуске, и иконку, положенную позже, они не видят до перезапуска.
echo
echo "If the menu shows Brevier without its icon, the desktop shell is still"
echo "running with the icon list it built before the install. Log out and back in,"
echo "or on KDE Plasma: systemctl --user restart plasma-plasmashell.service"
