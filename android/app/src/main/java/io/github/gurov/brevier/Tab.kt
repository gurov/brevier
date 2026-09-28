package io.github.gurov.brevier

/**
 * Вкладка: своя статья, своя история, своё место в тексте.
 *
 * История — та же модель, что `history.rs` в ядре: переход с середины
 * обрубает всё, что было впереди, повтор текущего адреса записью не считается,
 * и у каждой записи своё место чтения — смещение в тексте, а не пиксели.
 * Корень истории — начальная страница: «назад» с первой страницы ведёт на неё.
 */
class Tab(val id: Long) {
    val addresses = mutableListOf<String>()
    private val places = mutableListOf<Int>()

    /** Место в пути; `-1` — начальная страница, перед первой записью. */
    var at = -1
        private set

    /**
     * Виджет статьи. Есть только у видимой вкладки: фоновая его отдаёт
     * и рисуется заново, когда её покажут (`MainActivity.switchTo`).
     */
    var view: ArticleView? = null

    /** Что показано. `null` — начальная страница или загрузка. */
    var shown: Loaded? = null
    var title = "New tab"

    /**
     * Кэш «назад/вперёд»: уже показанные страницы по адресу. «Назад» рисует
     * страницу из него сразу, без сети, — как bfcache у браузеров. Живёт ровно
     * на путь истории: обрубленное переходом уходит (`prune`).
     */
    val pages = HashMap<String, Loaded>()

    /** Номер загрузки: ответ брошенной страницы отличаем по нему. */
    var generation = 0L
    var loading = false

    /** Куда вернуть читателя, когда страница приедет: смещение в тексте. */
    var resume: Int? = null

    /**
     * Вкладка есть, страницы ещё нет: так возвращается из сессии всё, кроме
     * той вкладки, что была впереди. Грузится она, когда на неё переключились.
     */
    var pending = false

    /** Точки входа в документацию проекта и для какого проекта они найдены. */
    var entries: List<Entry> = emptyList()
    var entriesFor: String? = null

    /** Страница только что приехала: прокрутить к решётке из адреса. */
    var anchorPending = false

    /** Сколько точек у «Loading» сейчас — чтобы вернувшаяся вкладка не мигала. */
    var dots = 1

    fun current(): String? = addresses.getOrNull(at)

    /** Переход обрубает всё впереди; с начальной страницы впереди весь путь. */
    fun visit(address: String) {
        if (current() == address) return
        while (addresses.size > at + 1) addresses.removeAt(addresses.size - 1)
        while (places.size > at + 1) places.removeAt(places.size - 1)
        addresses += address
        places += 0
        at = addresses.size - 1
    }

    /** История, поднятая из сессии: путь и место в нём. */
    fun restore(path: List<String>, at: Int) {
        addresses.clear()
        places.clear()
        addresses += path
        repeat(path.size) { places += 0 }
        this.at = if (path.isEmpty()) -1 else at.coerceIn(0, path.size - 1)
    }

    fun setPlace(place: Int) {
        if (at in places.indices) places[at] = place
    }

    fun place(): Int = places.getOrNull(at) ?: 0

    /** Открыта страница — значит, за ней есть куда вернуться: хотя бы на начальную. */
    fun canGoBack() = current() != null
    fun canGoForward() = at + 1 < addresses.size

    /** На первой странице пути: «назад» отсюда — на начальную. */
    fun atFirst() = at == 0 && addresses.isNotEmpty()

    /** Шаг назад; куда пришли, говорит `current()`: `null` — начальная страница. */
    fun back(): Boolean {
        if (!canGoBack()) return false
        at -= 1
        return true
    }

    fun forward(): Boolean {
        if (!canGoForward()) return false
        at += 1
        return true
    }

    /** Выбросить из кэша страницы, до которых по истории больше не дойти. */
    fun prune() {
        pages.keys.retainAll(addresses.toSet())
    }
}
