//! История вперёд-назад.
//!
//! Ровно та же модель, что у браузеров: переход с середины истории обрубает
//! всё, что было впереди. Иначе «назад, назад, открыть ссылку, вперёд» ведёт
//! в страницу, которую читатель не выбирал.
//!
//! Корень истории вкладки — начальная страница. Вкладка, открытая
//! сразу на странице (ссылкой, набором адреса), иначе стояла бы с погасшей
//! «назад», и деться с неё было бы некуда, кроме адресной строки. Кнопка
//! при этом остаётся стрелкой: кнопка, которая по состоянию делает разное,
//! сбивает руку (#26).
//!
//! Кроме вкладки, открытой адресом из другой программы (`from_outside`):
//! у неё корень — сама эта страница, и «назад» с неё гаснет. Читатель
//! пришёл из ленты новостей или чата, по Brevier не ходил, и живая стрелка
//! говорила ему обратное — будто за страницей есть что-то, им открытое.
//! На телефоне системный «назад» с такой страницы возвращает туда, откуда
//! пришла ссылка. Решено мейнтейнером 2 октября 2026.

use crate::address::Address;

#[derive(Debug, Default)]
pub struct History {
    entries: Vec<Address>,
    /// Место чтения на каждой странице пути — смещение в буфере, по одному
    /// на запись `entries`. По нему «назад» возвращает читателя туда, где он
    /// стоял, а не в начало страницы. Смещение, а не пиксели: оно не зависит
    /// ни от ширины окна, ни от масштаба.
    places: Vec<i32>,
    /// Где читатель в пути. `None` — на начальной странице, перед первой
    /// записью: туда ведёт «назад» с первой страницы вкладки.
    at: Option<usize>,
    /// Вкладку открыл адрес из другой программы: начальной страницы
    /// за первой записью нет.
    outside: bool,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    /// История вкладки, которую открыл адрес из другой программы: «назад»
    /// с первой страницы гаснет, а не ведёт на начальную.
    pub fn from_outside() -> Self {
        Self {
            outside: true,
            ..Self::default()
        }
    }

    /// История, поднятая с диска: список адресов и место в нём.
    /// Заведена ради восстановления сессии — вкладка обязана вернуться
    /// не только на свою страницу, но и со своими «назад» и «вперёд».
    pub fn restored(entries: Vec<Address>, at: usize) -> Self {
        let at = (!entries.is_empty()).then(|| at.min(entries.len() - 1));
        // Мест на диске сессия не хранит (у текущей страницы место едет
        // отдельным полем), поэтому восстановленные записи начинают с нуля.
        let places = vec![0; entries.len()];
        // Откуда вкладка пришла, сессия не помнит: поднятая вкладка —
        // уже вкладка Brevier, и корень у неё обычный.
        Self {
            entries,
            places,
            at,
            outside: false,
        }
    }

    /// Открытая страница; `None` — вкладка на начальной странице.
    pub fn current(&self) -> Option<&Address> {
        self.at.and_then(|at| self.entries.get(at))
    }

    /// Весь путь вкладки — то, что кладётся в сессию.
    pub fn entries(&self) -> &[Address] {
        &self.entries
    }

    /// Место в пути; `None` — начальная страница.
    pub fn at(&self) -> Option<usize> {
        self.at
    }

    /// Стоит ли вкладка на первой странице своего пути — там, откуда
    /// «назад» ведёт на начальную страницу (если ведёт: `can_go_back`).
    pub fn at_first(&self) -> bool {
        self.at == Some(0)
    }

    pub fn is_empty(&self) -> bool {
        self.entries.is_empty()
    }

    /// Открыть адрес. Повтор текущего адреса записью не считается — иначе
    /// перезагрузка страницы забивала бы историю. С начальной страницы
    /// впереди весь путь, и обрубается он весь.
    pub fn visit(&mut self, address: Address) {
        if self.current() == Some(&address) {
            return;
        }
        let kept = self.at.map_or(0, |at| at + 1);
        self.entries.truncate(kept);
        self.places.truncate(kept);
        self.entries.push(address);
        self.places.push(0);
        self.at = Some(self.entries.len() - 1);
    }

    /// Запомнить место чтения на текущей странице — чтобы вернуться сюда,
    /// когда читатель пойдёт «назад» или «вперёд». Зовётся перед самим шагом,
    /// пока `at` ещё указывает на покидаемую страницу. Место начальной
    /// страницы не помним: она короткая, и её собирают заново.
    pub fn set_place(&mut self, place: i32) {
        if let Some(slot) = self.at.and_then(|at| self.places.get_mut(at)) {
            *slot = place;
        }
    }

