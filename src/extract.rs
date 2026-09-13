//! Извлечение основного содержимого — порт Mozilla Readability.
//!
//! Это самое хрупкое место продукта: сайты меняют разметку, эвристики протухают.
//! Поэтому вывод отсюда фиксируется в корпусе эталонов и служит базой
//! регрессионных тестов на всю жизнь проекта.

use std::collections::{HashMap, HashSet};

use dom_query::{Document, NodeRef};
use dom_smoothie::{Config, Readability, ReadabilityError};
use url::Url;

use crate::error::Error;

pub struct Article {
    pub title: String,
    pub byline: Option<String>,
    /// Очищенный HTML статьи. Относительные ссылки уже развёрнуты в абсолютные.
    pub content_html: String,
    /// Миниатюры записей: адрес ссылки → адрес картинки внутри неё.
    /// Собраны со всей страницы, до извлечения; подставляются обратно
    /// только в ленте — см. [`thumbs`].
    pub thumbs: HashMap<String, String>,
    /// Лента страницы — если страница оказалась лентой, а извлечение
    /// принесло из неё одну карточку. HTML с абсолютными адресами,
    /// см. [`listing`].
    pub listing_html: Option<String>,
    /// Сноски статьи, снятые с дерева до конвертации, — см. [`notes`].
    pub notes: Notes,
    /// Навигация самого сайта: то, что лежит в его шапке, меню и подвале.
    /// Не текст статьи и в него не идёт — см. [`site`].
    pub site: Vec<Link>,
}

/// Строка навигации: подпись и адрес.
#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Link {
    pub title: String,
    pub address: String,
}

/// Сноски статьи в едином виде.
///
/// Тела лежат html-ом, а не markdown-ом, намеренно: перевод в markdown —
/// работа конвертера, и делать её дважды в двух модулях незачем.
#[derive(Debug, Clone, Default)]
pub struct Notes {
    /// id цели → номер сноски. Номера идут в порядке первой ссылки
    /// в тексте, а не в порядке списка в конце: читателю сноска
    /// встречается там.
    pub numbers: HashMap<String, usize>,
    /// Тела сносок: `bodies[0]` — сноска номер один.
    pub bodies: Vec<String>,
}

/// Сколько карточек делают страницу лентой.
///
/// Восемь, а не три: три карточки бывают и в хвосте статьи («читайте ещё»),
/// и шесть бывают — у spectrum.ieee.org ровно шесть. Лента же начинается
/// от десятка: у habr их девятнадцать, у блога на главной — десять.
const MIN_CARDS: usize = 8;

/// Какую долю ленты может занимать извлечённое, чтобы считаться записью
/// из неё, а не статьёй, к которой ленту приложили сбоку.
///
/// Замер 11 сентября 2026, шесть страниц: у ленты habr извлечённое — 17%
/// от суммы карточек, у статьи spectrum с шестью анонсами — 73%,
/// у stratechery — 75%, у alistapart — 400%. Порог посередине пустоты.
const CARD_SHARE: f32 = 0.4;

/// До этого размера картинка — значок, а не изображение. Где именно
/// это значит «выбросить», решает место: см. [`deicon`].
const ICON_PX: u32 = 48;

/// Сколько шагов наверх искать язык блока кода: `<code>` внутри `<pre>`
/// внутри обёртки.
const LANG_DEPTH: usize = 3;

/// Короче этого абзац для сверки не годится: по трём словам не отличить
/// один абзац от другого.
const KEY_WORDS: usize = 6;

/// По скольким знакам начала узнаём абзац. Извлечение текст абзаца
/// не переписывает, поэтому начала хватает.
const KEY_LEN: usize = 60;

/// Короче этого выпавший абзац не возвращаем: это подпись, дата
/// или обрезок интерфейса, а не проза.
const ORPHAN_WORDS: usize = 8;

/// А из врезки — только настоящий текст. Замер 11 сентября 2026: во врезках
/// лежат и абзацы статьи (сноски и вставки Фаулера, 43–97 слов), и рекламные
/// блюрбы (alistapart, nngroup, smashing — 10–36 слов). Ни `aside`,
/// ни доля ссылок их не различают, а длина различает.
const ASIDE_WORDS: usize = 40;

/// Куда абзац попадает не как часть статьи: во врезку, в подвал, в меню,
/// в форму, в подпись к картинке.
const ASIDE_TAGS: [&str; 5] = ["aside", "footer", "nav", "form", "figure"];

/// Заголовок длиннее этого — не заголовок, а абзац, которому забыли сменить
/// тег: такие попадаются в старой вёрстке, и возвращать их незачем.
const HEAD_CHARS: usize = 120;

/// Всё, что бывает заголовком раздела.
const HEAD_TAGS: &str = "h1, h2, h3, h4, h5, h6";

/// Чем бывает тело сноски. Только блок: ссылка-номер ведёт и на `<sup>`
/// в самом тексте — так устроена обратная ссылка у википедии, — а телом
/// сноски `<sup>` не бывает нигде.
const NOTE_TAGS: [&str; 6] = ["li", "p", "dd", "div", "td", "blockquote"];

/// Длиннее этого номер сноски не бывает: три знака — это 999, а дальше
/// начинается не сноска, а год или сумма.
const NOTE_DIGITS: usize = 3;

/// Лента, снятая с исходного дерева: её html и её же текст — по тексту
/// потом решают, лента это или статья с витриной в хвосте.
struct Listing {
    html: String,
    /// Текст ленты целиком: извлечённое должно лежать внутри неё, иначе
    /// это статья, а лента — виджет сбоку от неё.
    text: String,
    /// Сколько текста в записях. Складывается по карточкам, а не берётся
    /// у ленты: между карточками лежит и обвязка, которая в счёт не идёт.
    weight: usize,
    cards: usize,
}

pub fn extract(html: &str, url: &str) -> Result<Article, Error> {
    let cfg = Config::default();

    let doc = Document::from(html);
    unlazy(&doc);
    deicon(&doc);
    keep_lang(&doc);
    unglue(&doc);
    unprint(&doc);
    unbutton(&doc);
    unpermalink(&doc);
    // Снять до извлечения: `Readability` документ перебирает и чистит,
    // и половины картинок после него в дереве уже нет.
    let thumbs = thumbs(&doc, url);
    let navigation = site(&doc, url);
    let listing = listing(&doc, url);
    // Копия — под сверку с извлечённым: `Readability` документ забирает себе
    // и чистит на месте, а сироты ищутся в исходном дереве (см. `restore`).
    let source = doc.clone();

    let mut readability =
        Readability::with_document(doc, Some(url), Some(cfg)).map_err(|e| match e {
            ReadabilityError::BadDocumentURL => Error::BadUrl(url.to_owned()),
            _ => Error::EmptyExtraction,
        })?;

    let article = readability.parse().map_err(|_| Error::EmptyExtraction)?;

    if article.content.trim().is_empty() {
        return Err(Error::EmptyExtraction);
    }

    let content_html = article.content.to_string();
    let listing_html = listing
        .filter(|listing| listing.holds(&content_html))
        .map(|listing| listing.html);

    // Сироты возвращаем только в статью: в ленте абзацев статьи нет,
    // а решение «лента или статья» принято по строгому содержимому.
    let content_html = if listing_html.is_none() {
        restore(&source, &content_html, url, &article.title).unwrap_or(content_html)
    } else {
        content_html
    };

    // Заглавная картинка — последней добавкой и только в статью без единой
    // своей: в ленте карточки несут миниатюры сами.
    let content_html = match &listing_html {
        Some(_) => content_html,
        None => with_cover(&source, content_html, url),
    };

    // Сноски снимаем последними: к этому времени статья собрана целиком,
    // вместе с вернувшейся прозой, и ссылка со своей целью наконец лежат
    // в одном дереве.
    let (content_html, notes) = lift_notes(content_html);

    Ok(Article {
        title: article.title.to_string(),
        byline: article.byline.filter(|b| !b.trim().is_empty()),
        content_html,
        thumbs,
        listing_html,
        notes,
        site: navigation,
    })
}

/// Где лежит навигация сайта.
///
/// Тега `<nav>` мало: он моложе половины живого веба, и у 4pda, tonsky
/// и opennet меню свёрстано списком без него вовсе. Поэтому к тегам
/// добавлено имя — `menu` или `nav` в классе или в `id`. Это такое же
/// соглашение, как `language-*` у блоков кода или `noprint` у печати:
/// не правило под сайт, а способ, которым веб называет эту вещь.
const NAV_TAGS: &str = "nav a[href], footer a[href], [role=\"navigation\"] a[href], \
[class*=\"menu\" i] a[href], [class*=\"nav\" i] a[href], [id*=\"menu\" i] a[href], \
[id*=\"nav\" i] a[href], [class*=\"footer\" i] a[href], [id*=\"footer\" i] a[href]";

/// Подпись пункта меню.
///
/// Пункт бывает в два этажа — название и пояснение под ним
/// (`<a><div>Смартфоны</div><div>Новости, анонсы…</div></a>` у 4pda), —
/// и текстом это одна слипшаяся строка: пробела в разметке нет, разводит
/// их CSS. В меню нужен первый этаж: он и есть название раздела.
fn label(link: &NodeRef) -> String {
    let first = link
        .children_it(false)
        .find(|child| child.is_element())
        .map(|child| squeeze(&child.text()))
        .filter(|text| !text.is_empty());

    match first {
        Some(text) => text,
        None => squeeze(&link.text()),
    }
}

/// Навигация сайта — та, что у него в меню и в подвале.
///
/// Без JS страница остаётся тем, чем была, — набором ссылок, — и лишать
/// читателя этого набора значит отнимать у него сайт: с главной некуда
/// пойти, из статьи не подняться в раздел. Именно этим и берут текстовые
/// браузеры, у которых меню сайта видно всегда.
///
/// В текст статьи это не идёт и идти не должно: меню посреди прозы — дефект
/// по рубрике, и справедливо. Поэтому навигация едет рядом с документом
/// отдельным списком, а показывать ли её и где — дело интерфейса.
///
/// Что считается навигацией, решает форма: `<nav>`, `role="navigation"`,
/// подвал. Шапка целиком не годится — в ней у половины сайтов лежит
/// заголовок статьи и подпись автора, поэтому из неё берём только меню.
///
/// Дальше три отсева, и каждый нужен:
///
/// - **свой хост**: «мы в телеграме» и «сделано в студии» уводят с сайта,
///   а навигация — это про то, где ещё побывать здесь;
/// - **подпись короткая и не пустая**: ссылка-значок подписи не несёт,
///   а ссылка в абзац длиной — это не пункт меню, а текст, случайно
///   оказавшийся в подвале;
/// - **один адрес — одна строка**: меню на странице лежит дважды, для
///   широкого экрана и для узкого, и оба раза целиком.
fn site(doc: &Document, url: &str) -> Vec<Link> {
    const MAX_TITLE: usize = 40;
    const MAX_ROWS: usize = 60;

    let base = Url::parse(url).ok();
    let host = base
        .as_ref()
        .and_then(|url| url.host_str())
        .map(str::to_owned);
    let here = base.as_ref().map(|url| {
        let mut url = url.clone();
        url.set_fragment(None);
        url.to_string()
    });

    let mut seen: HashSet<String> = HashSet::new();
    let mut rows: Vec<Link> = Vec::new();

    for link in doc.select(NAV_TAGS).nodes() {
        if rows.len() >= MAX_ROWS {
            break;
        }
        let Some(href) = link.attr("href") else {
            continue;
        };
        let Some(address) = absolute(base.as_ref(), &href) else {
            continue;
        };
        let Ok(parsed) = Url::parse(&address) else {
            continue;
        };
        if !matches!(parsed.scheme(), "http" | "https") {
            continue;
        }
        if host.as_deref() != parsed.host_str() {
            continue;
        }
        // Ссылка на страницу, на которой читатель и стоит, никуда не ведёт.
        if here.as_deref() == Some(address.trim_end_matches('#')) {
            continue;
        }
        let title = label(link);
        if title.is_empty() || title.chars().count() > MAX_TITLE {
            continue;
        }
        if !seen.insert(address.clone()) {
            continue;
        }
        rows.push(Link { title, address });
    }

    rows
}

