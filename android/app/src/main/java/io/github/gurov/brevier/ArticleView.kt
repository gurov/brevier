package io.github.gurov.brevier

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.Rect
import android.graphics.drawable.GradientDrawable
import android.os.Build
import android.text.Layout
import android.text.Selection
import android.text.Spannable
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.TextPaint
import android.text.method.LinkMovementMethod
import android.text.style.ClickableSpan
import android.text.style.ForegroundColorSpan
import android.text.style.ReplacementSpan
import android.view.ActionMode
import android.view.Choreographer
import android.view.Gravity
import android.view.MenuItem
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.widget.Button
import android.widget.FrameLayout
import android.widget.HorizontalScrollView
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ScrollView
import android.widget.TableLayout
import android.widget.TableRow
import android.widget.TextView
import java.nio.ByteBuffer
import java.util.Locale
import kotlin.math.abs
import kotlin.math.max
import kotlin.math.min

/** Что статья просит у окна: она сама не знает ни вкладок, ни истории. */
interface ArticleHost {
    /** Нажали ссылку. `fresh` — открыть новой вкладкой. */
    fun follow(target: String, fresh: Boolean)

    /** Долгое нажатие на ссылку: что с ней сделать. `x`, `y` — точка на экране. */
    fun linkMenu(target: String, x: Int, y: Int)

    /** Отдать адрес системному браузеру: страницу, которую мы не показали. */
    fun openOutside(target: String)

    /** Нажали картинку (#38): сохранить, поделиться, открыть в браузере. */
    fun pictureMenu(source: String)

    /**
     * Выделенное `[from, to)` — в заметку (#39). `write` — приписать слова,
     * иначе просто отметить. Смещения — в тексте страницы, UTF-16.
     */
    fun mark(from: Int, to: Int, write: Boolean)

    /** Нажали отмеченное: что сделать с заметкой. `x`, `y` — точка на экране. */
    fun noteMenu(id: Long, x: Int, y: Int)

    /** Нажали картинку заметки (#42): показать её крупно. */
    fun notePicture(id: Long, path: String)

    /** Страницу прокрутили: полка отмечает, где читатель. */
    fun scrolled()

    /**
     * Палец сдвинул страницу на `dy` пикселей (#45): шапка идёт за ним.
     * Прыжки, броски после отпускания и подгрузка картинок сюда не доходят.
     */
    fun dragged(dy: Int)

    /** Палец отпустил страницу (`pinch` — это был щипок): шапке — к ближнему краю. */
    fun released(pinch: Boolean)

    /** Читатель выделяет текст — или кончил (#45): шапка на месте, пока выделяет. */
    fun selecting(on: Boolean)

    /** Щипок: ступень масштаба вверх или вниз. */
    fun zoom(step: Int)
}

/**
 * Статья: прокрутка, в ней колонка в меру, в колонке куски текста
 * (`TextView` на кусок между объектами), картинки и таблицы.
 *
 * Кусок, а не весь текст одним виджетом: картинку и таблицу посреди
 * `TextView` не поставить, а выделение внутри куска — от объекта до объекта —
 * это почти всегда то, что копируют. Формула в строке остаётся в тексте
 * спаном, как холст в буфере у окна.
 */
@SuppressLint("ViewConstructor")
class ArticleView(context: Context, private val host: ArticleHost) : ScrollView(context) {
    private val column = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
    private val fonts = Fonts.of(context)

    var page: Page? = null
        private set
    private lateinit var metrics: Metrics
    private lateinit var palette: Palette
    private var theme = Theme(dark = false, high = false)

    private val proses = mutableListOf<ProseView>()
    private val images = mutableListOf<ImageFrame>()
    private val tables = mutableListOf<TableFrame>()
    private val formulas = mutableListOf<Pair<FormulaSpan, ProseView>>()

    /** Номер показа: ответ картинки для прошлой страницы уже никому не нужен. */
    private var generation = 0

    /**
     * Куски, которые ещё не в колонке (#22): первый экран показываем сразу,
     * остальное досыпаем по куску за кадр. Кто прыгает в недосыпанное
     * (якорь, поиск, место чтения), досыпает до цели сам — см. [ensure].
     */
    private val pending = ArrayDeque<Any>()
    /** Докуда текст страницы уже в колонке — смещение за последним куском. */
    private var laid = 0
    private var eager = false
    private var offer: String? = null
    private var ways: List<Entry> = emptyList()
    /** Подсветка поиска — её получают и куски, досыпанные позже. */
    private var marks: List<Hit>? = null
    private var markHere = -1
    /** Заметки страницы (#39) — их тоже получают и досыпанные куски. */
    private var notes: List<NoteSpot> = emptyList()

    /**
     * Карточки заметок под абзацами (#42): слова и картинки читателя
     * под абзацем, где кончается цитата. Места справа от колонки на телефоне
     * нет, а в окне так же встают заметки, когда поле справа не влезает.
     * Колонка режется по концу такого абзаца — тем же разрезом, что длинный
     * текст (#22), — и карточка встаёт между кусками: текст страницы
     * и его смещения не меняются. Выключено настройкой — только подложка.
     */
    var cards = true
    /** По каким карточкам нарезана колонка: конец абзаца и его заметки. */
    private var laidCards: List<Pair<Int, List<NoteSpot>>> = emptyList()

    /**
     * Шапка лежит поверх статьи (#45): сверху у статьи поле в её рост
     * (`reserve`), текст под шапку уходит, не сдвигаясь, когда она уезжает.
     * `covered` — сколько сверху закрыто шапкой сейчас: место чтения, прыжки
     * и прочитанное меряются от её нижнего края (`visibleTop`).
     */
    var covered = 0
    /** Прокрутку ведёт палец: только её сдвиг достаётся шапке. */
    private var dragging = false

