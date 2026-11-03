//! Коллекция шрифтов (`.ttc`) из отдельных файлов — для Windows.
//!
//! Pango на Windows каждый `add_font_file` оплачивает двумя полными
//! перестройками карты шрифтов, и каждая заново перечисляет все шрифты
//! системы (`pango_win32_font_map_cache_clear` из `add_font_file`
//! и ещё раз из `pango_font_map_changed`, Pango 1.58). Семь файлов — это
//! четырнадцать перестроек, на ноутбуке с сотнями шрифтов — 2,4 с запуска.
//! Коллекция — один файл со всеми начертаниями: один вызов, две перестройки.
//!
//! Формат простой: заголовок `ttcf` со смещениями шрифтов, за ним сами
//! шрифты как есть. Смещения таблиц внутри шрифта в коллекции считаются
//! от начала файла, а не шрифта, поэтому их сдвигаем на начало шрифта.
//! Таблицы не делим между шрифтами: выигрыш в размере не нужен — файл
//! живёт в кэше, а не едет по сети.

/// Собрать коллекцию. `None` — какой-то из файлов не шрифт sfnt.
pub fn collection(fonts: &[&[u8]]) -> Option<Vec<u8>> {
    let header = 12 + 4 * fonts.len();
    let mut out =
        Vec::with_capacity(header + fonts.iter().map(|font| font.len() + 3).sum::<usize>());
    out.extend_from_slice(b"ttcf");
    out.extend_from_slice(&1u16.to_be_bytes());
    out.extend_from_slice(&0u16.to_be_bytes());
    out.extend_from_slice(&u32::try_from(fonts.len()).ok()?.to_be_bytes());
    out.resize(header, 0);

    for (index, font) in fonts.iter().enumerate() {
        // Шрифт — с границы четырёх байт, как требует формат.
        out.resize(out.len().next_multiple_of(4), 0);
        let base = out.len();
        let tables = tables(font)?;
        out.extend_from_slice(font);
        let shift = u32::try_from(base).ok()?;
        for table in 0..tables {
            let at = base + 12 + 16 * table + 8;
            let offset = u32::from_be_bytes(out[at..at + 4].try_into().ok()?);
            out[at..at + 4].copy_from_slice(&offset.checked_add(shift)?.to_be_bytes());
        }
        let slot = 12 + 4 * index;
        out[slot..slot + 4].copy_from_slice(&shift.to_be_bytes());
    }
    Some(out)
}

/// Число таблиц шрифта, если его каталог таблиц цел и не выходит за файл.
fn tables(font: &[u8]) -> Option<usize> {
    let version = font.get(0..4)?;
    if version != [0, 1, 0, 0] && version != b"OTTO" && version != b"true" {
        return None;
    }
    let count = usize::from(u16::from_be_bytes(font.get(4..6)?.try_into().ok()?));
    for table in 0..count {
        let record = font.get(12 + 16 * table..12 + 16 * (table + 1))?;
        let offset = u32::from_be_bytes(record[8..12].try_into().ok()?) as usize;
        let length = u32::from_be_bytes(record[12..16].try_into().ok()?) as usize;
        if offset.checked_add(length)? > font.len() {
            return None;
        }
    }
    Some(count)
}

#[cfg(test)]
mod tests {
    use super::*;

    fn word(bytes: &[u8], at: usize) -> usize {
        u32::from_be_bytes(bytes[at..at + 4].try_into().unwrap()) as usize
    }

    /// Каждая таблица каждого шрифта в коллекции — побайтно та же, что
    /// в отдельном файле: сдвинулись только адреса.
    #[test]
    fn every_table_of_every_font_stays_the_same() {
        let fonts: Vec<&[u8]> = vec![
            include_bytes!("../../assets/fonts/NotoSans-Regular.ttf"),
            include_bytes!("../../assets/fonts/NotoSans-Italic.ttf"),
            include_bytes!("../../assets/fonts/NotoSansMono-Regular.ttf"),
        ];
        let ttc = collection(&fonts).unwrap();
        assert_eq!(&ttc[0..4], b"ttcf");
        assert_eq!(word(&ttc, 8), fonts.len());
        for (index, font) in fonts.iter().enumerate() {
            let base = word(&ttc, 12 + 4 * index);
            assert_eq!(base % 4, 0);
            let count = tables(font).unwrap();
            assert_eq!(&ttc[base + 4..base + 6], &font[4..6]);
            for table in 0..count {
                let record = 12 + 16 * table;
                let (offset, length) = (word(font, record + 8), word(font, record + 12));
                let moved = word(&ttc, base + record + 8);
                assert_eq!(moved, offset + base);
                assert_eq!(&ttc[moved..moved + length], &font[offset..offset + length]);
            }
        }
    }

    #[test]
    fn something_else_is_not_collected() {
        assert_eq!(collection(&[b"not a font at all"]), None);
        // Каталог обещает таблицу за концом файла.
        let mut broken = vec![0, 1, 0, 0, 0, 1, 0, 0, 0, 0, 0, 0];
        broken.extend_from_slice(b"head\0\0\0\0\0\0\0\x1c\0\0\0\x36");
        assert_eq!(collection(&[&broken]), None);
    }
}