impl Listing {
    /// Лежит ли извлечённое внутри одной из карточек.
    ///
    /// Вопрос ровно один: что нашла Readability — статью или одну запись
    /// из двадцати. Ответ даёт место, а не размер. Текст статьи, у которой
    /// в хвосте витрина «читайте ещё», не лежит ни в одной карточке
    /// витрины; текст выбранной записи лежит в своей карточке целиком.
    ///
    /// Сверять с лентой целиком нельзя: общим предком карточек бывает
    /// и весь `body` — когда заголовок-ссылка нашёлся и в шапке, и в подвале,
    /// — а в нём лежит вообще всё, включая статью. Поэтому сверяем
    /// с отдельной карточкой, и куском из середины: начало занято
    /// заголовком и подписью автора, которые извлечение переписывает.
    fn holds(&self, content_html: &str) -> bool {
        const SAMPLE: usize = 120;

        let inside = squeeze(&Document::from(content_html).text());
        let length = inside.chars().count();
        if self.cards < MIN_CARDS || length < SAMPLE * 2 {
            return false;
        }

        // Статья, к которой ленту приложили сбоку, занимает больше, чем сама
        // лента; запись из ленты — малую её долю.
        if length as f32 > self.weight as f32 * CARD_SHARE {
            return false;
        }

        // И лежать эта запись должна внутри ленты: у статьи с виджетом
        // «читайте ещё» текст лежит мимо него. Берём кусок из середины —
        // начало занято заголовком и подписью автора, которые извлечение
        // переписывает по-своему.
        let from: String = inside.chars().skip(length / 3).collect();
        let sample: String = from.chars().take(SAMPLE).collect();
        self.text.contains(&sample)
    }
}

/// Выбросить значки: картинки, которые изображением не являются.
///
/// Аватар автора, флажок языка, иконка «поделиться», карандашик
/// «исправить в викиданных» — читателю каждая достаётся отдельной строкой
/// `![](…)`, а в окне ещё и рамкой на якоре. Признак по форме: объявлена
/// мелкой с обеих сторон — и стоит там, где картинке быть нечем.
///
/// Мест таких два, и подпись в них значит разное. **Одна в ссылке** — тогда
/// подпись роли не играет: это имя ссылки, а не подпись к картинке
/// (у википедии «Edit this at Wikidata», у medium — имя автора). **Одна
/// в блоке** — тогда только без подписи: подписанная мелкая картинка ещё
/// бывает иллюстрацией, и аватары собеседников у fasterthanli.me остаются.
///
/// Картинку посреди прозы не трогаем вовсе: там мелкая картинка — знак,
/// а не значок. На этом уже ловились: первая версия правила съела логотип
/// apple прямо посреди строки у tonsky.me. Формула цела по той же причине —
/// она стоит в строке, и в `alt` у неё исходник.
fn deicon(doc: &Document) {
    for link in doc.select("a").nodes() {
        if !link.text().trim().is_empty() {
            continue;
        }
        let images: Vec<NodeRef> = link
            .descendants()
            .into_iter()
            .filter(|node| node.node_name().as_deref() == Some("img"))
            .collect();
        let [image] = images.as_slice() else {
            continue;
        };
        if is_icon(image) {
            image.remove_from_parent();
        }
    }

    for image in doc.select("img").nodes() {
        if !is_icon(image) || captioned(image) {
            continue;
        }
        // Одна в блоке: кроме неё в родителе ни слова. Текст рядом
        // означает прозу, а в прозе мелкая картинка — знак.
        if image
            .parent()
            .is_some_and(|parent| parent.text().trim().is_empty())
        {
            image.remove_from_parent();
        }
    }
}

/// Что страница сама исключила из печати. Имена из соглашений, а не
/// из вёрстки одного сайта: `noprint` у MediaWiki и вообще в вебе,
/// `hidden-print` и `d-print-none` у bootstrap.
const PRINT_HIDDEN: &str = ".noprint,.no-print,.hidden-print,.d-print-none";

/// Выбросить то, чего нет на печати.
///
/// Признак по форме и от самой страницы: она объявила, что этот кусок
/// в печатный документ не идёт, — значит это обвязка, а не текст.
/// Так со статьи уходит «Материал из Википедии — свободной энциклопедии»,
/// карандашики правки и навигационные коробки.
///
/// Соседняя мысль у defuddle — скрытое мобильными стилями почти наверняка
/// не текст; там признак тот же по духу, но читать чужой CSS мы не станем
/// принципиально, а класс лежит в самой разметке.
fn unprint(doc: &Document) {
    for node in doc.select(PRINT_HIDDEN).nodes() {
        node.remove_from_parent();
    }
}

/// Кнопка, притворившаяся ссылкой, — не ссылка.
///
/// `href="#"` и `href="javascript:…"` означают одно: нажатие обрабатывает
/// скрипт, а сам адрес не ведёт никуда. Без JS такая ссылка не делает
/// ничего, и предлагать её читателю — обманывать. Особенно густо их вокруг
/// комментариев: у opennet каждая реплика несёт «[+]/[–]», «свернуть ветку»
/// и «к родителю», и в тексте это шесть управляющих ссылок на реплику.
///
/// Текст оставляем: он бывает осмысленным («Показать ещё»), и решать
/// его судьбу — работа правил про обвязку, а не наша. Но если от кнопки
/// остался один значок, читать в нём нечего.
fn unbutton(doc: &Document) {
    const GLYPH_CHARS: usize = 3;

    for link in doc.select("a[href]").nodes() {
        let Some(href) = link.attr("href") else {
            continue;
        };
        let href = href.trim();
        let dead = href == "#" || href.to_lowercase().starts_with("javascript:");
        let text = squeeze(&link.text());
        // Значок вместо текста у ссылки внутрь страницы — та же кнопка,
        // только доехавшая до места: «^» к родителю, «^^» на уровень выше,
        // «···» свернуть ветку. Читать в ней нечего, а в треде их по три
        // на реплику.
        let glyph = text.chars().filter(|c| !c.is_whitespace()).count() <= GLYPH_CHARS
            && !text.chars().any(|c| c.is_alphanumeric());
        if !dead && !(href.starts_with('#') && glyph) {
            continue;
        }
        if glyph {
            unbracket(link);
            unseparate(link);
            link.remove_from_parent();
        } else {
            link.remove_attr("href");
        }
    }
}

/// Скобки, в которые кнопка была одета.
///
/// Старая школа пишет управляющие ссылки в квадратных скобках: `\[^\]`,
/// `\[ответить\]`, `\[править\]`. Скобки лежат текстом рядом с ссылкой,
/// и, убрав кнопку, мы оставляли читателю пустую пару — у opennet по три
/// на каждую реплику.
/// Разделитель, который разделять перестал.
///
/// Кнопки ставят в ряд через косую черту или точку — «[+]/[–]», «[править] ·
/// [удалить]». Убрав кнопки, мы оставляли читателю строку из одних
/// разделителей.
fn unseparate(link: &NodeRef) {
    const SEPARATORS: &str = "/|·•,;–—-";

    for side in [link.prev_sibling(), link.next_sibling()]
        .into_iter()
        .flatten()
    {
        if !side.is_text() {
            continue;
        }
        let text = side.text();
        let bare = text.trim();
        if !bare.is_empty() && bare.chars().all(|c| SEPARATORS.contains(c)) {
            side.remove_from_parent();
        }
    }
}

fn unbracket(link: &NodeRef) {
    let Some(left) = link.prev_sibling() else {
        return;
    };
    let Some(right) = link.next_sibling() else {
        return;
    };
    if !left.is_text() || !right.is_text() {
        return;
    }
    let before = left.text();
    let after = right.text();
    let Some(before) = before.strip_suffix('[') else {
        return;
    };
    let Some(after) = after.strip_prefix(']') else {
        return;
    };
    left.set_text(before);
    right.set_text(after);
}

/// Признаки подписи автора — те же, по которым её ищет Readability
/// (`[rel=author]`, `[itemprop*=author]`, класс или id со словом из списка
/// `byline`/`author`/`dateline`). Правим ровно то, что она потом возьмёт.
const BYLINE: &str = "[rel=\"author\"],[itemprop*=\"author\" i],[class*=\"author\" i],\
[class*=\"byline\" i],[class*=\"dateline\" i],[class*=\"writtenby\" i],\
[id*=\"author\" i],[id*=\"byline\" i],[id*=\"dateline\" i]";

/// Развести подпись автора и дату, слипшиеся на границе элементов.
///
/// `<a>RationalAnswer</a><span>8 сен в 13:42</span>` браузер разводит
/// стилями, а текстом это одна строка «RationalAnswer8 сен в 13:42»:
/// пробела в разметке нет вовсе. CSS мы не читаем принципиально, поэтому
/// пробел ставим сами — но только в подписи.
///
/// В прозе так делать нельзя: там граница элементов сплошь и рядом
/// настоящая — `<b>по</b><i>лу</i>слово`, ссылка внутри слова, дробь
/// из `<sup>` и `<sub>`. Подпись же тем и отличается, что склеены в ней
/// разные сведения: имя, дата, время чтения.
fn unglue(doc: &Document) {
    for byline in doc.select(BYLINE).nodes() {
        for node in byline.descendants() {
            if !node.is_element() {
                continue;
            }
            // Комментарии между элементами не разделитель: в тексте
            // их нет. У habr между именем и датой их три подряд.
            let mut left = node.prev_sibling();
            while left.as_ref().is_some_and(NodeRef::is_comment) {
                left = left.and_then(|node| node.prev_sibling());
            }
            let Some(left) = left.filter(NodeRef::is_element) else {
                continue;
            };

            let ends = left.text().chars().last();
            let starts = node.text().chars().next();
            if let (Some(ends), Some(starts)) = (ends, starts)
                && ends.is_alphanumeric()
                && starts.is_alphanumeric()
            {
                node.before_html(" ");
            }
        }
    }
}

fn is_icon(image: &NodeRef) -> bool {
    let px = |name: &str| {
        image
            .attr(name)
            .and_then(|value| value.trim().parse::<u32>().ok())
    };
    let small = |name: &str| px(name).is_some_and(|size| size <= ICON_PX);

    // Распорка старой вёрстки подходит под значок по всем признакам, но
    // в её ширине записана вложенность треда, и разбирает её конвертер
    // (`markdown::image_handler`). Выбросить её здесь — сплющить тред.
    let spacer = px("height").is_some_and(|height| height <= crate::markdown::SPACER_PX);

    small("width") && small("height") && !spacer
}

