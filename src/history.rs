//! История вперёд-назад.
//!
//! Ровно та же модель, что у браузеров: переход с середины истории обрубает
//! всё, что было впереди. Иначе «назад, назад, открыть ссылку, вперёд» ведёт
//! в страницу, которую читатель не выбирал.

use crate::address::Address;

#[derive(Debug, Default)]
pub struct History {
    entries: Vec<Address>,
    at: usize,
}

impl History {
    pub fn new() -> Self {
        Self::default()
    }

    pub fn current(&self) -> Option<&Address> {
        self.entries.get(self.at)
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
            self.at += 1;
        }
        self.entries.push(address);
        self.at = self.entries.len() - 1;
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
    fn reloading_the_same_page_is_not_a_new_entry() {
        let mut history = History::new();
        history.visit(web("https://a.test/"));
        history.visit(web("https://a.test/"));
        assert!(!history.can_go_back());
    }
}