    init {
        isFillViewport = true
        isVerticalScrollBarEnabled = true
        // Фокус держит сама статья: иначе, когда кусок текста с фокусом
        // уходит вместе со старой страницей, Android отдаёт фокус первому
        // полю ввода — адресной строке, и та начинает принимать «назад».
        isFocusableInTouchMode = true
        descendantFocusability = FOCUS_BEFORE_DESCENDANTS
        clipToPadding = false
        addView(column, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    }

    /**
     * Показать страницу. `offer` — кнопка «Open in your browser» под текстом
     * отказа, `ways` — копии страницы (#9) кнопками над ней: предлагать их
     * решает ядро.
     */
    fun show(
        page: Page,
        metrics: Metrics,
        theme: Theme,
        eager: Boolean,
        offer: String? = null,
        ways: List<Entry> = emptyList(),
        notes: List<NoteSpot> = emptyList(),
    ) {
        this.page = page
        this.metrics = metrics
        this.theme = theme
        palette = metrics.type.palette(theme)
        setBackgroundColor(palette.paper)
        if (hasFocus()) requestFocus()
        marks = null
        markHere = -1
        this.notes = notes
        laidCards = cardsOf(page, notes)
        this.eager = eager
        this.offer = offer
        this.ways = ways
        fitColumn(width)
        build()
        jump { scrollTo(0, 0) }
    }

    /** Собрать колонку заново из страницы — при показе и когда сменились карточки. */
    private fun build() {
        val page = page ?: return
        generation += 1
        column.removeAllViews()
        proses.clear()
        images.clear()
        tables.clear()
        formulas.clear()
        pending.clear()
        laid = 0

        // Первый экран — сразу и целиком, остальное — по куску за кадр.
        // Последний кусок `append` закрывает сам (`finish`); здесь — только
        // страница без кусков вовсе, иначе кнопка отказа вставала дважды.
        pending.addAll(withCards(pieces(page, metrics.justify), laidCards))
        if (pending.isEmpty()) finish()
        while (pending.isNotEmpty() && laid < FIRST_CHARS) append()
        if (pending.isNotEmpty()) drip(generation)
        if (eager) loadAll()
    }

    /** Положить в колонку следующий кусок. */
    private fun append() {
        val page = page ?: return
        when (val item = pending.removeFirst()) {
            is Piece -> {
                addProse(page, item)
                noteView(proses.last())
                marks?.let { hits -> markView(proses.last(), hits, markHere) }
                laid = item.end
            }
            is Block.Image -> {
                addImage(item)
                if (eager) images.last().load()
                laid = item.at + 1
            }
            is Block.Table -> {
                addTable(item)
                marks?.let { hits -> tables.last().mark(hits, markHere) }
                laid = item.at + 1
            }
            is CardSlot -> item.notes.forEach { addCard(item.at, it) }
        }
        if (pending.isEmpty()) finish()
    }

    /** Все куски на месте: кнопки отказа и поле внизу. */
    private fun finish() {
        // Копия открывается здесь же, как ссылка; первая кнопка — с воздухом
        // от текста, остальные — плотнее.
        ways.forEachIndexed { index, way -> addButton(way.title, index == 0) { host.follow(way.address, false) } }
        offer?.let { target -> addButton("Open in your browser", ways.isEmpty()) { host.openOutside(target) } }
        // Пустое место внизу: последнюю строку можно поднять с края экрана.
        column.addView(View(context), LinearLayout.LayoutParams(1, (metrics.text * 4).toInt()))
    }

    /** Досыпать кусок на следующем кадре — и так, пока не кончатся. */
    private fun drip(shown: Int) {
        postOnAnimation {
            if (shown != generation || pending.isEmpty()) return@postOnAnimation
            append()
            if (pending.isNotEmpty()) drip(shown)
        }
    }

    /** Досыпать куски до смещения `offset` сейчас же: к нему прыгают. */
    private fun ensure(offset: Int) {
        while (pending.isNotEmpty() && laid <= offset) append()
    }

    private fun addProse(page: Page, piece: Piece) {
        val view = ProseView(context, piece)
        val text = typeset(page, piece, metrics, palette, fonts) { block ->
            FormulaSpan(block, formulaText(block.alt), palette.dim)
        }
        // Ссылка ещё и `ClickableSpan`: так её видит TalkBack и предлагает
        // в своём списке ссылок. Касание пальцем разбирает сам кусок
        // (`linkAt`): выделяемый `TextView` такой спан на касание не зовёт,
        // и двойного перехода не будет. Вид ссылки задают стили ядра.
        for (link in page.links) {
            val from = maxOf(link.start, piece.start) - piece.start
            val to = minOf(link.end, piece.start + text.length) - piece.start
            if (from >= to || from < 0) continue
            text.setSpan(object : ClickableSpan() {
                override fun onClick(widget: View) = host.follow(link.target, false)
                override fun updateDrawState(paint: TextPaint) {}
            }, from, to, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }
        view.setup(metrics, palette, fonts)
        view.setText(text, TextView.BufferType.SPANNABLE)
        val spannable = view.text as Spannable
        for (span in spannable.getSpans(0, spannable.length, FormulaSpan::class.java)) {
            formulas += span to view
        }
        proses += view
        column.addView(view, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    }

    private fun addImage(block: Block.Image) {
        val frame = ImageFrame(context, block)
        images += frame
        val margin = metrics.px(6f).toInt()
        column.addView(frame, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
            topMargin = margin
            bottomMargin = margin + metrics.extra.toInt()
        })
        frame.idle(null)
    }

    private fun addTable(block: Block.Table) {
        val table = TableFrame(context, block)
        tables += table
        column.addView(table, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
            topMargin = metrics.px(10f).toInt()
            bottomMargin = metrics.px(14f).toInt() + metrics.extra.toInt()
        })
    }

    /**
     * Кнопка, а не только пункт меню: на странице, где ничего не показалось,
     * читателю нужен выход, а не память о меню.
     */
    private fun addButton(label: String, first: Boolean, action: () -> Unit) {
        val button = Button(context).apply {
            text = label
            isAllCaps = false
            typeface = fonts.regular
            setTextColor(palette.ink)
            background = GradientDrawable().apply {
                setColor(palette.shelf)
                setStroke(max(1, metrics.px(1f).toInt()), palette.rule)
                cornerRadius = metrics.px(6f)
            }
            val side = metrics.px(14f).toInt()
            setPadding(side, metrics.px(8f).toInt(), side, metrics.px(8f).toInt())
            minHeight = 0
            minimumHeight = 0
            setOnClickListener { action() }
        }
        column.addView(button, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
            topMargin = if (first) (metrics.extra * 2).toInt() else metrics.px(8f).toInt()
            gravity = Gravity.START
        })
    }

    /** Загрузить все картинки: переключатель в настройках включён. */
    fun loadAll() {
        eager = true
        images.forEach { it.load() }
        formulas.forEach { (span, view) -> load(span, view) }
    }

    // ── колонка ─────────────────────────────────────────────────────────────

    // Поля колонки ставятся до измерения детей: поставленные после, посреди
    // раскладки, они оставляли текст со старой шириной — строки уходили
    // за правый край.
    override fun onMeasure(widthMeasureSpec: Int, heightMeasureSpec: Int) {
        fitColumn(MeasureSpec.getSize(widthMeasureSpec))
        super.onMeasure(widthMeasureSpec, heightMeasureSpec)
    }