fn captioned(image: &NodeRef) -> bool {
    image.attr("alt").is_some_and(|alt| !alt.trim().is_empty())
}

/// Вернуть в статью прозу, выпавшую внутри её же границ.
///
/// Readability выбирает один узел-кандидата и его соседей, а что лежит
/// в стороне — врезка, сноска, абзац в своей обёртке — теряет целиком,
/// и никакими настройками не возвращается: ослабление флагов у самого
/// `dom_smoothie` меняет отбор кандидата, а не добирает потерянное
/// (проверено на шести страницах — у Фаулера все политики дают один
/// и тот же текст). Это и есть «каскад с перезапуском» соседей в том
/// единственном месте, где он работает: сравниваем исходное дерево
/// с извлечённым и возвращаем то, что лежит **между** абзацами статьи.
///
/// Границы и решают. Абзац выше первого взятого — шапка, ниже последнего —
/// подвал и комментарии; между ними сайт обвязку почти не кладёт. Замер
/// по корпусу 11 сентября 2026: сироты нашлись на 12 страницах из 86,
/// и настоящей прозой оказались шесть — два потерянных абзаца
/// доказательства у ru.wikipedia (459 слов), врезки и сноски Фаулера
/// (1111 и 57), куски статьи у tim.blog (1178) и sqlite.org (332),
/// реплики у danluu. На остальных шести сироты — рекламные блюрбы,
/// и от прозы их отделяет длина, а не место: см. `ASIDE_WORDS`.
///
/// Порядок сохраняем, вставляя накопленное одним куском: `after_html`
/// ставит новое сразу за якорем, и два вызова подряд перевернули бы пару
/// абзацев местами.
fn restore(source: &Document, content: &str, base: &str, title: &str) -> Option<String> {
    let article = Document::from(content.to_string());

    let mut places: HashMap<String, NodeRef> = HashMap::new();
    for node in article.select("p").nodes() {
        if let Some((key, _)) = para_key(node) {
            places.entry(key).or_insert(*node);
        }
    }
    if places.is_empty() {
        return None;
    }

    // Текст статьи целиком — под проверку на дубль, и он растёт вместе
    // с возвращённым. Сверять абзацем мало с двух сторон: сноски Фаулера
    // лежат в статье списком, а на странице абзацем внутри врезки, и друг
    // друга по разметке не узнают; сама же врезка на странице стоит дважды —
    // обычная адаптивная вёрстка, копия на узкий экран. И там и там читатель
    // получал один и тот же текст по второму разу.
    let mut inside = squeeze(&article.select("body").text());

    // Заголовки, которые в статье уже есть, — по своему тексту. Сверять их
    // с телом статьи целиком нельзя: строка в два слова находится в прозе
    // слишком легко, и настоящий заголовок пропал бы из-за случайного
    // совпадения.
    let kept: HashSet<String> = article
        .select(HEAD_TAGS)
        .nodes()
        .iter()
        .filter_map(head_key)
        .collect();

    // Картинки статьи — по адресу: тем же ключом узнаётся потерянная.
    let root = Url::parse(base).ok();
    let mut shown: HashSet<String> = article
        .select("img")
        .nodes()
        .iter()
        .filter_map(|node| shot_key(node, root.as_ref()))
        .collect();

    let page: Vec<(NodeRef, Piece)> = source
        .select(&format!("p, img, {HEAD_TAGS}"))
        .nodes()
        .iter()
        .filter_map(|node| piece_of(node, root.as_ref()).map(|piece| (*node, piece)))
        .collect();

    // Границы считаем по абзацам, как и считали: заголовок в шапке страницы
    // или в подвале границей статьи не является.
    let inside_article = |piece: &Piece| match piece {
        Piece::Para(key, _) => places.contains_key(key),
        Piece::Head(_) | Piece::Shot(_) => false,
    };
    let first = page.iter().position(|(_, piece)| inside_article(piece))?;
    let last = page.iter().rposition(|(_, piece)| inside_article(piece))?;

    let mut anchor: Option<NodeRef> = None;
    let mut pending: Vec<String> = Vec::new();
    let mut blocks: HashSet<dom_query::NodeId> = HashSet::new();
    let mut restored = 0;

    let mut heads: HashSet<String> = HashSet::new();
    let mut used: HashSet<dom_query::NodeId> = HashSet::new();

    for (node, piece) in &page[first..=last] {
        let (key, words) = match piece {
            Piece::Para(key, words) => (key, *words),
            // Заголовок, которого в статье нет, встаёт перед ближайшим
            // следующим её абзацем — то есть туда же, куда его накопленным
            // куском поставит якорь. Отдельного места ему не нужно.
            Piece::Head(text) => {
                if !kept.contains(text)
                    && worth_heading(node, text, title)
                    && heads.insert(text.clone())
                {
                    pending.push(fragment(&node.html(), base));
                    restored += 1;
                }
                continue;
            }
            // Иллюстрация, выпавшая внутри статьи, возвращается тем же
            // приёмом, что и проза. Теряется она у Readability регулярно:
            // галерея ixbt лежит в своём `<div>` без текста, и правило
            // про долю ссылок выносит её целиком. Вместе с картинкой
            // поднимаем её `<figure>` — там подпись.
            Piece::Shot(src) => {
                if worth_shot(node) && shown.insert(src.clone()) {
                    let block = lost_shot(node);
                    if blocks.insert(block.id) {
                        pending.push(fragment(&block.html(), base));
                        restored += 1;
                    }
                }
                continue;
            }
        };
        if let Some(place) = places.get(key) {
            // Каждое место в статье служит якорем один раз. Один и тот же
            // абзац на странице встречается дважды — повторённая цитата,
            // копия врезки на узкий экран, — а в статье ему соответствует
            // один узел: второй раз якорь уехал бы назад, и всё следующее
            // за ним встало бы выше по тексту. Поймано на теореме Эрроу:
            // цитата теоремы повторена в двух формулировках, и заголовок
            // «Доказательство» вставал перед разделом, которому предшествует.
            if !used.insert(place.id) {
                continue;
            }
            if let Some(previous) = anchor.take()
                && !pending.is_empty()
            {
                previous.after_html(pending.concat());
                pending.clear();
            }
            anchor = Some(place_after(place));
            continue;
        }
        if !worth_restoring(node, words) || inside.contains(key.as_str()) {
            continue;
        }
        let block = lost_block(node, &places);
        if !blocks.insert(block.id) {
            // Вторая сирота из того же куска: кусок уже возвращён целиком.
            continue;
        }
        pending.push(fragment(&block.html(), base));
        inside.push(' ');
        inside.push_str(&squeeze(&block.text()));
        restored += 1;
    }
    if let Some(previous) = anchor
        && !pending.is_empty()
    {
        previous.after_html(pending.concat());
    }

    if restored == 0 {
        return None;
    }

    // Отдаём в той же форме, в какой пришло: `#readability-page-1` —
    // обёртка самого Readability, и дальше по тракту ждут именно её.
    let page_node = article.select("#readability-page-1");
    Some(match page_node.nodes().first() {
        Some(node) => node.html().to_string(),
        None => article.select("body").inner_html().to_string(),
    })
}

/// Что именно вернуть: сам абзац или обёртку, в которой он потерялся.
///
/// Абзац в `<li>` без своего пункта — половина мысли: у sqlite.org в пункте
/// лежат заголовок и объяснение, и без заголовка объяснение повисает.
/// Цитата, потерянная целиком, должна вернуться цитатой, а не строкой текста.
/// Поэтому поднимаемся на шаг — но только через обёртки известной формы
/// и только если в обёртке нет ничего из статьи: иначе абзац приедет
/// вместе с тем, что уже стоит на своём месте.
fn lost_block<'a>(para: &NodeRef<'a>, places: &HashMap<String, NodeRef>) -> NodeRef<'a> {
    const WRAPPERS: [&str; 4] = ["li", "aside", "blockquote", "figure"];

    let Some(parent) = para.parent() else {
        return *para;
    };
    let wrapper = parent
        .node_name()
        .is_some_and(|name| WRAPPERS.contains(&name.as_ref()));
    if !wrapper {
        return *para;
    }
    let mixed = parent.descendants().iter().any(|node| {
        node.node_name().as_deref() == Some("p")
            && para_key(node).is_some_and(|(key, _)| places.contains_key(&key))
    });
    if mixed { *para } else { parent }
}

/// За чем встанет возвращённое.
///
/// За самим абзацем — кроме случая, когда абзац в обёртке один: тогда
/// за обёрткой. Иначе проза статьи уезжает внутрь цитаты и читается
/// цитатой — поймано на доказательстве теоремы Эрроу в википедии.
fn place_after<'a>(anchor: &NodeRef<'a>) -> NodeRef<'a> {
    const WRAPPERS: [&str; 3] = ["blockquote", "li", "figure"];

    let Some(parent) = anchor.parent() else {
        return *anchor;
    };
    let wrapper = parent
        .node_name()
        .is_some_and(|name| WRAPPERS.contains(&name.as_ref()));
    if !wrapper {
        return *anchor;
    }
    let alone = parent
        .descendants()
        .iter()
        .filter(|node| node.node_name().as_deref() == Some("p"))
        .count()
        == 1;
    if alone { parent } else { *anchor }
}

/// Ключ абзаца и его вес в словах. Ключ — начало текста без лишних
/// пробелов: этого хватает, чтобы узнать абзац в извлечённом.
/// Что именно встретилось в исходном дереве: абзац или заголовок раздела.
enum Piece {
    Para(String, usize),
    Head(String),
    Shot(String),
}

fn piece_of(node: &NodeRef, base: Option<&Url>) -> Option<Piece> {
    let name = node.node_name()?;
    if name.as_ref() == "p" {
        return para_key(node).map(|(key, words)| Piece::Para(key, words));
    }
    if name.as_ref() == "img" {
        return shot_key(node, base).map(Piece::Shot);
    }
    head_key(node).map(Piece::Head)
}

/// Чем узнаётся картинка — своим адресом. К этому времени ложную
/// ленивость уже снял `unlazy`, поэтому в `src` стоит то же, что увидит
/// читатель.
///
/// Адрес приводим к абсолютному: в извлечённом его уже развернул
/// `Readability`, а на странице он лежит как написан. Сверяя как есть,
/// мы не узнавали в относительном `threads.svg` ту самую картинку,
/// которая в статье уже стоит, и возвращали её вторым экземпляром.
fn shot_key(node: &NodeRef, base: Option<&Url>) -> Option<String> {
    let src = node.attr("src")?;
    let src = src.trim();
    if src.is_empty() || src.starts_with("data:") {
        return None;
    }
    Some(absolute(base, src).unwrap_or_else(|| src.to_string()))
}

fn head_key(node: &NodeRef) -> Option<String> {
    let text = squeeze(&node.text());
    (!text.is_empty()).then_some(text)
}

