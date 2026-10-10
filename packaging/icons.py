#!/usr/bin/env python3
"""Растровые иконки из знака: Android (все плотности), витрина F-Droid и .ico для Windows.

    packaging/icons.py                 # проверить assets/brevier.svg и перерисовать всё
    packaging/icons.py --check         # только проверить знак, ничего не писать

Знак один — `assets/brevier.svg`, остальное из него рисуется. Руками PNG
не правим: поменялся знак — запускаем скрипт. Нужны inkscape и Pillow.

Сначала проверка того, что ломалось раньше (CLAUDE.md, «Знак — контур»):
холст квадратный, текста нет (буква оконтурена), градиент один и плоский —
без `xlink:href` на соседний и без `gradientTransform`. Каждое сохранение
из Inkscape возвращает ссылку и матрицу, а слабые отрисовщики (QtSvg)
на этом рисуют градиент одним цветом.

Геометрия — та, что была у иконок до скрипта (восстановлена по пикселям):

- передний план адаптивной иконки: холст знака 56 dp на поле 108 dp,
  по центру, фон прозрачный — бумагу под него кладёт `ic_launcher.xml`;
- иконка до Android 8: бумага в скруглённом квадрате на всё поле 48 dp,
  радиус 8.4 dp, холст знака 32.5 dp по центру (дробный пиксель
  отбрасывается — так были сделаны прежние, проверено на всех плотностях);
- витрина (`fastlane/…/icon.png`, 512 px): бумага во весь квадрат, холст
  в той же доле видимой части, что на рабочем столе, — 56 dp из 72;
- Windows (`packaging/windows/brevier.ico`): сам знак, как на Linux, где
  иконка — это SVG; каждый размер отрисован отдельно, а не ужат из 256,
  иначе 16 px в заголовке окна выходят мылом.
"""
import os
import re
import subprocess
import sys
import tempfile

ROOT = os.path.dirname(os.path.dirname(os.path.abspath(__file__)))
SVG = os.path.join(ROOT, 'assets', 'brevier.svg')
RES = os.path.join(ROOT, 'android', 'app', 'src', 'main', 'res')
STORE = os.path.join(ROOT, 'fastlane', 'metadata', 'android', 'en-US', 'images', 'icon.png')
ICO = os.path.join(ROOT, 'packaging', 'windows', 'brevier.ico')
ICO_SIZES = (16, 20, 24, 32, 40, 48, 64, 128, 256)
PAPER = (0xfa, 0xf5, 0xea, 255)
DENSITIES = {'mdpi': 1.0, 'hdpi': 1.5, 'xhdpi': 2.0, 'xxhdpi': 3.0, 'xxxhdpi': 4.0}


def check(text):
    """Что не так со знаком; пустой список — всё в порядке."""
    problems = []
    box = re.search(r'viewBox="\s*0\s+0\s+([\d.]+)\s+([\d.]+)\s*"', text)
    if not box or box.group(1) != box.group(2):
        problems.append('холст не квадратный (viewBox)')
    if re.search(r'<(svg:)?text\b', text):
        problems.append('в знаке живой текст: букву надо оконтурить (Path → Object to Path)')
    gradients = re.findall(r'<linearGradient\b[^>]*>', text)
    if len(gradients) != 1:
        problems.append(f'градиентов {len(gradients)}, а нужен один')
    if any('href=' in g for g in gradients) or 'gradientTransform' in text:
        problems.append('градиент не плоский: ссылка на соседний или gradientTransform')
    return problems


def render(px, tmp):
    from PIL import Image

    out = os.path.join(tmp, f'{px}.png')
    subprocess.run(['inkscape', SVG, '--export-type=png', f'--export-filename={out}',
                    '-w', str(px), '-h', str(px)], check=True, capture_output=True)
    return Image.open(out).convert('RGBA')


def centered(field, glyph):
    # Pillow нужен только для рисования: `--check` в CI обходится без него.
    field.alpha_composite(glyph, ((field.size[0] - glyph.size[0]) // 2,
                                  (field.size[1] - glyph.size[1]) // 2))
    return field


def foreground(scale, tmp):
    from PIL import Image

    size = round(108 * scale)
    return centered(Image.new('RGBA', (size, size), (0, 0, 0, 0)), render(round(56 * scale), tmp))


def legacy(scale, tmp):
    from PIL import Image, ImageDraw

    size = round(48 * scale)
    # Скругление рисуем вчетверо крупнее и ужимаем: край без лесенки.
    big = size * 4
    mask = Image.new('L', (big, big), 0)
    ImageDraw.Draw(mask).rounded_rectangle((0, 0, big - 1, big - 1), radius=round(big * 8.4 / 48), fill=255)
    paper = Image.new('RGBA', (size, size), PAPER)
    paper.putalpha(mask.resize((size, size), Image.LANCZOS))
    return centered(paper, render(int(32.5 * scale), tmp))


def store(tmp):
    from PIL import Image

    return centered(Image.new('RGBA', (512, 512), PAPER), render(round(512 * 56 / 72), tmp))


def windows(tmp):
    images = [render(px, tmp) for px in ICO_SIZES]
    images[-1].save(ICO, sizes=[image.size for image in images], append_images=images[:-1])


def main():
    problems = check(open(SVG, encoding='utf-8').read())
    if problems:
        for problem in problems:
            print(f'{SVG}: {problem}', file=sys.stderr)
        sys.exit(1)
    if '--check' in sys.argv[1:]:
        return
    with tempfile.TemporaryDirectory() as tmp:
        for name, scale in DENSITIES.items():
            folder = os.path.join(RES, f'mipmap-{name}')
            foreground(scale, tmp).save(os.path.join(folder, 'ic_launcher_foreground.png'), optimize=True)
            legacy(scale, tmp).save(os.path.join(folder, 'ic_launcher.png'), optimize=True)
        store(tmp).convert('RGB').save(STORE, optimize=True)
        windows(tmp)
    print('android/app/src/main/res/mipmap-*/ic_launcher{,_foreground}.png, fastlane …/icon.png,'
          ' packaging/windows/brevier.ico')


if __name__ == '__main__':
    main()