    /**
     * Колонка в меру: не шире 36 кеглей, по бокам поля. На телефоне мера
     * обычно шире экрана, и колонка — это весь экран без полей.
     */
    private fun fitColumn(width: Int) {
        if (width <= 0 || !::metrics.isInitialized) return
        val gutter = metrics.text
        val side = max(gutter, (width - metrics.measure) / 2f).toInt()
        val top = (metrics.extra * 2).toInt()
        if (column.paddingLeft != side || column.paddingRight != side || column.paddingTop != top) {
            column.setPadding(side, top, side, 0)
        }
    }

    /** Ширина колонки в пикселях: под неё декодируются картинки. */
    private fun columnWidth(): Int =
        max(1, (if (width > 0) width else resources.displayMetrics.widthPixels) - column.paddingLeft - column.paddingRight)

    // ── место в тексте ──────────────────────────────────────────────────────

    /** Смещение в тексте на высоте `y` колонки. `null` — ниже текста. */
    fun offsetAt(y: Int): Int? {
        for (index in 0 until column.childCount) {
            val child = column.getChildAt(index)
            if (y > child.bottom) continue
            return when (child) {
                is ProseView -> {
                    val layout = child.layout ?: return child.piece.start
                    val line = layout.getLineForVertical(max(0, y - child.top - child.totalPaddingTop))
                    child.piece.start + layout.getLineStart(line)
                }
                is ImageFrame -> child.block.at
                is TableFrame -> child.block.at
                is NoteCard -> child.at
                else -> null
            }
        }
        return null
    }

    /** Где читатель: смещение строки у верхнего края. От масштаба не зависит. */
    fun place(): Int = offsetAt(visibleTop()) ?: 0

    /** Поле сверху под шапку (#45): её полный рост. */
    fun reserve(top: Int) {
        if (paddingTop != top) setPadding(0, top, 0, 0)
    }

    /** Верх видимого — под шапкой, если она видна, — на высоте колонки. */
    fun visibleTop(): Int = scrollY - paddingTop + covered

    /** Низ видимого на высоте колонки: снизу статью ничего не закрывает. */
    fun visibleBottom(): Int = scrollY - paddingTop + height

    /** Сколько статьи видно: окно без шапки. */
    fun visibleHeight(): Int = height - covered

    /** Где на высоте колонки стоит это смещение. */
    private fun yOf(offset: Int): Int? {
        for (index in 0 until column.childCount) {
            when (val child = column.getChildAt(index)) {
                is ProseView -> {
                    val end = child.piece.start + child.text.length
                    if (offset in child.piece.start..end) {
                        val layout = child.layout ?: return child.top
                        val line = layout.getLineForOffset(offset - child.piece.start)
                        return child.top + child.totalPaddingTop + layout.getLineTop(line)
                    }
                    if (offset < child.piece.start) return child.top
                }
                is ImageFrame -> if (child.block.at >= offset) return child.top
                is TableFrame -> if (child.block.at >= offset) return child.top
            }
        }
        return null
    }

    /** Прокрутить так, чтобы смещение стояло на доле высоты окна. */
    fun scrollToOffset(offset: Int, align: Float) {
        ensure(offset)
        val y = yOf(offset) ?: return
        jump { scrollTo(0, max(0, y + paddingTop - covered - (visibleHeight() * align).toInt())) }
    }

    /** Прокрутить не пальцем — прыжком: шапку (#45) это не двигает. */
    private fun jump(move: () -> Unit) {
        val was = dragging
        dragging = false
        move()
        dragging = was
    }

    /**
     * Прокрутка, которая доводит дело до конца: картинки и таблицы добирают
     * высоту не сразу, и одного прыжка мало — он оказывается выше цели.
     * Повторяем кадр за кадром, пока высота не устоится, — как `settle` в окне.
     */
    fun settle(offset: Int, align: Float) {
        val shown = generation
        var left = SETTLE_FRAMES
        var was = -1
        var stable = 0
        val tick = object : Choreographer.FrameCallback {
            override fun doFrame(frameTimeNanos: Long) {
                if (shown != generation) return
                scrollToOffset(offset, align)
                val height = column.height
                if (height == was) stable += 1 else {
                    was = height
                    stable = 0
                }
                left -= 1
                if (left > 0 && !(height > 0 && stable >= 5)) {
                    Choreographer.getInstance().postFrameCallback(this)
                }
            }
        }
        Choreographer.getInstance().postFrameCallback(tick)
    }

    override fun onScrollChanged(l: Int, t: Int, oldl: Int, oldt: Int) {
        super.onScrollChanged(l, t, oldl, oldt)
        if (dragging && t != oldt) host.dragged(t - oldt)
        host.scrolled()
    }

    // ── поиск ───────────────────────────────────────────────────────────────

    /** Подсветить совпадения; `here` — то, на котором стоим. */
    fun highlight(hits: List<Hit>, here: Int) {
        marks = hits.takeIf { it.isNotEmpty() }
        markHere = here
        for (view in proses) markView(view, hits, here)
        for (table in tables) table.mark(hits, here)
        when (val current = hits.getOrNull(here)) {
            is Hit.Text -> scrollToOffset(current.from, 0.3f)
            is Hit.Cell -> scrollToCell(current, 0.3f)
            null -> {}
        }
    }

    /** Подсветка совпадений в одном куске. */
    private fun markView(view: ProseView, hits: List<Hit>, here: Int) {
        view.clearMarks()
        val type = metrics.type
        val start = view.piece.start
        val end = start + view.text.length
        hits.forEachIndexed { index, hit ->
            if (hit !is Hit.Text) return@forEachIndexed
            val a = max(hit.from, start)
            val b = min(hit.to, end)
            if (a < b) view.mark(a - start, b - start, if (index == here) type.foundHere else type.found, type.foundInk)
        }
    }

    /**
     * Подвести ячейку на долю высоты окна (#10). Таблица могла ещё не лечь
     * в колонку — сначала докладываем до неё; место ячейки — после раскладки.
     * Широкая таблица листается вбок и к ячейке.
     */
    private fun scrollToCell(hit: Hit.Cell, align: Float) {
        ensure(hit.at + 1)
        post {
            val frame = tables.firstOrNull { it.block === hit.table } ?: return@post
            val cell = frame.cells.getOrNull(hit.cell) ?: return@post
            val box = Rect()
            cell.getDrawingRect(box)
            frame.offsetDescendantRectToMyCoords(cell, box)
            jump { scrollTo(0, max(0, frame.top + box.top + paddingTop - covered - (visibleHeight() * align).toInt())) }
            if (box.left < frame.scrollX || box.right > frame.scrollX + frame.width) {
                frame.scrollTo(max(0, box.left - metrics.px(16f).toInt()), 0)
            }
        }
    }

    // ── заметки (#39) ───────────────────────────────────────────────────────