/// Значок ссылки на сам заголовок — не текст заголовка.
///
/// Генераторы сайтов оборачивают заголовок ссылкой на него же, а внутрь
/// кладут «#», «¶» или звено цепи, спрятанные от скринридера
/// (`aria-hidden="true"`): глазами это кнопка «дай ссылку на этот раздел»,
/// в тексте — мусор перед первым словом («## [#TLDR](…)» у overreacted.io).
///
/// Правило по форме и узкое с двух сторон: только внутри заголовка, только
/// то, что сама страница объявила невидимым для чтения, и только если это
/// знак-другой. Подпись, честно спрятанную от скринридера как дубль
/// (карточки ленты, см. `thumbs`), такое правило не заденет — она длиннее.
fn unpermalink(doc: &Document) {
    const GLYPH_CHARS: usize = 2;

    let inside_heads: String = HEAD_TAGS
        .split(", ")
        .map(|tag| format!("{tag} [aria-hidden=\"true\"]"))
        .collect::<Vec<_>>()
        .join(", ");

    for node in doc.select(&inside_heads).nodes() {
        if node.text().trim().chars().count() <= GLYPH_CHARS {
            node.remove_from_parent();
        }
    }

    // Тот же жест, набранный иначе: ссылка на блок, внутри которого она
    // же и лежит. Номер пункта у «Ководства» (`<a href="#01">01</a>`
    // внутри `<div id="01">`), номер примечания у bizibah. Адрес такой
    // ссылки ведёт в то место, где читатель уже стоит, — снимаем адрес,
    // но оставляем текст: номер пункта это часть текста, по нему на него
    // и ссылаются.
    for link in doc.select("a[href^='#']").nodes() {
        let Some(href) = link.attr("href") else {
            continue;
        };
        let Some(id) = href.strip_prefix('#') else {
            continue;
        };
        if id.is_empty() {
            continue;
        }
        let self_link = link
            .ancestors_it(None)
            .any(|up| up.attr("id").is_some_and(|own| own.as_ref() == id));
        if !self_link {
            continue;
        }
        // Если текст ссылки — сам якорь, набранный якорем («#section2»
        // у alistapart), читать в нём нечего: это тот же значок «дай ссылку
        // на раздел», только словами. Решётка тут и есть признак: без неё
        // совпадение текста с якорем значит обратное — так набран сам
        // заголовок (`<h2 id="rust-analyzer"><a href="#rust-analyzer">
        // rust-analyzer</a></h2>` у matklad), и выбросить его значит
        // потерять раздел.
        let text = squeeze(&link.text());
        if text.strip_prefix('#') == Some(id) {
            link.remove_from_parent();
        } else {
            link.remove_attr("href");
        }
    }
}

/// Стоит ли возвращать выпавший заголовок.
///
/// Правило узкое, потому что заголовков на странице всегда больше, чем
/// в статье: у коробки навигации, у инфобокса, у подвала — свои. Признаки
/// по форме: заголовок это короткая строка (`HEAD_CHARS`), он не лежит
/// в обвязке (`ASIDE_TAGS`) и не лежит в таблице — там он подпись
/// к таблице, а не раздел статьи.
fn worth_heading(node: &NodeRef, text: &str, title: &str) -> bool {
    if text.chars().count() > HEAD_CHARS {
        return false;
    }
    // Название статьи мы печатаем сами, первой строкой. Заголовок, который
    // его повторяет, — это тот же `<h1>` со страницы, и вернуть его значит
    // напечатать название дважды. Поймано на polygon: `<title>` набран
    // капитализацией, `<h1>` обычной строкой, — поэтому сверяем без учёта
    // регистра, а не побайтово.
    if !title.trim().is_empty() && text.to_lowercase() == squeeze(title).to_lowercase() {
        return false;
    }
    !node.ancestors_it(None).any(|up| {
        up.node_name()
            .is_some_and(|name| ASIDE_TAGS.contains(&name.as_ref()) || name.as_ref() == "table")
    })
}

/// Стоит ли возвращать выпавшую картинку.
///
/// Значок сюда не попадёт — его снял `deicon` до извлечения, — но проверку
/// повторяем: она дешёвая, а мелкая картинка в обвязке бывает и без ссылки.
/// Остальное решают границы статьи и место: картинка в шапке, в подвале,
/// в боковой колонке или в таблице иллюстрацией не является.
fn worth_shot(node: &NodeRef) -> bool {
    if is_icon(node) {
        return false;
    }
    // `figure` из списка обвязки здесь исключён намеренно: для прозы
    // врезка это обвязка, а для картинки `<figure>` — родное место.
    let chrome = node.ancestors_it(None).any(|up| {
        up.node_name().is_some_and(|name| {
            let name = name.as_ref();
            (ASIDE_TAGS.contains(&name) && name != "figure") || name == "table"
        })
    });
    if chrome {
        return false;
    }
    // Картинка в ссылке бывает двух разных вещей. Ссылка на файл картинки —
    // это «открыть в полном размере», нормальная часть статьи. Ссылка
    // на страницу — это анонс, карточка «читайте ещё», и возвращать её
    // значит тащить в статью чужие заголовки.
    node.ancestors_it(None)
        .filter(|up| up.node_name().as_deref() == Some("a"))
        .all(|link| {
            link.attr("href")
                .is_some_and(|href| crate::markdown::points_at_image(&href))
        })
}

/// Заглавная картинка статьи — та, которую страница объявила своей.
///
/// Иллюстрация лида доезжает до читателя не всегда: у panorama.pub она
/// вовсе не картинка, а фон блока (`data-bg-image-jpeg`), и `<img>` для
/// неё на странице нет ни одного. Разбирать чужой CSS мы не станем, но
/// то же самое страница объявляет о себе стандартом — Open Graph, той
/// самой карточкой, которую показывают мессенджеры.
///
/// Правило узкое с двух сторон: страница объявила себя статьёй
/// (`og:type`), и в статье не нашлось **ни одной** своей картинки —
/// иначе заглавная приехала бы вторым экземпляром того, что уже стоит
/// первым абзацем. Цена записана честно: у личного блога в этой карточке
/// лежит портрет автора (nav.al, joelonsoftware), и он встанет над
/// статьёй. Замер по корпусу 13 сентября 2026: тринадцать страниц без
/// картинки объявляют себя статьёй, у одиннадцати в карточке настоящая
/// иллюстрация, у двух — портрет.
fn with_cover(source: &Document, content_html: String, url: &str) -> String {
    let meta = |selector: &str| -> Option<String> {
        source
            .select(selector)
            .nodes()
            .first()
            .and_then(|node| node.attr("content"))
            .map(|value| value.trim().to_string())
            .filter(|value| !value.is_empty())
    };

    let article = meta("meta[property='og:type']").is_some_and(|kind| kind == "article");
    if !article {
        return content_html;
    }

    let content = Document::from(content_html.clone());
    if content.select("img").nodes().iter().any(has_source) {
        return content_html;
    }

    let Some(cover) =
        meta("meta[property='og:image']").or_else(|| meta("meta[name='twitter:image']"))
    else {
        return content_html;
    };
    let Some(cover) = absolute(Url::parse(url).ok().as_ref(), &cover) else {
        return content_html;
    };
    let alt = meta("meta[property='og:image:alt']").unwrap_or_default();

    let image = format!(
        "<p><img src=\"{}\" alt=\"{}\"></p>",
        escape(&cover),
        escape(&alt)
    );
    match content.select("#readability-page-1").nodes().first() {
        Some(node) => {
            node.prepend_html(image);
            node.html().to_string()
        }
        None => format!("{image}{content_html}"),
    }
}

fn has_source(node: &NodeRef) -> bool {
    node.attr("src")
        .is_some_and(|src| !src.trim().is_empty() && !src.trim().starts_with("data:"))
}

fn escape(value: &str) -> String {
    value
        .replace('&', "&amp;")
        .replace('"', "&quot;")
        .replace('<', "&lt;")
}

/// Что вернуть вместе с картинкой: её `<figure>`, если он есть, — там
/// подпись и ссылка «открыть в полном размере». Иначе саму картинку.
fn lost_shot<'a>(image: &NodeRef<'a>) -> NodeRef<'a> {
    const WRAPPERS: [&str; 2] = ["figure", "picture"];

    let mut block = *image;
    for up in image.ancestors_it(Some(3)) {
        if up
            .node_name()
            .is_some_and(|name| WRAPPERS.contains(&name.as_ref()))
        {
            block = up;
        }
    }
    block
}

fn para_key(node: &NodeRef) -> Option<(String, usize)> {
    let text = squeeze(&node.text());
    let words = text.split_whitespace().count();
    if words < KEY_WORDS {
        return None;
    }
    Some((text.chars().take(KEY_LEN).collect(), words))
}

/// Стоит ли возвращать выпавший абзац.
fn worth_restoring(node: &NodeRef, words: usize) -> bool {
    if words < ORPHAN_WORDS {
        return false;
    }
    let aside = node.ancestors_it(None).any(|up| {
        up.node_name()
            .is_some_and(|name| ASIDE_TAGS.contains(&name.as_ref()))
    });
    !aside || words >= ASIDE_WORDS
}

/// Развернуть адреса в куске, оставив его куском: `absolute_html` отдаёт
/// целый документ, а этот кусок встаёт соседом абзаца внутри статьи.
fn fragment(html: &str, base: &str) -> String {
    let doc = Document::from(absolute_html(html, base));
    doc.select("body").inner_html().to_string()
}

/// Язык блока кода — в атрибут, который переживёт извлечение.
///
/// Подсветка (`code::spans`) берёт язык из ограждения, ограждение пишет
/// конвертер по классу `language-*` — а Readability классы вычищает целиком
/// (`keep_classes: false`, и `classes_to_preserve` понимает точные имена,
/// не префиксы). До конвертера язык не доезжает, и подсветка на любой
/// веб-странице выходит общая. Поэтому снимаем язык сами, до извлечения,
/// и кладём в `data-lang`: атрибуты Readability не трогает.
///
/// Лежит он в трёх местах, и все три встречаются в корпусе: на самом
/// `<code>` (brandur, rust book), на нём же атрибутом (`data-lang="plain"`
/// у блога Rust), на обёртке — у fasterthanli.me язык объявлен
/// на `<figure class="code-block" data-lang="shell">`, а у `<code>`
/// внутри только класс вёрстки.
fn keep_lang(doc: &Document) {
    for code in doc.select("code").nodes() {
        // Блок, а не код-спан: признак тот же, по которому их различает
        // конвертер, — перевод строки внутри либо `<pre>` снаружи.
        let in_pre = code
            .parent()
            .and_then(|parent| parent.node_name())
            .as_deref()
            == Some("pre");
        if !in_pre && !code.text().contains('\n') {
            continue;
        }
        let Some(language) = nearest_language(code) else {
            continue;
        };
        code.set_attr("data-lang", &language);
    }
}

/// Язык у самого блока или у ближайшей обёртки.
fn nearest_language(code: &NodeRef) -> Option<String> {
    std::iter::successors(Some(*code), NodeRef::parent)
        .take(LANG_DEPTH)
        .find_map(|node| language_of(&node))
}

/// Где язык объявлен атрибутом.
pub(crate) const LANG_ATTRS: [&str; 3] = ["data-lang", "data-language", "data-code-language"];

