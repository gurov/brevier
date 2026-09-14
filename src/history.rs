//! История вперёд-назад.
//!
//! Ровно та же модель, что у браузеров: переход с середины истории обрубает
//! всё, что было впереди. Иначе «назад, назад, открыть ссылку, вперёд» ведёт
//! в страницу, которую читатель не выбирал.

use crate::address::Address;

#[derive(Debug, Default)]
pub struct History {
    entries: Vec<Address>,
    /// Место чтения на каждой странице пути — смещение в буфере, по одному
    /// на запись `entries`. По нему «назад» возвращает читателя туда, где он
    /// стоял, а не в начало страницы. Смещение, а не пиксели: оно не зависит
    /// ни от ширины окна, ни от масштаба.
    places: Vec<i32>,
    at: usize,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// История, поднятая с диска: список адресов и место в нём.
    /// Заведена ради восстановления сессии — вкладка обязана вернуться
    /// не только на свою страницу, но и со своими «назад» и «вперёд».
    pub fn restored(entries: Vec<Address>, at: usize) -> Self {
        let at = at.min(entries.len().saturating_sub(1));
        // Мест на диске сессия не хранит (у текущей страницы место едет
        // отдельным полем), поэтому восстановленные записи начинают с нуля.
        let places = vec![0; entries.len()];
        Self {
            entries,
            places,
            at,
        }
    }

    pub fn current(&self) -> Option<&Address> {
        self.entries.get(self.at)
    }

    /// Весь путь вкладки и место в нём — то, что кладётся в сессию.
    pub fn entries(&self) -> &[Address] {
        &self.entries
    }

    pub fn at(&self) -> usize {
        self.at
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Открыть адрес. Повтор текущего адреса записью не считается — иначе
    /// перезагрузка страницы забивала бы историю.
    pub fn visit(&mut self, address: Address) {
        if self.current() == Some(&address) {
            return;
        }
        if !self.entries.is_empty() {
            self.entries.truncate(self.at + 1);
            self.places.truncate(self.at + 1);
            self.at += 1;
        }
        self.entries.push(address);
        self.places.push(0);
        self.at = self.entries.len() - 1;
    }

    /// Запомнить место чтения на текущей странице — чтобы вернуться сюда,
    /// когда читатель пойдёт «назад» или «вперёд». Зовётся перед самим шагом,
    /// пока `at` ещё указывает на покидаемую страницу.
    pub fn set_place(&mut self, place: i32) {
        if let Some(slot) = self.places.get_mut(self.at) {
            *slot = place;
        }
    }

    /// Место чтения на текущей странице, если оно было запомнено; иначе ноль
    /// — начало страницы.
    pub fn place(&self) -> i32 {
        self.places.get(self.at).copied().unwrap_or(0)
    }

    pub fn can_go_back(&self) -> bool {
        self.at > 0
    }

    pub fn can_go_forward(&self) -> bool {
        self.at + 1 < self.entries.len()
    }

    pub fn back(&mut self) -> Option<&Address> {
        if !self.can_go_back() {
            return None;
        }
        self.at -= 1;
        self.current()
    }

    pub fn forward(&mut self) -> Option<&Address> {
        if !self.can_go_forward() {
            return None;
        }
        self.at += 1;
        self.current()
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    fn web(url: &str) -> Address {
        Address::Web(url.to_owned())
    }

    #[test]
    fn empty_history_goes_nowhere() {
        let mut history = History::new();
        assert!(history.current().is_none());
        assert!(history.back().is_none());
        assert!(history.forward().is_none());
    }

    #[test]
    fn back_and_forward_walk_the_same_path() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.visit(web("https://b.test/"));
        history.visit(web("https://c.test/"));

        assert_eq!(history.back(), Some(&web("https://b.test/")));
        assert_eq!(history.back(), Some(&web("https://a.test/")));
        assert!(!history.can_go_back());
        assert_eq!(history.forward(), Some(&web("https://b.test/")));
    }

    #[test]
    fn a_new_page_cuts_off_what_was_ahead() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.visit(web("https://b.test/"));
        history.back();
        history.visit(web("https://c.test/"));

        assert!(!history.can_go_forward());
        assert_eq!(history.current(), Some(&web("https://c.test/")));
        assert_eq!(history.back(), Some(&web("https://a.test/")));
    }

    #[test]
    fn a_restored_history_walks_both_ways() {
        let entries = vec![
            web("https://a.test/"),
            web("https://b.test/"),
            web("https://c.test/"),
        ];
        let mut history = History::restored(entries, 1);
        assert_eq!(history.current(), Some(&web("https://b.test/")));
        assert!(history.can_go_back() && history.can_go_forward());
        assert_eq!(history.forward(), Some(&web("https://c.test/")));

        // Место вне списка — не повод падать: файл сессии правят руками,
        // и он приезжает каким угодно.
        let short = History::restored(vec![web("https://a.test/")], 9);
        assert_eq!(short.current(), Some(&web("https://a.test/")));
        assert!(History::restored(Vec::new(), 3).current().is_none());
    }

    #[test]
    fn a_history_hands_out_what_it_holds() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.visit(web("https://b.test/"));
        history.back();
        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.at(), 0);
    }

    #[test]
    fn each_page_keeps_its_reading_place() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.set_place(120);
        history.visit(web("https://b.test/"));
        history.set_place(340);

        // Назад — и место страницы a возвращается.
        history.back();
        assert_eq!(history.place(), 120);
        // Вперёд — место страницы b на месте.
        history.forward();
        assert_eq!(history.place(), 340);
    }

    #[test]
    fn a_new_page_drops_the_places_that_were_ahead() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.set_place(50);
        history.visit(web("https://b.test/"));
        history.set_place(60);
        history.back();
        // Уходим в сторону — «вперёд» и его место обрублены вместе.
        history.visit(web("https://c.test/"));
        assert_eq!(history.place(), 0);
        history.back();
        assert_eq!(history.place(), 50);
        assert_eq!(history.entries().len(), 2);
    }

    #[test]
    fn reloading_the_same_page_is_not_a_new_entry() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.visit(web("https://a.test/"));
        assert!(!history.can_go_back());
    }
}