    /**
     * Закрасить заметки страницы. Зовётся после каждой правки. Сменились
     * карточки под абзацами — колонка собирается заново на том же месте.
     */
    fun notes(list: List<NoteSpot>) {
        notes = list
        val page = page
        val wanted = if (page != null) cardsOf(page, list) else emptyList()
        if (page != null && key(wanted) != key(laidCards)) {
            val place = place()
            laidCards = wanted
            build()
            settle(place, 0f)
            return
        }
        for (view in proses) noteView(view)
    }

    /** Включить или выключить карточки под абзацами. */
    fun showCards(on: Boolean) {
        if (cards == on) return
        cards = on
        notes(notes)
    }

    /** Карточки страницы: конец абзаца, где кончается цитата, и заметки под ним. */
    private fun cardsOf(page: Page, list: List<NoteSpot>): List<Pair<Int, List<NoteSpot>>> {
        if (!cards) return emptyList()
        val text = page.text
        return list.filter { it.placed && it.says && it.to <= text.length }
            .groupBy { note ->
                val end = text.indexOf('\n', max(note.from, note.to - 1))
                if (end < 0) text.length else end + 1
            }
            .toSortedMap()
            .map { (at, notes) -> at to notes.sortedBy { it.from } }
    }

    /** Что в карточках видно: по нему решаем, собирать ли колонку заново. */
    private fun key(laid: List<Pair<Int, List<NoteSpot>>>) =
        laid.map { (at, notes) -> at to notes.map { Triple(it.id, it.text, it.pictures) } }

    /** Место карточки в очереди кусков: после абзаца `at`. */
    private class CardSlot(val at: Int, val notes: List<NoteSpot>)

    /**
     * Разрезать куски по концам абзацев с карточками и поставить карточки
     * после них. Разрез — как у длинного текста (`prose`): кусок кончается
     * переводом строки абзаца. Перед объектом этот перевод строки кусок
     * уже потерял (`pieces`), поэтому и конец на знак раньше — свой.
     */
    private fun withCards(items: List<Any>, laid: List<Pair<Int, List<NoteSpot>>>): List<Any> {
        if (laid.isEmpty()) return items
        val out = mutableListOf<Any>()
        var next = 0
        for (item in items) {
            if (item !is Piece) {
                out += item
                continue
            }
            var piece: Piece = item
            while (next < laid.size && laid[next].first > piece.start && laid[next].first < piece.end) {
                val at = laid[next].first
                out += Piece(piece.start, at, piece.justified)
                out += CardSlot(at, laid[next].second)
                piece = Piece(at, piece.end, piece.justified)
                next += 1
            }
            out += piece
            while (next < laid.size && laid[next].first <= piece.end + 1) {
                out += CardSlot(laid[next].first, laid[next].second)
                next += 1
            }
        }
        while (next < laid.size) {
            out += CardSlot(laid[next].first, laid[next].second)
            next += 1
        }
        return out
    }