    /// Место чтения на текущей странице, если оно было запомнено; иначе ноль
    /// — начало страницы.
    pub fn place(&self) -> i32 {
        self.at
            .and_then(|at| self.places.get(at))
            .copied()
            .unwrap_or(0)
    }

    /// Открыта страница — значит, за ней есть куда вернуться: хотя бы
    /// на начальную. Кроме первой страницы вкладки, открытой снаружи.
    pub fn can_go_back(&self) -> bool {
        match self.at {
            Some(0) => !self.outside,
            Some(_) => true,
            None => false,
        }
    }

    pub fn can_go_forward(&self) -> bool {
        self.at.map_or(0, |at| at + 1) < self.entries.len()
    }

    /// Шаг назад. `false` — идти некуда; куда пришли, говорит `current`:
    /// `None` там — начальная страница.
    pub fn back(&mut self) -> bool {
        if !self.can_go_back() {
            return false;
        }
        // С первой страницы — на начальную: `0 - 1` и есть `None`.
        self.at = self.at.and_then(|at| at.checked_sub(1));
        true
    }

    /// Шаг вперёд, с начальной страницы — на первую запись пути.
    pub fn forward(&mut self) -> bool {
        if !self.can_go_forward() {
            return false;
        }
        self.at = Some(self.at.map_or(0, |at| at + 1));
        true
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
        assert!(!history.back());
        assert!(!history.forward());
    }

    #[test]
    fn back_and_forward_walk_the_same_path() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.visit(web("https://b.test/"));
        history.visit(web("https://c.test/"));

        assert!(history.back());
        assert_eq!(history.current(), Some(&web("https://b.test/")));
        assert!(history.back());
        assert_eq!(history.current(), Some(&web("https://a.test/")));
        assert!(history.forward());
        assert_eq!(history.current(), Some(&web("https://b.test/")));
    }

    #[test]
    fn the_first_page_goes_back_to_the_start_page() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        assert!(history.at_first());
        assert!(history.can_go_back());

        // Назад с первой страницы — на начальную, и оттуда уже некуда.
        assert!(history.back());
        assert!(history.current().is_none());
        assert_eq!(history.at(), None);
        assert!(!history.can_go_back());
        assert!(!history.back());

        // Путь при этом цел: вперёд — снова на первую страницу.
        assert!(history.can_go_forward());
        assert!(history.forward());
        assert_eq!(history.current(), Some(&web("https://a.test/")));
    }

    #[test]
    fn a_tab_opened_from_outside_has_no_start_page_behind() {
        let mut history = History::from_outside();
        history.visit(web("https://news.test/story"));
        assert!(history.at_first());
        assert!(!history.can_go_back());
        assert!(!history.back());
        assert_eq!(history.current(), Some(&web("https://news.test/story")));

        // Дальше по ссылкам — обычный путь, и «назад» ведёт до первой
        // страницы, но не за неё.
        history.visit(web("https://news.test/more"));
        assert!(history.can_go_back());
        assert!(history.back());
        assert_eq!(history.current(), Some(&web("https://news.test/story")));
        assert!(!history.back());
        assert!(history.forward());
        assert_eq!(history.current(), Some(&web("https://news.test/more")));
    }

    #[test]
    fn a_page_opened_from_the_start_page_cuts_off_the_whole_path() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.visit(web("https://b.test/"));
        history.back();
        history.back();
        history.visit(web("https://c.test/"));

        assert_eq!(history.entries(), &[web("https://c.test/")]);
        assert!(!history.can_go_forward());
        assert!(history.at_first());
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
        history.back();
        assert_eq!(history.current(), Some(&web("https://a.test/")));
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
        history.forward();
        assert_eq!(history.current(), Some(&web("https://c.test/")));

        // Место вне списка — не повод падать: файл сессии правят руками,
        // и он приезжает каким угодно.
        let short = History::restored(vec![web("https://a.test/")], 9);
        assert_eq!(short.current(), Some(&web("https://a.test/")));
        let empty = History::restored(Vec::new(), 3);
        assert!(empty.current().is_none());
        assert!(!empty.can_go_back());
    }

    #[test]
    fn a_history_hands_out_what_it_holds() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.visit(web("https://b.test/"));
        history.back();
        assert_eq!(history.entries().len(), 2);
        assert_eq!(history.at(), Some(0));
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

        // Начальная страница своего места не держит и чужого не затирает.
        history.back();
        history.back();
        history.set_place(999);
        assert_eq!(history.place(), 0);
        history.forward();
        assert_eq!(history.place(), 120);
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
        assert_eq!(history.entries().len(), 1);
        assert!(history.at_first());
    }
}