/// Чем размечают язык в классе.
pub(crate) const LANG_PREFIXES: [&str; 3] = ["language-", "lang-", "highlight-source-"];

/// Что на узле объявлено языком.
///
/// Атрибут вперёд класса: у chroma рядом стоят `class="language-rust"`
/// и `data-lang="rust"`, и второй уже очищен от префикса.
fn language_of(node: &NodeRef) -> Option<String> {
    let named = LANG_ATTRS.iter().find_map(|attr| node.attr(attr));
    if let Some(language) = named.as_deref().and_then(language_token) {
        return Some(language);
    }

    let class = node.attr("class")?;
    class.split_whitespace().find_map(|class| {
        LANG_PREFIXES
            .iter()
            .find_map(|prefix| class.strip_prefix(prefix))
            .and_then(language_token)
    })
}

/// Имя языка, пригодное для ограждения: одно слово из букв, цифр и знаков,
/// которые встречаются в именах языков (`c++`, `c#`, `objective-c`).
///
/// Сайты кладут в это место что угодно — «Shell session», пустую строку,
/// подпись к блоку, — а попадает оно прямо в текст статьи и в сохранённый
/// файл.
pub(crate) fn language_token(raw: &str) -> Option<String> {
    const LIMIT: usize = 20;

    let token = raw.split_whitespace().next()?.to_ascii_lowercase();
    let ok = |ch: char| ch.is_ascii_alphanumeric() || matches!(ch, '+' | '#' | '-' | '_' | '.');
    if token.is_empty() || token.len() > LIMIT || !token.chars().all(ok) {
        return None;
    }
    Some(token)
}

/// Лента страницы: узел, в котором лежат все карточки.
///
/// Карточка — заголовок, который целиком ссылка на другую страницу; форма
/// та же, по которой конвертер отличает анонс от раздела. Readability
/// на ленте выбирает самую текстовую карточку и выбрасывает остальные
/// девятнадцать: получается «статья» под именем сайта, которой на странице
/// нет. Поэтому ленту снимаем сами — общим предком карточек, а не всей
/// страницей: так в текст не попадают ни меню, ни подвал.
///
/// Снимать надо до извлечения: `Readability` забирает документ себе.
fn listing(doc: &Document, base: &str) -> Option<Listing> {
    let cards: Vec<NodeRef> = doc
        .select("h1, h2, h3")
        .nodes()
        .iter()
        .filter(|node| is_card(node))
        .cloned()
        .collect();

    if cards.len() < MIN_CARDS {
        return None;
    }

    let region = common_ancestor(&cards)?;
    // Лента — часть страницы, а не страница целиком. Если в вырезанное
    // попали меню или подвал, значит общий предок карточек — весь документ:
    // так бывает, когда заголовок-ссылка нашёлся и в шапке, и сбоку.
    // Тогда вырезать нечего, и страница остаётся статьёй.
    if region
        .descendants()
        .iter()
        .any(|node| matches!(node.node_name().as_deref(), Some("nav" | "footer")))
    {
        return None;
    }

    let weight = cards
        .iter()
        .map(|card| squeeze(&block_of(card, &cards).text()).chars().count())
        .sum();

    Some(Listing {
        html: absolute_html(&region.html(), base),
        text: squeeze(&region.text()),
        weight,
        cards: cards.len(),
    })
}

/// Карточка целиком: самый верхний предок заголовка, в котором нет других
/// заголовков-карточек. Это и есть запись ленты — с миниатюрой, датой
/// и подводкой.
fn block_of<'a>(card: &NodeRef<'a>, cards: &[NodeRef<'a>]) -> NodeRef<'a> {
    let mut block = *card;

    for parent in card.ancestors_it(None) {
        let shared = cards
            .iter()
            .any(|other| other.id != card.id && inside(other, &parent));
        if shared {
            break;
        }
        block = parent;
    }

    block
}

fn inside(node: &NodeRef, ancestor: &NodeRef) -> bool {
    node.ancestors_it(None).any(|up| up.id == ancestor.id)
}

/// Развернуть адреса в вырезанном куске.
///
/// Правим копию, а не само дерево: правка на месте досталась бы и статье —
/// `Readability` разбирает тот же документ, — и якоря заголовков
/// (`[#section2](#section2)`) превратились бы в адреса страницы.
fn absolute_html(html: &str, base: &str) -> String {
    let doc = Document::from(html.to_string());
    let base = Url::parse(base).ok();

    for node in doc.select("a[href], img[src]").nodes() {
        let attr = match node.node_name().as_deref() {
            Some("a") => "href",
            _ => "src",
        };
        let Some(value) = node.attr(attr) else {
            continue;
        };
        if let Some(absolute) = absolute(base.as_ref(), &value) {
            node.set_attr(attr, &absolute);
        }
    }

    doc.html().to_string()
}

/// Заголовок, который целиком является ссылкой на другую страницу.
fn is_card(node: &NodeRef) -> bool {
    let text = node.text();
    let text = text.trim();
    if text.is_empty() {
        return false;
    }

    let links: Vec<NodeRef> = node
        .descendants()
        .into_iter()
        .filter(|node| node.node_name().as_deref() == Some("a") && node.has_attr("href"))
        .collect();
    let [link] = links.as_slice() else {
        return false;
    };

    let href = link.attr("href").unwrap_or_default();
    let href = href.trim();
    // Ссылка на якорь внутри страницы — это раздел статьи, а не карточка.
    !href.is_empty() && !href.starts_with('#') && link.text().trim() == text
}

/// Ближайший общий предок карточек.
fn common_ancestor<'a>(cards: &[NodeRef<'a>]) -> Option<NodeRef<'a>> {
    let chains: Vec<Vec<dom_query::NodeId>> = cards
        .iter()
        .map(|card| card.ancestors_it(None).map(|node| node.id).collect())
        .collect();

    cards
        .first()?
        .ancestors_it(None)
        .find(|node| chains.iter().all(|chain| chain.contains(&node.id)))
}

/// Текст без лишних пробелов: разметка ставит их как придётся, а сравнивать
/// приходится куски из разных мест дерева.
fn squeeze(text: &str) -> String {
    text.split_whitespace().collect::<Vec<_>>().join(" ")
}

/// Картинки, спрятанные за ссылками: адрес ссылки → адрес картинки.
///
/// Лента состоит из карточек «миниатюра плюс заголовок», и миниатюру сайт
/// помечает `aria-hidden="true"` — для скринридера она дубль соседнего
/// заголовка, и размечено это правильно. Readability понимает подсказку
/// буквально и выбрасывает картинку вместе со ссылкой; читателю, который
/// смотрит глазами, карточка без картинки уже не карточка.
///
/// Поэтому миниатюры снимаются с исходного дерева, до извлечения, и в текст
/// возвращаются только там, где страница оказалась лентой: в статье такая
/// подстановка была бы отсебятиной.
///
/// Адреса разворачиваем сами: `fix_relative_uris` из dom_smoothie правит
/// только извлечённое содержимое, а мы берём картинки мимо него.
fn thumbs(doc: &Document, base: &str) -> HashMap<String, String> {
    /// Насколько глубоко картинка сидит внутри ссылки. Обычно `<a><img>`,
    /// но между ними бывает `<figure>` или `<span>` с рамкой.
    const DEPTH: usize = 4;

    let base = Url::parse(base).ok();
    let mut out = HashMap::new();

    for img in doc.select("img[src]").nodes() {
        let Some(src) = img.attr("src") else {
            continue;
        };
        let Some(link) = img
            .ancestors_it(Some(DEPTH))
            .find(|node| node.node_name().as_deref() == Some("a"))
        else {
            continue;
        };
        let Some(href) = link.attr("href") else {
            continue;
        };

        let (Some(href), Some(src)) = (
            absolute(base.as_ref(), &href),
            absolute(base.as_ref(), &src),
        ) else {
            continue;
        };
        // Первая картинка ссылки и есть её миниатюра: дальше идут значки
        // вроде «комментарии» и «поделиться».
        out.entry(href).or_insert(src);
    }

    out
}

/// Снять сноски со статьи, отдав её html без их тел.
fn lift_notes(content_html: String) -> (String, Notes) {
    let article = Document::from(content_html.as_str());
    let notes = notes(&article);
    if notes.bodies.is_empty() {
        return (content_html, Notes::default());
    }

    // В той же форме, в какой пришло, — как и в `restore`.
    let page = article.select("#readability-page-1");
    let html = match page.nodes().first() {
        Some(node) => node.html().to_string(),
        None => article.select("body").inner_html().to_string(),
    };
    (html, notes)
}

/// Сноски к единому виду.
///
/// В вебе сноска — это ссылка на якорь внутри страницы, а её текст лежит
/// пунктом списка в конце: `[1](#fn:R)` у danluu, `[1](#footnote-1)`
/// у brandur, `[\[1\]](#cite_note-…)` у википедии, `[[1](#f1n)` у Грэма.
/// До читателя не доезжало ни то ни другое: markdown несёт только текст
/// ссылки, а id пункта теряется при конвертации. В окне сноска выходила
/// мёртвой ссылкой, в `less` — строкой мусора, и на статье о Rust таких
/// строк двести семьдесят семь.
///
/// Поэтому сноски снимаются здесь, по дереву, где ещё видно и ссылку,
/// и цель; дальше они живут сносками GFM, которые умеет и конвертер,
/// и окно.
///
/// Форма узкая, и каждое условие поймано на живой странице: текст ссылки —
/// только число (иначе в сноски уедет оглавление, у которого ссылки
/// такие же), цель — блок (обратная ссылка википедии ведёт на `<sup>`
/// посреди текста), цель — внутри статьи (ссылка в никуда сноской
/// не была).
fn notes(doc: &Document) -> Notes {
    // Тела ищем среди блоков с id: по ним и опознаётся сноска.
    let mut blocks: HashMap<String, NodeRef> = HashMap::new();
    for node in doc.select("[id]").nodes() {
        let Some(id) = node.attr("id") else { continue };
        if node
            .node_name()
            .is_some_and(|name| NOTE_TAGS.contains(&name.as_ref()))
        {
            blocks.entry(id.to_string()).or_insert(*node);
        }
    }
    if blocks.is_empty() {
        return Notes::default();
    }

    // Номера раздаём в порядке ссылок в тексте: читателю сноска встречается
    // там, а не в списке под статьёй.
    let mut numbers: HashMap<String, usize> = HashMap::new();
    let mut order: Vec<String> = Vec::new();
    for link in doc.select("a[href]").nodes() {
        let Some(href) = link.attr("href") else {
            continue;
        };
        let Some(id) = href.strip_prefix('#') else {
            continue;
        };
        if !is_note_mark(&squeeze(&link.text())) {
            continue;
        }
        let Some(body) = blocks.get(id) else { continue };
        // Метка сноски стоит в тексте, а тело — в стороне. Ссылка,
        // лежащая внутри того блока, на который она же и показывает, —
        // это постоянная ссылка на абзац («дай ссылку на этот пункт»),
        // и таким набран весь «Ководство» и разметка примечаний bizibah.
        // Не отличив одно от другого, мы уносили в сноски саму статью.
        if inside(link, body) {
            continue;
        }
        let id = id.to_owned();
        if !numbers.contains_key(&id) {
            numbers.insert(id.clone(), order.len() + 1);
            order.push(id);
        }
    }
    if order.is_empty() {
        return Notes::default();
    }

    // Тело берём после того, как раздали номера: внутри сноски бывают
    // ссылки на другие сноски, и они тоже должны получить свой номер.
    let mut bodies = Vec::with_capacity(order.len());
    for id in &order {
        let body = blocks[id];
        // Обратная ссылка («↑», «1 2» у википедии, «[return]» у danluu)
        // ведёт назад в текст: в едином виде дорогу назад даёт окно,
        // и здесь она лишняя. Остальные ссылки внутрь страницы ведут туда,
        // куда markdown дойти не может, — у них снимаем адрес, но оставляем
        // текст. Снять вместе с текстом значит потерять прозу: у википедии
        // короткая ссылка на источник («Klabnik & Nichols 2023») набрана
        // именно так, и на одной статье их двести восемьдесят.
        let inside: Vec<NodeRef> = body
            .descendants_it()
            .filter(|node| node.node_name().as_deref() == Some("a"))
            .collect();
        for inner in inside {
            let Some(href) = inner.attr("href") else {
                continue;
            };
            if let Some(target) = href.strip_prefix('#')
                && !numbers.contains_key(target)
            {
                if is_backlink(&squeeze(&inner.text())) {
                    inner.remove_from_parent();
                } else {
                    inner.remove_attr("href");
                }
            }
        }
        bodies.push(body.inner_html().to_string());
    }

    // Тела из статьи убираем: они уезжают в свой блок, и остаться на месте
    // значило бы приехать к читателю дважды.
    for id in &order {
        blocks[id].remove_from_parent();
    }
    // Список, из которого всё вынули, — пустая строка перед хвостом.
    for list in doc.select("ol, ul, dl").nodes() {
        let holds_picture = list
            .descendants_it()
            .any(|node| node.node_name().as_deref() == Some("img"));
        if squeeze(&list.text()).is_empty() && !holds_picture {
            list.remove_from_parent();
        }
    }

    Notes { numbers, bodies }
}