    private fun addCard(at: Int, note: NoteSpot) {
        column.addView(NoteCard(context, at, note), LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
            topMargin = metrics.px(2f).toInt()
            bottomMargin = metrics.extra.toInt() + metrics.px(4f).toInt()
        })
    }

    /** Заметки в одном куске: подложка, а у приписанного — ещё и черта. */
    private fun noteView(view: ProseView) {
        view.clearNotes()
        val start = view.piece.start
        val end = start + view.text.length
        for (note in notes) {
            if (!note.placed) continue
            val a = max(note.from, start)
            val b = min(note.to, end)
            if (a < b) view.note(a - start, b - start, palette.note, if (note.text.isEmpty()) 0 else palette.noteRule)
        }
    }

    /** Заметка под смещением страницы: нажатое отмеченное. */
    private fun noteAt(offset: Int): NoteSpot? = notes.firstOrNull { it.placed && offset >= it.from && offset < it.to }

    // ── щипок ───────────────────────────────────────────────────────────────

    private var pinch = 1f
    private val scaler = ScaleGestureDetector(context, object : ScaleGestureDetector.SimpleOnScaleGestureListener() {
        override fun onScaleBegin(detector: ScaleGestureDetector): Boolean {
            pinch = 1f
            return true
        }

        override fun onScale(detector: ScaleGestureDetector): Boolean {
            pinch *= detector.scaleFactor
            // Ступень берётся, когда щипок ушёл на четверть: мельче — это дрожь
            // пальцев, а не просьба.
            if (pinch > 1.25f) {
                pinch = 1f
                host.zoom(+1)
            } else if (pinch < 0.8f) {
                pinch = 1f
                host.zoom(-1)
            }
            return true
        }
    })

    override fun dispatchTouchEvent(event: MotionEvent): Boolean {
        scaler.onTouchEvent(event)
        if (scaler.isInProgress) {
            if (dragging) host.released(pinch = true)
            dragging = false
            return true
        }
        // Шапка (#45) идёт за пальцем, как у Firefox: от касания до отпускания;
        // бросок после него страницу листает, а шапку уже нет.
        when (event.actionMasked) {
            MotionEvent.ACTION_DOWN -> dragging = true
            MotionEvent.ACTION_UP, MotionEvent.ACTION_CANCEL -> if (dragging) {
                dragging = false
                host.released(pinch = false)
            }
        }
        return super.dispatchTouchEvent(event)
    }

    // ── картинки ────────────────────────────────────────────────────────────

    private fun look(natural: Boolean) = Look(
        width = columnWidth(),
        // Прозрачное кладём на светлую бумагу, а не на белое: белая карточка
        // посреди слоновой кости заметна, а схеме нужен только светлый фон.
        // Высокому контрасту — его белую (`palette::picture_paper`).
        paper = metrics.type.palette(theme.copy(dark = false)).paper and 0xffffff,
        fontSize = metrics.text,
        natural = natural,
    )

    private class Look(val width: Int, val paper: Int, val fontSize: Float, val natural: Boolean)

    private class Decoded(val bitmap: Bitmap?, val trouble: String?)

    private fun fetch(source: String, look: Look, done: (Decoded) -> Unit) {
        val shown = generation
        App.work.execute {
            val fell = byteArrayOf(1) + "The load fell through".toByteArray()
            val bytes = try {
                Core.image(source, look.width, look.paper, look.fontSize, look.natural) ?: fell
            } catch (failure: Throwable) {
                fell
            }
            val decoded = decode(bytes)
            post { if (shown == generation) done(decoded) }
        }
    }

    private fun decode(bytes: ByteArray): Decoded {
        if (bytes.isEmpty()) return Decoded(null, "The load fell through")
        if (bytes[0].toInt() != 0) return Decoded(null, String(bytes, 1, bytes.size - 1))
        val buffer = ByteBuffer.wrap(bytes)
        buffer.get()
        val w = buffer.int
        val h = buffer.int
        return try {
            val bitmap = Bitmap.createBitmap(w, h, Bitmap.Config.ARGB_8888)
            bitmap.copyPixelsFromBuffer(ByteBuffer.wrap(bytes, 9, w * h * 4))
            Decoded(bitmap, null)
        } catch (failure: Throwable) {
            Decoded(null, "The image did not fit in memory")
        }
    }

    private fun load(span: FormulaSpan, view: ProseView) {
        if (span.busy) return
        span.busy = true
        fetch(span.block.source, look(natural = true)) { decoded ->
            span.busy = false
            decoded.bitmap?.let { span.picture(it, view) }
        }
    }

    /**
     * Исходник формулы из `alt`: MathJax заворачивает его в `{\displaystyle …}`,
     * и читателю эта обёртка не нужна (`page::formula` в ядре).
     */
    private fun formulaText(alt: String): String {
        var text = alt.trim()
        if (text.startsWith("{") && text.endsWith("}")) {
            text = text.substring(1, text.length - 1).trim().removePrefix("\\displaystyle").trim()
        }
        return text.ifEmpty { "formula" }
    }

    /** Формула в строке: исходник, пока картинки нет; картинка ростом с текст. */
    inner class FormulaSpan(val block: Block.Image, private val fallback: String, private val ink: Int) : ReplacementSpan() {
        private var bitmap: Bitmap? = null
        var busy = false

        fun picture(bitmap: Bitmap, view: ProseView) {
            this.bitmap = bitmap
            val text = view.text as? Spannable ?: return
            val start = text.getSpanStart(this)
            val end = text.getSpanEnd(this)
            if (start < 0) return
            text.removeSpan(this)
            text.setSpan(this, start, end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        }

        override fun getSize(paint: Paint, text: CharSequence?, start: Int, end: Int, fm: Paint.FontMetricsInt?): Int {
            val picture = bitmap ?: return paint.measureText(fallback).toInt()
            if (fm != null) {
                val own = paint.fontMetricsInt
                val middle = -(paint.textSize * 0.3f).toInt()
                val top = middle - picture.height / 2
                val bottom = top + picture.height
                fm.ascent = min(own.ascent, top)
                fm.top = min(own.top, top)
                fm.descent = max(own.descent, bottom)
                fm.bottom = max(own.bottom, bottom)
            }
            return picture.width
        }

        override fun draw(canvas: Canvas, text: CharSequence?, start: Int, end: Int, x: Float, top: Int, y: Int, bottom: Int, paint: Paint) {
            val picture = bitmap
            if (picture == null) {
                val was = paint.color
                paint.color = ink
                canvas.drawText(fallback, x, y.toFloat(), paint)
                paint.color = was
                return
            }
            val middle = y - paint.textSize * 0.3f
            canvas.drawBitmap(picture, x, middle - picture.height / 2f, null)
        }
    }

    /** Место иллюстрации: рамка с подписью, по нажатию — картинка. */
    @SuppressLint("ViewConstructor")
    inner class ImageFrame(context: Context, val block: Block.Image) : LinearLayout(context) {
        private var busy = false

        init {
            orientation = VERTICAL
            gravity = Gravity.CENTER_HORIZONTAL
        }

        /** Рамка-заглушка: нажмёшь — загрузится. `trouble` — почему не вышло. */
        fun idle(trouble: String?) {
            removeAllViews()
            val name = if (block.alt.isEmpty()) "image" else "image: ${clip(block.alt, 160)}"
            val label = TextView(context).apply {
                text = if (trouble != null) "$trouble\n$name" else name
                gravity = Gravity.CENTER
                setTextColor(palette.dim)
                typeface = fonts.regular
                textSize = 0f
                setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, metrics.text * 0.85f)
                val side = metrics.px(14f).toInt()
                val tall = metrics.px(20f).toInt()
                setPadding(side, tall, side, tall)
                background = GradientDrawable().apply {
                    setStroke(max(1, metrics.px(1f).toInt()), palette.dim, metrics.px(4f), metrics.px(3f))
                    cornerRadius = metrics.px(6f)
                }
                setOnClickListener { load() }
            }
            addView(label, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT))
        }

        fun load() {
            if (busy) return
            busy = true
            removeAllViews()
            addView(caption("loading the image…"))
            fetch(block.source, look(natural = false)) { decoded ->
                busy = false
                val bitmap = decoded.bitmap
                hold {
                    if (bitmap == null) idle(decoded.trouble ?: "The load fell through")
                    else picture(bitmap)
                }
            }
        }

        /**
         * Сменить содержимое рамки, не сдвинув текст под глазами. Рамка выше
         * окна вырастет из заглушки в полный рост и толкнула бы читаемое
         * вниз — прокрутка сдвигается ровно на столько, на сколько рамка
         * выросла. Пикселями, а не местом в тексте (`place` и `settle`):
         * место округляется до строки, а пока один `settle` не дошёл до цели,
         * следующая картинка брала за место промежуточное, — на статье
         * Хабра с 29 картинками над комментарием читатель уползал на тысячу
         * знаков вверх. Прокрутка — статьи: своя `scrollY` у рамки всегда 0,
         * и до 18 октября 2026 место не держалось вовсе.
         */
        private fun hold(change: () -> Unit) {
            val above = bottom <= this@ArticleView.visibleTop()
            val before = height
            change()
            if (!above) return
            addOnLayoutChangeListener(object : View.OnLayoutChangeListener {
                override fun onLayoutChange(
                    view: View, left: Int, top: Int, right: Int, bottom: Int,
                    oldLeft: Int, oldTop: Int, oldRight: Int, oldBottom: Int,
                ) {
                    view.removeOnLayoutChangeListener(this)
                    this@ArticleView.jump { scrollBy(0, (bottom - top) - before) }
                }
            })
        }

        private fun picture(bitmap: Bitmap) {
            removeAllViews()
            val image = ImageView(context).apply {
                setImageBitmap(bitmap)
                adjustViewBounds = true
                scaleType = ImageView.ScaleType.FIT_CENTER
                contentDescription = block.alt.ifEmpty { null }
                // Читатель, нажавший картинку, хочет её саму (#38): сохранить
                // или отправить дальше. Браузер — последним пунктом.
                setOnClickListener { host.pictureMenu(block.source) }
            }
            // Ядро отдаёт картинку в её пикселях — пикселях монитора. На экране
            // телефона они втрое мельче, поэтому растим её тем же множителем,
            // что и кегль, но не шире колонки.
            val width = min((bitmap.width * metrics.unit).toInt(), columnWidth())
            val height = (bitmap.height.toLong() * width / max(1, bitmap.width)).toInt()
            addView(image, LayoutParams(width, height))
            if (block.alt.isNotEmpty()) {
                // Подпись стоит под картинкой, а не под колонкой: узкая картинка
                // висит по центру, и подпись у левого поля выглядела бы чужой.
                val narrow = width < columnWidth()
                addView(caption(block.alt).apply {
                    gravity = if (narrow) Gravity.CENTER_HORIZONTAL else Gravity.START
                })
            }
        }

        private fun caption(text: String) = TextView(context).apply {
            this.text = text
            setTextColor(palette.dim)
            typeface = fonts.pick(false, 400, false, metrics.face).typeface
            setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, metrics.text * 0.85f)
            setPadding(0, metrics.px(4f).toInt(), 0, metrics.px(6f).toInt())
            layoutParams = LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.WRAP_CONTENT)
        }
    }

    /**
     * Таблица — сетка на своей строке. Широкая листается вбок: на телефоне
     * это честнее, чем ужать столбцы до слога в строке.
     */
    @SuppressLint("ViewConstructor")
    inner class TableFrame(context: Context, val block: Block.Table) : HorizontalScrollView(context) {
        /** Ячейки по порядку чтения — для поиска (#10). */
        val cells = mutableListOf<Washed>()

        init {
            isHorizontalScrollBarEnabled = true
            val grid = TableLayout(context)
            val columns = block.rows.maxOfOrNull { it.cells.size } ?: 1
            val gap = metrics.px(20f).toInt()
            val rowGap = metrics.px(7f).toInt()
            for (row in block.rows) {
                val line = TableRow(context)
                row.cells.forEachIndexed { index, cell ->
                    val view = Washed(context).apply {
                        setTextColor(palette.ink)
                        setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, metrics.text)
                        typeface = fonts.pick(false, if (row.header) 500 else 400, false, metrics.face).typeface
                        includeFontPadding = false
                        setLineSpacing(metrics.extra, 1f)
                        // Шире тридцати знаков ячейка не растёт — дальше перенос,
                        // как у `max_width_chars` в окне.
                        maxWidth = (metrics.text * 16.5f).toInt()
                        gravity = when (block.align.getOrNull(index)) {
                            "right" -> Gravity.END
                            "center" -> Gravity.CENTER_HORIZONTAL
                            else -> Gravity.START
                        }
                        setPadding(0, 0, if (index + 1 < columns) gap else 0, rowGap)
                        text = cellText(cell, row.header)
                        movementMethod = LinkMovementMethod.getInstance()
                        highlightColor = palette.chosen
                    }
                    line.addView(view)
                    cells += view
                }
                grid.addView(line)
                if (row.header) {
                    grid.addView(View(context).apply { setBackgroundColor(palette.rule) },
                        TableLayout.LayoutParams(TableLayout.LayoutParams.MATCH_PARENT, max(1, metrics.px(1f).toInt())).apply {
                            bottomMargin = rowGap
                        })
                }
            }
            grid.setColumnStretchable(columns - 1, true)
            grid.setColumnShrinkable(columns - 1, false)
            grid.minimumWidth = columnWidth()
            addView(grid, LayoutParams(LayoutParams.WRAP_CONTENT, LayoutParams.WRAP_CONTENT))
        }

        /** Подсветить найденное в своих ячейках; `here` — на котором стоим. */
        fun mark(hits: List<Hit>, here: Int) {
            val type = metrics.type
            for (cell in cells) cell.clearMarks()
            hits.forEachIndexed { index, hit ->
                if (hit !is Hit.Cell || hit.table !== block) return@forEachIndexed
                val cell = cells.getOrNull(hit.cell) ?: return@forEachIndexed
                val end = min(hit.to, cell.text.length)
                if (hit.from < end) cell.mark(hit.from, end, if (index == here) type.foundHere else type.found, type.foundInk)
            }
        }

        private fun cellText(cell: Cell, header: Boolean): CharSequence {
            val out = SpannableStringBuilder(cell.text)
            for (run in cell.runs) {
                val weight = if ("strong" in run.styles) 700 else if (header) 500 else 400
                val italic = "em" in run.styles
                val mono = "code" in run.styles
                val face = fonts.pick(mono, weight, italic, metrics.face)
                val size = if (mono) metrics.text * metrics.type.codeSize else metrics.text
                out.setSpan(FaceSpan(face, size, 0f, 0f), run.start, run.end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
                if (mono) out.setSpan(Wash(palette.panel), run.start, run.end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
            for (link in cell.links) {
                out.setSpan(object : ClickableSpan() {
                    override fun onClick(widget: View) = host.follow(link.target, false)
                    override fun updateDrawState(paint: TextPaint) {
                        paint.color = palette.link
                        paint.isUnderlineText = true
                    }
                }, link.start, link.end, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
            }
            return out
        }
    }

    /**
     * Карточка заметки под абзацем (#42): слова и картинки читателя, черта
     * цвета заметки слева — чтобы не спутать с текстом автора. Нажатие —
     * то же меню, что у отмеченного; нажатие на картинку — она крупно.
     * `at` — конец абзаца: место чтения на карточке — там.
     */
    @SuppressLint("ViewConstructor")
    inner class NoteCard(context: Context, val at: Int, val note: NoteSpot) : LinearLayout(context) {
        init {
            orientation = HORIZONTAL
            val rule = View(context).apply { setBackgroundColor(palette.noteRule) }
            addView(rule, LayoutParams(max(2, metrics.px(3f).toInt()), LayoutParams.MATCH_PARENT))
            val body = LinearLayout(context).apply {
                orientation = VERTICAL
                val side = metrics.px(10f).toInt()
                setPadding(side, metrics.px(6f).toInt(), side, metrics.px(8f).toInt())
            }
            addView(body, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            background = GradientDrawable().apply {
                setColor(palette.shelf)
                val round = metrics.px(6f)
                cornerRadii = floatArrayOf(0f, 0f, round, round, round, round, 0f, 0f)
            }
            clipToOutline = true
            if (note.text.isNotEmpty()) {
                body.addView(TextView(context).apply {
                    text = note.text
                    setTextColor(palette.ink)
                    typeface = fonts.pick(false, 400, false, metrics.face).typeface
                    setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, metrics.text * NOTE_TYPE)
                    setLineSpacing(0f, 1.25f)
                })
            }
            val inner = columnWidth() - max(2, metrics.px(3f).toInt()) - 2 * metrics.px(10f).toInt()
            for (path in note.pictures) body.addView(picture(path, inner))
            contentDescription = "Your note: " + note.text.ifEmpty { "a picture" }
            var x = 0
            var y = 0
            setOnTouchListener { _, event ->
                if (event.action == MotionEvent.ACTION_DOWN) {
                    x = event.rawX.toInt()
                    y = event.rawY.toInt()
                }
                false
            }
            setOnClickListener { host.noteMenu(note.id, x, y) }
        }

        /** Картинка заметки: место — сразу, по ширине, пиксели — из фона. */
        private fun picture(path: String, inner: Int): View {
            val image = ImageView(context).apply {
                adjustViewBounds = true
                scaleType = ImageView.ScaleType.FIT_START
                contentDescription = "Your picture"
                setOnClickListener { host.notePicture(note.id, path) }
            }
            val holder = FrameLayout(context)
            holder.addView(image, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT))
            val look = Look(inner, look(false).paper, metrics.text, natural = false)
            fetch(path, look) { decoded ->
                val bitmap = decoded.bitmap ?: return@fetch
                // Как у картинки статьи: пиксели монитора — множителем кегля,
                // не шире карточки и не выше доли экрана.
                var w = min((bitmap.width * metrics.unit).toInt(), inner)
                var h = (bitmap.height.toLong() * w / max(1, bitmap.width)).toInt()
                val tall = (metrics.text * PICTURE_LINES).toInt()
                if (h > tall) {
                    w = max(1, (w.toLong() * tall / h).toInt())
                    h = tall
                }
                image.setImageBitmap(bitmap)
                image.layoutParams = FrameLayout.LayoutParams(w, h)
            }
            holder.setPadding(0, metrics.px(6f).toInt(), 0, 0)
            return holder
        }
    }

    /**
     * Кусок текста. Выделяется и копируется; ссылка открывается нажатием,
     * новой вкладкой — долгим нажатием через меню.
     */
    @SuppressLint("ViewConstructor")
    inner class ProseView(context: Context, val piece: Piece) : Washed(context) {
        private var downX = 0f
        private var downY = 0f
        private var rawX = 0f
        private var rawY = 0f
        private var downAt = 0L
        private var hadSelection = false

        fun setup(metrics: Metrics, palette: Palette, fonts: Fonts) {
            setTextIsSelectable(true)
            includeFontPadding = false
            setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, metrics.text)
            typeface = fonts.pick(false, 400, false, metrics.face).typeface
            setTextColor(palette.ink)
            highlightColor = palette.chosen
            setLineSpacing(0f, 1f)
            // Выключка (#6) — только кускам прозы: заголовки, код и списки
            // режутся в свои куски (`pieces`) и остаются с рваным краем.
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.O) {
                justificationMode = if (piece.justified) Layout.JUSTIFICATION_MODE_INTER_WORD else Layout.JUSTIFICATION_MODE_NONE
            }
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                breakStrategy = Layout.BREAK_STRATEGY_HIGH_QUALITY
                // Переносы ставит ядро мягкими дефисами — по языку страницы.
                // Свои Android брал бы по языку системы, а он у статьи другой:
                // поэтому язык текста — «никакой», и переносы только наши.
                // Выключке переносы нужнее: с обычной частотой Android
                // предпочитает растянуть строку дырами, чем перенести слово.
                hyphenationFrequency = if (piece.justified) Layout.HYPHENATION_FREQUENCY_FULL else Layout.HYPHENATION_FREQUENCY_NORMAL
            }
            textLocale = Locale.ROOT
            // Выделенное — в заметку (#39): два своих пункта в системном меню
            // выделения, рядом с «Копировать» и «Поделиться».
            customSelectionActionModeCallback = object : ActionMode.Callback {
                override fun onCreateActionMode(mode: ActionMode, menu: android.view.Menu): Boolean {
                    menu.add(android.view.Menu.NONE, MARK, 0, "Highlight")
                    menu.add(android.view.Menu.NONE, NOTE, 1, "Note")
                    host.selecting(true)
                    return true
                }

                override fun onPrepareActionMode(mode: ActionMode, menu: android.view.Menu) = false

                override fun onActionItemClicked(mode: ActionMode, item: MenuItem): Boolean {
                    val write = when (item.itemId) {
                        MARK -> false
                        NOTE -> true
                        else -> return false
                    }
                    val from = selectionStart
                    val to = selectionEnd
                    if (from < 0 || to <= from) return false
                    Selection.setSelection(text as Spannable, to)
                    mode.finish()
                    host.mark(piece.start + from, piece.start + to, write)
                    return true
                }

                override fun onDestroyActionMode(mode: ActionMode) = host.selecting(false)
            }
            setOnLongClickListener {
                val link = linkAt(downX, downY) ?: return@setOnLongClickListener false
                host.linkMenu(link.target, rawX.toInt(), rawY.toInt())
                true
            }
        }

        @SuppressLint("ClickableViewAccessibility")
        override fun onTouchEvent(event: MotionEvent): Boolean {
            when (event.actionMasked) {
                MotionEvent.ACTION_DOWN -> {
                    downX = event.x
                    downY = event.y
                    rawX = event.rawX
                    rawY = event.rawY
                    downAt = event.eventTime
                    hadSelection = hasSelection()
                }
                MotionEvent.ACTION_UP -> {
                    val slop = ViewConfiguration.get(context).scaledTouchSlop
                    val tap = abs(event.x - downX) < slop && abs(event.y - downY) < slop &&
                        event.eventTime - downAt < ViewConfiguration.getLongPressTimeout()
                    if (tap && !hadSelection) {
                        val link = linkAt(event.x, event.y)
                        if (link != null) {
                            post { host.follow(link.target, false) }
                        } else {
                            // Ссылка главнее: отмеченная ссылка открывается,
                            // а заметка к ней — с полки.
                            val note = offsetAt(event.x, event.y)?.let { noteAt(it) }
                            if (note != null) post { host.noteMenu(note.id, rawX.toInt(), rawY.toInt()) }
                        }
                    }
                }
            }
            return super.onTouchEvent(event)
        }

        /** Смещение страницы под точкой — только если палец на тексте строки. */
        private fun offsetAt(x: Float, y: Float): Int? {
            val layout = layout ?: return null
            val line = layout.getLineForVertical((y - totalPaddingTop + scrollY).toInt())
            val lx = x - totalPaddingLeft + scrollX
            if (lx < layout.getLineLeft(line) || lx > layout.getLineRight(line)) return null
            return piece.start + layout.getOffsetForHorizontal(line, lx)
        }

        /** Ссылка под точкой — только если палец и правда на тексте строки. */
        private fun linkAt(x: Float, y: Float): Link? {
            val layout = layout ?: return null
            val line = layout.getLineForVertical((y - totalPaddingTop + scrollY).toInt())
            val lx = x - totalPaddingLeft + scrollX
            if (lx < layout.getLineLeft(line) || lx > layout.getLineRight(line)) return null
            val offset = piece.start + layout.getOffsetForHorizontal(line, lx)
            val links = page?.links ?: return null
            return links.firstOrNull { offset >= it.start && offset < it.end }
                ?: links.firstOrNull { offset == it.end && it.end > it.start && lx < layout.getLineRight(line) &&
                    layout.getPrimaryHorizontal(it.end - piece.start) >= lx - 1 }
        }

        /** Копирование — чистым текстом: без мягких переносов и неразрывных пробелов. */
        override fun onTextContextMenuItem(id: Int): Boolean {
            if (id == android.R.id.copy || id == android.R.id.cut) {
                val from = selectionStart
                val to = selectionEnd
                if (from in 0 until to) {
                    val clean = plain(text.subSequence(from, to).toString())
                    val clipboard = context.getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
                    clipboard.setPrimaryClip(ClipData.newPlainText(null, clean))
                    Selection.setSelection(text as Spannable, to)
                    return true
                }
            }
            return super.onTextContextMenuItem(id)
        }

        private val noted = mutableListOf<Any>()

        fun clearNotes() {
            val text = text as? Spannable ?: return
            if (noted.isEmpty()) return
            noted.forEach { text.removeSpan(it) }
            noted.clear()
            invalidate()
        }

        /**
         * Заметка — подложкой под найденным поиском: приоритет спана выше,
         * значит `getSpans` отдаёт её первой, и рисуется она раньше, ниже.
         *
         * Перерисовку просим сами: `Wash` — не стиль текста, и `TextView`
         * на него не перерисовывается. У поиска это делает соседний
         * `ForegroundColorSpan`, а здесь без `invalidate` заметка, оставленная
         * через диалог, появлялась только при следующем показе страницы.
         */
        fun note(from: Int, to: Int, background: Int, rule: Int) {
            val text = text as? Spannable ?: return
            val wash = Wash(background, rule)
            text.setSpan(wash, from, to, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE or (1 shl Spanned.SPAN_PRIORITY_SHIFT))
            noted += wash
            invalidate()
        }

    }

    companion object {
        /** Сколько кадров ждём, пока картинки и таблицы займут своё место. */
        const val SETTLE_FRAMES = 45

        /** Сколько знаков кладём в колонку сразу: первый экран с запасом. */
        const val FIRST_CHARS = 2 * CHUNK_CHARS

        /** Кегль слов заметки под абзацем — доля кегля текста: это поля, а не текст. */
        const val NOTE_TYPE = 0.85f

        /** Выше скольких строк текста картинка заметки не встаёт: крупно — по нажатию. */
        const val PICTURE_LINES = 18f

        /** Пункты меню выделения (#39): отметить и приписать. */
        private const val MARK = 0x4e01
        private const val NOTE = 0x4e02

        /** Снять типографские знаки: в буфер обмена идёт чистый текст. */
        fun plain(text: String): String = buildString(text.length) {
            for (ch in text) when (ch) {
                '­', '​', '￼' -> {}
                ' ' -> append(' ')
                else -> append(ch)
            }
        }

        fun clip(text: String, max: Int): String =
            if (text.length <= max) text else text.take(max - 1).trimEnd() + "…"
    }
}

/**
 * Текст с подложками: код в строке и найденное поиском лежат на своём цвете
 * от выносного элемента до нижнего — только под буквами, а не на всю высоту
 * строки вместе с воздухом.
 */
open class Washed(context: Context) : TextView(context) {
    private val wash = Paint()
    private val face = android.text.TextPaint()
    private val probe = Paint.FontMetricsInt()
    private val found = mutableListOf<Any>()

    /** Снять подсветку поиска. Здесь, а не в куске текста: ячейки таблиц — тоже `Washed` (#10). */
    fun clearMarks() {
        val text = text as? Spannable ?: return
        found.forEach { text.removeSpan(it) }
        found.clear()
    }

    fun mark(from: Int, to: Int, background: Int, ink: Int) {
        val text = text as? Spannable ?: return
        val back = Wash(background)
        val fore = ForegroundColorSpan(ink)
        text.setSpan(back, from, to, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        text.setSpan(fore, from, to, Spanned.SPAN_EXCLUSIVE_EXCLUSIVE)
        found += back
        found += fore
    }

    override fun onDraw(canvas: Canvas) {
        val layout = layout
        val text = text as? Spanned
        if (layout != null && text != null) {
            val washes = text.getSpans(0, text.length, Wash::class.java)
            if (washes.isNotEmpty()) {
                canvas.save()
                canvas.translate(totalPaddingLeft.toFloat(), totalPaddingTop.toFloat())
                for (span in washes) draw(canvas, layout, text, span)
                canvas.restore()
            }
        }
        super.onDraw(canvas)
    }

    private fun draw(canvas: Canvas, layout: Layout, text: Spanned, span: Wash) {
        val from = text.getSpanStart(span)
        val to = text.getSpanEnd(span)
        if (from < 0 || from >= to) return
        // Высота подложки — по гарнитуре и кеглю того участка, на котором она лежит.
        face.set(paint)
        text.getSpans(from, from + 1, FaceSpan::class.java).lastOrNull()?.updateMeasureState(face)
        face.getFontMetricsInt(probe)
        wash.color = span.color
        val pad = face.textSize * 0.08f
        var line = layout.getLineForOffset(from)
        val last = layout.getLineForOffset(to)
        while (line <= last) {
            val lineStart = layout.getLineStart(line)
            val lineEnd = layout.getLineEnd(line)
            val a = maxOf(from, lineStart)
            val b = minOf(to, lineEnd)
            if (a < b) {
                val x1 = layout.getPrimaryHorizontal(a)
                val x2 = if (b >= lineEnd && to > b) layout.getLineRight(line)
                else if (b == lineEnd) layout.getLineRight(line)
                else layout.getPrimaryHorizontal(b)
                val baseline = layout.getLineBaseline(line).toFloat()
                canvas.drawRect(minOf(x1, x2) - pad, baseline + probe.ascent, maxOf(x1, x2) + pad, baseline + probe.descent, wash)
                if (span.rule != 0) {
                    // Черта — по нижнему краю подложки, в толщину волоса.
                    wash.color = span.rule
                    val thick = maxOf(1f, face.textSize * 0.07f)
                    val y = baseline + probe.descent
                    canvas.drawRect(minOf(x1, x2) - pad, y - thick, maxOf(x1, x2) + pad, y, wash)
                    wash.color = span.color
                }
            }
            line += 1
        }
    }
}