/// Обратная ссылка сноски — та, что ведёт из неё назад в текст.
fn is_backlink(text: &str) -> bool {
    let text = text.trim_matches(|c: char| "[]() ".contains(c)).trim();
    text.is_empty()
        || text.chars().all(|c| "↑↩⇑^".contains(c))
        || text.chars().all(|c| c.is_ascii_digit())
        || matches!(
            text.to_lowercase().as_str(),
            "return" | "back" | "jump up" | "назад" | "вверх"
        )
}

/// Похож ли текст ссылки на номер сноски: «1», «\[1\]», «(1)».
fn is_note_mark(text: &str) -> bool {
    let digits = text.trim_matches(|c: char| !c.is_ascii_digit());
    !digits.is_empty()
        && digits.len() <= NOTE_DIGITS
        && digits.chars().all(|c| c.is_ascii_digit())
        // Кроме цифр и скобок в метке сноски ничего не бывает; «глава 2»
        // и «2 сентября» — это текст, а не метка.
        && text
            .chars()
            .all(|c| c.is_ascii_digit() || "[]()（）【】 \u{00a0}".contains(c))
}

fn absolute(base: Option<&Url>, link: &str) -> Option<String> {
    let link = link.trim();
    if link.is_empty() || link.starts_with("data:") {
        return None;
    }
    match base {
        Some(base) => base.join(link).ok().map(String::from),
        None => Url::parse(link).ok().map(String::from),
    }
}

/// Снять подсказку `loading="lazy"` с картинок, у которых адрес и так на месте.
///
/// `loading` — подсказка браузеру, когда качать, а не признак подменённого
/// адреса. dom_smoothie считает иначе: у него в «ленивые» попадает любая
/// `<img loading="lazy">`, и тогда он ищет настоящий адрес по остальным
/// атрибутам — берёт первый, в значении которого мерещится имя файла
/// картинки. У википедии рядом со `src` стоит `resource` с адресом
/// *страницы описания* файла (`…/wiki/Файл:Портрет.jpg`), и он затирает
/// настоящий адрес на upload.wikimedia.org — вместо фотографии читателю
/// приезжает html. Иконки на той же странице уцелели случайно: их `resource`
/// оканчивается на `.svg`, а этого расширения в списке у dom_smoothie нет.
///
/// В самом readability.js ленивой считается картинка без `src` либо
/// с классом `lazy`; возвращаем это правило, снимая подсказку с тех,
/// у кого адрес уже есть. Заглушку в `src` (`data:`-пиксель) не трогаем:
/// вот там подстановка по атрибутам и есть единственный способ найти
/// картинку.
fn unlazy(doc: &Document) {
    for node in doc.select("img[loading]").nodes() {
        let Some(src) = node.attr("src") else {
            continue;
        };
        let src = src.trim();
        if src.is_empty() || src.starts_with("data:") {
            continue;
        }
        node.remove_attr("loading");
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    /// Абзац настоящей длины: извлечению нужен текст, иначе оно ничего
    /// не выбирает.
    const TEXT: &str = "Кеннет Эрроу доказал теорему о невозможности коллективного \
        выбора, и это перевернуло теорию общественного благосостояния. \
        Ниже разбирается, что именно утверждает теорема и почему её \
        следствия так неудобны для любой процедуры голосования.";

    /// Статья с картинкой, у которой рядом со `src` лежит посторонний
    /// атрибут с похожим на картинку значением.
    fn page(img: &str) -> String {
        let text = TEXT;
        format!(
            "<html><body><article><h1>Эрроу</h1><p>{img}</p><p>{text}</p><p>{text}</p><p>{text}</p></article></body></html>"
        )
    }

    #[test]
    fn a_byline_keeps_the_name_apart_from_the_date() {
        let text = TEXT;
        let page = format!(
            "<html><body><article>\
             <span class=\"tm-user-info author\">\
             <a href=\"/users/RationalAnswer/\">RationalAnswer</a><!----><!--[-->\
             <span class=\"datetime-published\"><time>8 сен в 13:42</time></span>\
             </span>\
             <h1>Эрроу</h1><p>{text}</p><p>{text}</p><p>{text}</p></article></body></html>"
        );
        let article = extract(&page, "https://example.org/a").unwrap();
        assert_eq!(
            article.byline.as_deref(),
            Some("RationalAnswer 8 сен в 13:42")
        );
    }

    #[test]
    fn what_the_page_hides_from_print_is_not_text() {
        let text = TEXT;
        let page = format!(
            "<html><body><article>\
             <div id=\"siteSub\" class=\"noprint\">Материал из Википедии — свободной энциклопедии</div>\
             <p>{text}</p><p>{text} <a class=\"noprint\" href=\"/wikidata\">[вд]</a></p><p>{text}</p>\
             </article></body></html>"
        );
        let article = extract(&page, "https://example.org/a").unwrap();
        assert!(!article.content_html.contains("Материал из Википедии"));
        assert!(!article.content_html.contains("[вд]"));
        assert!(article.content_html.contains("Кеннет Эрроу"));
    }

    #[test]
    fn a_lazy_hint_does_not_replace_a_working_address() {
        let img = r#"<img loading="lazy" src="https://upload.example.org/commons/portrait.jpg" resource="https://example.org/wiki/File:Portrait.jpg">"#;
        let article = extract(&page(img), "https://example.org/wiki/Arrow").unwrap();
        assert!(
            article
                .content_html
                .contains(r#"src="https://upload.example.org/commons/portrait.jpg""#)
        );
        assert!(
            !article
                .content_html
                .contains(r#"src="https://example.org/wiki/File:Portrait.jpg""#)
        );
    }

    /// Проза, выпавшая внутри статьи, возвращается на своё место;
    /// обвязка за её границами и короткая врезка — нет.
    #[test]
    fn lost_prose_comes_back_and_boilerplate_does_not() {
        let long = "слово ".repeat(50);
        let source = Document::from(format!(
            "<body>\
             <header><p>Шапка сайта: разделы, поиск и вход в личный кабинет</p></header>\
             <article>\
             <p>Первый абзац статьи, с которого всё начинается тут</p>\
             <p>Выпавший абзац, который лежит между своими: {long}</p>\
             <p>Второй выпавший подряд, и порядок двух должен сохраниться</p>\
             <aside><p>Реклама курса: успейте записаться сегодня со скидкой</p></aside>\
             <aside><p>Врезка в полновесный абзац, и она часть статьи: {long}</p></aside>\
             <p>Последний абзац статьи, на котором она заканчивается тут</p>\
             </article>\
             <footer><p>Подвал сайта: копирайт, ссылки на соцсети и лицензия</p></footer>\
             </body>"
        ));
        let content = "<div id=\"readability-page-1\">\
             <p>Первый абзац статьи, с которого всё начинается тут</p>\
             <p>Последний абзац статьи, на котором она заканчивается тут</p>\
             </div>";

        let restored = restore(&source, content, "https://example.org/post", "").expect("ничего");

        assert!(restored.contains("Выпавший абзац"), "{restored}");
        assert!(
            restored.contains("Врезка в полновесный абзац"),
            "{restored}"
        );
        // Порядок двух подряд идущих сирот сохраняется.
        let second = restored
            .find("Второй выпавший подряд")
            .expect("нет второго");
        assert!(restored.find("Выпавший абзац").unwrap() < second);
        // Шапка и подвал лежат за границами статьи, короткая врезка —
        // реклама: ни того, ни другого в статье быть не должно.
        assert!(!restored.contains("Шапка сайта"), "{restored}");
        assert!(!restored.contains("Подвал сайта"), "{restored}");
        assert!(!restored.contains("Реклама курса"), "{restored}");
    }

    /// Заголовки статьи возвращаются на свои места — и только они.
    ///
    /// Современный MediaWiki заворачивает заголовок в `<div class="mw-heading">`,
    /// и кандидат выбирается мимо них: на теореме Эрроу в выводе не оставалось
    /// ни одного `##`, оглавление собиралось из вех по абзацам. Заголовков
    /// на странице при этом всегда больше, чем в статье, — у коробки
    /// навигации и у подвала свои, — поэтому правило узкое с четырёх сторон.
    #[test]
    fn the_headings_of_the_article_come_back_and_nothing_else() {
        let source = Document::from(
            "<body>\
             <nav><h2>Навигация по сайту</h2></nav>\
             <article>\
             <h1>Заголовок статьи, он же её название</h1>\
             <p>Первый абзац статьи, с которого всё начинается тут</p>\
             <div class=\"mw-heading\"><h2>Формулировки</h2></div>\
             <p>Второй абзац статьи, он идёт сразу за заголовком раздела</p>\
             <div class=\"mw-heading\"><h3>Доказательство</h3></div>\
             <p>Последний абзац статьи, на котором она заканчивается тут</p>\
             <table><tr><td><h2>Подпись таблицы, а не раздел</h2></td></tr></table>\
             </article>\
             <footer><h2>Подвал сайта</h2></footer>\
             </body>"
                .to_string(),
        );
        let content = "<div id=\"readability-page-1\">\
             <p>Первый абзац статьи, с которого всё начинается тут</p>\
             <p>Второй абзац статьи, он идёт сразу за заголовком раздела</p>\
             <p>Последний абзац статьи, на котором она заканчивается тут</p>\
             </div>";

        let restored = restore(
            &source,
            content,
            "https://example.org/post",
            "Заголовок статьи, он же её название",
        )
        .expect("заголовки не вернулись");

        // Порядок — страницы, а не наш: раздел стоит перед своим абзацем.
        let head = restored.find("Формулировки").expect("нет заголовка");
        let para = restored.find("Второй абзац").expect("нет абзаца");
        assert!(head < para, "{restored}");
        assert!(restored.contains("Доказательство"), "{restored}");

        // Название статьи мы печатаем сами: второй раз не надо.
        assert!(!restored.contains("он же её название"), "{restored}");
        // Обвязка за границами статьи и подпись таблицы — не разделы.
        assert!(!restored.contains("Навигация по сайту"), "{restored}");
        assert!(!restored.contains("Подвал сайта"), "{restored}");
        assert!(!restored.contains("Подпись таблицы"), "{restored}");
    }

    /// Заголовок, который в статье уже есть, вторым экземпляром не приезжает.
    #[test]
    fn a_heading_already_in_the_article_is_left_alone() {
        let source = Document::from(
            "<body><article>\
             <p>Первый абзац статьи, с которого всё начинается тут</p>\
             <h2>Раздел про шину</h2>\
             <p>Последний абзац статьи, на котором она заканчивается тут</p>\
             </article></body>"
                .to_string(),
        );
        let content = "<div id=\"readability-page-1\">\
             <p>Первый абзац статьи, с которого всё начинается тут</p>\
             <h2>Раздел про шину</h2>\
             <p>Последний абзац статьи, на котором она заканчивается тут</p>\
             </div>";
        assert!(restore(&source, content, "https://example.org/post", "").is_none());
    }

    /// Значок ссылки на сам заголовок — не текст заголовка.
    #[test]
    fn a_permalink_glyph_is_not_part_of_the_heading() {
        let doc = Document::from(
            "<body><article>\
             <h2 id=\"tldr\"><a href=\"#tldr\"><span aria-hidden=\"true\">#</span>TLDR</a></h2>\
             <p>Абзац с подписью, спрятанной от скринридера как дубль</p>\
             <figure><img src=\"a.png\"><figcaption aria-hidden=\"true\">Подпись, которую \
             скринридер услышит из alt, и она длиннее значка</figcaption></figure>\
             </article></body>"
                .to_string(),
        );
        unpermalink(&doc);

        let html = doc.select("body").inner_html().to_string();
        assert!(!html.contains(">#<"), "{html}");
        assert!(html.contains("TLDR"), "{html}");
        // Правило живёт только внутри заголовка: спрятанное в другом месте
        // бывает содержанием (карточки ленты, см. `thumbs`).
        assert!(html.contains("Подпись, которую"), "{html}");
    }

    /// Дубль — не сирота, и ловится он текстом, а не разметкой: в статье
    /// сноска лежит списком, на странице — абзацем во врезке; а сама врезка
    /// на странице стоит дважды, копией на узкий экран.
    #[test]
    fn the_same_text_comes_back_once_and_only_if_missing() {
        let long = "слово ".repeat(50);
        let note = format!("Сноска про шину предприятия, и она длинная: {long}");
        let page = |note: &str| {
            format!(
                "<body><article>\
                 <p>Первый абзац статьи, с которого всё начинается тут</p>\
                 <aside><p>{note}</p></aside>\
                 <aside><p>{note}</p></aside>\
                 <p>Последний абзац статьи, на котором она заканчивается тут</p>\
                 </article></body>"
            )
        };
        let head = "<p>Первый абзац статьи, с которого всё начинается тут</p>";
        let tail = "<p>Последний абзац статьи, на котором она заканчивается тут</p>";

        // Сноски в статье нет — возвращается, и один раз, а не два.
        let restored = restore(
            &Document::from(page(&note)),
            &format!("<div id=\"readability-page-1\">{head}{tail}</div>"),
            "https://example.org/post",
            "",
        )
        .expect("сноска не вернулась");
        assert_eq!(restored.matches("Сноска про шину").count(), 1, "{restored}");

        // Сноска в статье уже есть, только другой разметкой — не трогаем.
        let already =
            format!("<div id=\"readability-page-1\">{head}<ul><li>{note}</li></ul>{tail}</div>");
        assert!(
            restore(
                &Document::from(page(&note)),
                &already,
                "https://example.org/post",
                ""
            )
            .is_none()
        );
    }

    /// Возвращать нечего — возвращаем `None`, а не переписанный html:
    /// на подавляющем большинстве страниц сирот нет вовсе.
    #[test]
    fn a_whole_article_is_left_alone() {
        let source = Document::from(
            "<body><article>\
             <p>Первый абзац статьи, с которого всё начинается тут</p>\
             <p>Последний абзац статьи, на котором она заканчивается тут</p>\
             </article></body>"
                .to_string(),
        );
        let content = "<div id=\"readability-page-1\">\
             <p>Первый абзац статьи, с которого всё начинается тут</p>\
             <p>Последний абзац статьи, на котором она заканчивается тут</p>\
             </div>";
        assert!(restore(&source, content, "https://example.org/post", "").is_none());
    }

    /// Значок против картинки: правило проверяется на всех местах сразу,
    /// потому что различает их именно место, а не сама картинка.
    #[test]
    fn an_icon_goes_and_a_picture_stays() {
        let doc = Document::from(
            "<body>\
             <p><a href=\"/edit\"><img src=\"pencil.png\" alt=\"Edit this at Wikidata\" \
               width=\"10\" height=\"10\"></a></p>\
             <div><img src=\"share.png\" width=\"16\" height=\"16\"></div>\
             <p>Логотип <img src=\"apple.png\" width=\"24\" height=\"24\"> в строке.</p>\
             <div><img src=\"avatar.png\" alt=\"Cool bear\" width=\"42\" height=\"42\"></div>\
             <div><img src=\"s.gif\" width=\"40\" height=\"1\"></div>\
             <p><img src=\"photo.jpg\" width=\"600\" height=\"400\"></p>\
             </body>"
                .to_string(),
        );
        deicon(&doc);
        let html = doc.html();

        // Значок в ссылке: подпись у него — имя ссылки, а не подпись.
        assert!(!html.contains("pencil.png"), "карандашик цел");
        // Значок один в блоке — кнопка, а не изображение.
        assert!(!html.contains("share.png"), "кнопка цела");
        // Знак посреди прозы, подписанная картинка, распорка треда
        // и обычная иллюстрация остаются.
        assert!(html.contains("apple.png"), "логотип в строке съеден");
        assert!(html.contains("avatar.png"), "подписанная картинка съедена");
        assert!(html.contains("s.gif"), "распорка треда съедена");
        assert!(html.contains("photo.jpg"), "иллюстрация съедена");
    }

    /// Класс с языком Readability вычищает вместе со всеми классами,
    /// и подсветка на веб-странице выходила общая. Язык должен доехать
    /// до конвертера — хоть со своего `<code>`, хоть с обёртки:
    /// у fasterthanli.me он объявлен на `<figure>`.
    #[test]
    fn a_code_language_survives_extraction() {
        let code = "<pre><code class=\"language-rust\">fn main() {\n}</code></pre>\
            <figure class=\"code-block\" data-lang=\"shell\">\
            <code class=\"scroll-wrapper\">cargo test\ncargo run</code></figure>";
        let html = page(code);

        let article = extract(&html, "https://example.org/post").unwrap();
        assert!(
            !article.content_html.contains("language-rust"),
            "класс цел?"
        );
        assert!(article.content_html.contains(r#"data-lang="rust""#));
        assert!(article.content_html.contains(r#"data-lang="shell""#));

        let markdown = crate::markdown::from_article(&article).unwrap().markdown;
        assert!(markdown.contains("```rust\n"), "{markdown}");
        assert!(markdown.contains("```shell\n"), "{markdown}");
    }

    /// Код-спан посреди абзаца языком обёртки не красится: у него
    /// ограждения нет вовсе, а `data-lang` в тексте — мусор.
    #[test]
    fn an_inline_span_keeps_no_language() {
        let doc = Document::from(
            "<div data-lang=\"rust\"><p>Тут <code>Vec</code> и всё.</p></div>".to_string(),
        );
        keep_lang(&doc);
        assert!(!doc.select("code[data-lang]").exists());
    }

    /// Лента: десяток карточек, и Readability приносит из них одну.
    /// Правило должно узнать в этом ленту — проверяем само правило,
    /// а не то, какую из карточек выберет извлечение.
    #[test]
    fn a_feed_is_recognised_when_extraction_keeps_one_card() {
        let teaser = TEXT.repeat(2);
        let mut cards = String::new();
        for i in 1..=10 {
            cards.push_str(&format!(
                "<article><h2><a href=\"/post/{i}\">Запись номер {i}</a></h2><p>{teaser}</p></article>"
            ));
        }
        let html =
            format!("<html><body><main><div class=\"feed\">{cards}</div></main></body></html>");

        let listing =
            listing(&Document::from(html), "https://example.org/feed").expect("лента не опознана");
        // Извлечение принесло одну запись из десяти.
        assert!(listing.holds(&format!("<div><p>{teaser}</p></div>")));
        assert!(listing.html.contains("Запись номер 10"));
        // Адреса в ленте разворачиваем сами: мимо Readability это некому.
        assert!(listing.html.contains("https://example.org/post/3"));
    }

    /// Статья, к которой приложили витрину «читайте ещё», остаётся статьёй:
    /// её текст больше всей витрины.
    #[test]
    fn an_article_with_a_teaser_grid_is_not_a_feed() {
        let mut cards = String::new();
        for i in 1..=10 {
            cards.push_str(&format!(
                "<div><h3><a href=\"/post/{i}\">Другая статья {i}</a></h3><p>{TEXT}</p></div>"
            ));
        }
        let body = TEXT.repeat(12);
        let html = format!(
            "<html><body><main><article><h1>Статья</h1><p>{body}</p></article>\
             <div class=\"more\">{cards}</div></main></body></html>"
        );

        let listing =
            listing(&Document::from(html), "https://example.org/post").expect("витрина есть");
        assert!(
            !listing.holds(&format!("<article><p>{body}</p></article>")),
            "статью приняли за запись ленты"
        );
    }

    /// Меню и подвал внутри вырезанного означают, что общим предком карточек
    /// оказалась вся страница. Вырезать нечего.
    #[test]
    fn a_page_wide_region_is_not_a_feed() {
        let mut cards = String::new();
        for i in 1..=10 {
            cards.push_str(&format!(
                "<div><h3><a href=\"/post/{i}\">Запись {i}</a></h3><p>{TEXT}</p></div>"
            ));
        }
        let html = format!(
            "<html><body><div><h3><a href=\"/top\">Самое читаемое</a></h3>{cards}\
             <footer>Все права</footer></div></body></html>"
        );

        assert!(listing(&Document::from(html), "https://example.org/").is_none());
    }

    /// Аватар в ленте — значок, а не картинка: он один заполняет ссылку,
    /// объявлен мелким и без подписи.
    #[test]
    fn an_icon_filling_a_link_is_dropped() {
        let img = r#"<p><a href="https://example.org/users/kot"><img src="https://example.org/avatar.jpg" width="24" height="24" alt=""></a></p>"#;
        let article = extract(&page(img), "https://example.org/feed").unwrap();
        assert!(!article.content_html.contains("avatar.jpg"));
    }

    /// А та же мелкая картинка в строке текста — картинка: так tonsky
    /// ставит логотип apple посреди абзаца.
    #[test]
    fn a_small_picture_in_the_text_survives() {
        let img = r#"<p>и вы увидите <img src="https://example.org/logo.png" width="16" height="16" alt=""> вот это</p>"#;
        let article = extract(&page(img), "https://example.org/post").unwrap();
        assert!(article.content_html.contains("logo.png"));
    }

    #[test]
    fn a_placeholder_is_still_replaced() {
        let img = r#"<img loading="lazy" src="data:image/gif;base64,R0lGODlhAQABAAAAACH5BAEKAAEALAAAAAABAAEAAAICTAEAOw==" data-src="https://upload.example.org/commons/portrait.jpg">"#;
        let article = extract(&page(img), "https://example.org/wiki/Arrow").unwrap();
        assert!(
            article
                .content_html
                .contains("upload.example.org/commons/portrait.jpg")
        );
    }

    /// Ссылка на абзац, внутри которого она же и лежит, — постоянная
    /// ссылка, а не метка сноски. Не отличив, мы уносили в сноски саму
    /// статью: так набрано «Ководство» и примечания bizibah.
    #[test]
    fn a_permalink_to_its_own_block_is_not_a_footnote() {
        let text = TEXT;
        let page = format!(
            "<html><body><article><h1>Эрроу</h1>             <div id=\"01\"><div><a href=\"#01\">01</a></div><p>{text}</p></div>             <div id=\"02\"><div><a href=\"#02\">02</a></div><p>{text}</p></div>             <div id=\"03\"><div><a href=\"#03\">03</a></div><p>{text}</p></div>             </article></body></html>"
        );
        let article = extract(&page, "https://example.org/kovodstvo/188/").unwrap();
        assert!(
            article.notes.bodies.is_empty(),
            "сноски: {:?}",
            article.notes
        );
        assert!(article.content_html.contains("Кеннет Эрроу"));
        // Адрес у такой ссылки ведёт туда, где читатель уже стоит: снимаем
        // его, но номер пункта оставляем текстом.
        assert!(!article.content_html.contains("href=\"#01\""));
        assert!(article.content_html.contains("01"));
    }

    /// А заголовок, который сам себе ссылка, теряет только адрес: выбросить
    /// его целиком значит потерять раздел (matklad). Исключение — текст,
    /// набранный якорем («#section2» у alistapart): там читать нечего.
    #[test]
    fn a_self_linked_heading_keeps_its_text() {
        let text = TEXT;
        let page = format!(
            "<html><body><article><h1>Эрроу</h1><p>{text}</p>             <h2 id=\"rust-analyzer\"><a href=\"#rust-analyzer\">rust-analyzer</a></h2><p>{text}</p>             <h2 id=\"section2\">Как считают<a href=\"#section2\">#section2</a></h2><p>{text}</p>             </article></body></html>"
        );
        let article = extract(&page, "https://example.org/post").unwrap();
        assert!(article.content_html.contains("rust-analyzer"));
        assert!(article.content_html.contains("Как считают"));
        assert!(!article.content_html.contains("#section2"));
    }

    /// Иллюстрация, выпавшая внутри статьи, возвращается тем же приёмом,
    /// что и проза: галерея ixbt лежит в своём блоке без текста, и правило
    /// про долю ссылок выносит её целиком.
    #[test]
    fn a_lost_illustration_comes_back() {
        let text = TEXT;
        let page = format!(
            "<html><body><article><h1>Эрроу</h1><p>{text}</p>             <div class=\"gallery\"><figure>             <a href=\"https://example.org/big.jpg\"><img src=\"/small.jpg\" alt=\"Фото 1\" width=\"1045\" height=\"740\"></a>             </figure></div>             <p>{text}</p><p>{text}</p></article></body></html>"
        );
        let article = extract(&page, "https://example.org/post").unwrap();
        assert!(
            article
                .content_html
                .contains("https://example.org/small.jpg"),
            "{}",
            article.content_html
        );
    }

    /// Но вернуться она должна один раз: адрес на странице относительный,
    /// а в извлечённом уже развёрнутый, и сверяя как есть мы не узнавали
    /// в них одну и ту же картинку.
    #[test]
    fn an_illustration_already_shown_is_not_doubled() {
        let text = TEXT;
        let page = format!(
            "<html><body><article><h1>Эрроу</h1><p>{text}</p>             <p><img src=\"/chart.svg\" alt=\"График\" width=\"800\" height=\"600\"></p>             <p>{text}</p><p>{text}</p></article></body></html>"
        );
        let article = extract(&page, "https://example.org/post").unwrap();
        assert_eq!(article.content_html.matches("chart.svg").count(), 1);
    }

    /// Что возвращать картинкой, а что нет. Ссылка на файл картинки — это
    /// «открыть в полном размере», часть статьи; ссылка на страницу —
    /// карточка «читайте ещё». Подвал и боковая колонка не иллюстрации
    /// вовсе, а `<figure>`, наоборот, родное место картинки.
    #[test]
    fn only_illustrations_are_worth_returning() {
        let doc = Document::from(
            r#"<html><body>
               <figure><a href="https://example.org/big.jpg"><img id="full" src="/small.jpg"></a></figure>
               <div><a href="https://example.org/other-story"><img id="teaser" src="/teaser.jpg"></a></div>
               <footer><img id="under" src="/logo.png"></footer>
               <p><img id="badge" src="/flag.svg" width="16" height="16"></p>
               </body></html>"#
                .to_string(),
        );
        let shot = |id: &str| worth_shot(doc.select(id).nodes().first().expect(id));
        assert!(shot("#full"));
        assert!(!shot("#teaser"));
        assert!(!shot("#under"));
        assert!(!shot("#badge"));
    }

    /// Навигацию берём из меню и подвала, а отсеиваем по трём признакам:
    /// чужой хост, пустая или слишком длинная подпись, повтор адреса.
    /// Подпись в два этажа сводим к первому — это название раздела.
    #[test]
    fn site_navigation_is_menu_and_footer_only() {
        let page = "<html><body>\
             <div class=\"menu\"><a href=\"/news/\"><div>Новости</div><div>анонсы и слухи</div></a>\
             <a href=\"/news/\">Новости</a>\
             <a href=\"https://t.me/example\">Мы в телеграме</a></div>\
             <p><a href=\"/article/other\">ссылка из текста</a></p>\
             <footer><a href=\"/about\">О проекте</a>\
             <a href=\"/legal\">Этот текст слишком длинный, чтобы быть пунктом меню сайта</a>\
             <a href=\"/here\"><img src=\"/i.png\"></a></footer>\
             </body></html>";
        let doc = Document::from(page.to_string());
        let rows = site(&doc, "https://example.org/article/one");
        let titles: Vec<&str> = rows.iter().map(|link| link.title.as_str()).collect();
        assert_eq!(titles, vec!["Новости", "О проекте"]);
        assert_eq!(rows[0].address, "https://example.org/news/");
    }

    /// Ссылка на страницу, на которой читатель и стоит, никуда не ведёт.
    #[test]
    fn site_navigation_skips_the_page_we_are_on() {
        let page = "<html><body><nav>\
             <a href=\"/here\">Эта страница</a><a href=\"/there\">Другая</a>\
             </nav></body></html>";
        let doc = Document::from(page.to_string());
        let rows = site(&doc, "https://example.org/here");
        assert_eq!(rows.len(), 1);
        assert_eq!(rows[0].title, "Другая");
    }

    /// Ссылка, которая без JS не делает ничего, — кнопка, а не ссылка.
    /// Вокруг комментариев их особенно густо: у opennet каждая реплика
    /// несёт «[+]/[–]», «свернуть ветку» и «к родителю».
    #[test]
    fn a_link_that_leads_nowhere_is_a_button() {
        let doc = Document::from(
            r##"<html><body><p id="p">оценка [<a href="#">+</a>/<a href="#">–</a>]
               [<a href="#1">^</a>] <a href="javascript:more()">Показать ещё</a>
               <a href="https://e.com/reply">ответить</a></p></body></html>"##
                .to_string(),
        );
        unbutton(&doc);
        let html = doc.select("#p").inner_html().to_string();
        // Значок исчезает вместе со скобками и разделителем.
        assert!(!html.contains('+'), "{html}");
        assert!(!html.contains('^'), "{html}");
        assert!(!html.contains("[]"), "{html}");
        // Осмысленный текст остаётся, но уже не притворяется ссылкой.
        assert!(html.contains("Показать ещё"), "{html}");
        assert!(!html.contains("javascript:"), "{html}");
        // Настоящая ссылка не тронута.
        assert!(html.contains(r##"href="https://e.com/reply""##), "{html}");
    }

    /// Заглавную картинку берём из карточки Open Graph — но только если
    /// страница объявила себя статьёй и своей картинки в ней нет ни одной.
    #[test]
    fn a_cover_comes_from_the_open_graph_card() {
        let text = TEXT;
        let head = "<meta property=\"og:type\" content=\"article\">                    <meta property=\"og:image\" content=\"https://cdn.example.org/lead.jpg\">                    <meta property=\"og:image:alt\" content=\"Бегун\">";
        let page = format!(
            "<html><head>{head}</head><body><article><h1>Налог</h1>             <p>{text}</p><p>{text}</p><p>{text}</p></article></body></html>"
        );
        let article = extract(&page, "https://example.org/news/tax").unwrap();
        assert!(article.content_html.contains("cdn.example.org/lead.jpg"));
        assert!(article.content_html.contains("Бегун"));
    }

    #[test]
    fn a_cover_does_not_double_a_picture_the_article_already_has() {
        let text = TEXT;
        let head = "<meta property=\"og:type\" content=\"article\">                    <meta property=\"og:image\" content=\"https://cdn.example.org/lead.jpg\">";
        let page = format!(
            "<html><head>{head}</head><body><article><h1>Налог</h1>             <p><img src=\"https://cdn.example.org/own.jpg\" alt=\"Своя\" width=\"800\" height=\"600\"></p>             <p>{text}</p><p>{text}</p><p>{text}</p></article></body></html>"
        );
        let article = extract(&page, "https://example.org/news/tax").unwrap();
        assert!(!article.content_html.contains("lead.jpg"));
    }

    /// Страница, которая статьёй себя не объявляла, заглавной картинки
    /// не получает: в карточке у такой лежит логотип сайта.
    #[test]
    fn a_page_that_is_not_an_article_gets_no_cover() {
        let text = TEXT;
        let head = "<meta property=\"og:type\" content=\"website\">                    <meta property=\"og:image\" content=\"https://cdn.example.org/logo.png\">";
        let page = format!(
            "<html><head>{head}</head><body><article><h1>Раздел</h1>             <p>{text}</p><p>{text}</p><p>{text}</p></article></body></html>"
        );
        let article = extract(&page, "https://example.org/section").unwrap();
        assert!(!article.content_html.contains("logo.png"));
    }
}
