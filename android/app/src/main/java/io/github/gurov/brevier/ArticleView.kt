package io.github.gurov.brevier

import android.annotation.SuppressLint
import android.content.ClipData
import android.content.ClipboardManager
import android.content.Context
import android.graphics.Bitmap
import android.graphics.Canvas
import android.graphics.Paint
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
import android.view.Choreographer
import android.view.Gravity
import android.view.MotionEvent
import android.view.ScaleGestureDetector
import android.view.View
import android.view.ViewConfiguration
import android.view.ViewGroup
import android.widget.Button
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

    /** Отдать адрес системному браузеру: картинку в полном размере, страницу. */
    fun openOutside(target: String)

    /** Страницу прокрутили: полка отмечает, где читатель. */
    fun scrolled()

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
    private var dark = false

    private val proses = mutableListOf<ProseView>()
    private val images = mutableListOf<ImageFrame>()
    private val formulas = mutableListOf<Pair<FormulaSpan, ProseView>>()

    /** Номер показа: ответ картинки для прошлой страницы уже никому не нужен. */
    private var generation = 0

    init {
        isFillViewport = true
        isVerticalScrollBarEnabled = true
        // Фокус держит сама статья: иначе, когда кусок текста с фокусом
        // уходит вместе со старой страницей, Android отдаёт фокус первому
        // полю ввода — адресной строке, и та начинает принимать «назад».
        isFocusableInTouchMode = true
        descendantFocusability = FOCUS_BEFORE_DESCENDANTS
        addView(column, LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT))
    }

    /**
     * Показать страницу. `offer` — кнопка «Open in your browser» под текстом
     * отказа: предлагать её решает ядро.
     */
    fun show(page: Page, metrics: Metrics, dark: Boolean, eager: Boolean, offer: String? = null) {
        generation += 1
        this.page = page
        this.metrics = metrics
        this.dark = dark
        palette = metrics.type.palette(dark)
        setBackgroundColor(palette.paper)
        if (hasFocus()) requestFocus()
        column.removeAllViews()
        proses.clear()
        images.clear()
        formulas.clear()
        fitColumn(width)

        for (item in pieces(page)) {
            when (item) {
                is Piece -> addProse(page, item)
                is Block.Image -> addImage(item)
                is Block.Table -> addTable(item)
            }
        }
        if (offer != null) addOffer(offer)
        // Пустое место внизу: последнюю строку можно поднять с края экрана.
        column.addView(View(context), LinearLayout.LayoutParams(1, (metrics.text * 4).toInt()))

        scrollTo(0, 0)
        if (eager) loadAll()
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
        column.addView(table, LinearLayout.LayoutParams(ViewGroup.LayoutParams.MATCH_PARENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
            topMargin = metrics.px(10f).toInt()
            bottomMargin = metrics.px(14f).toInt() + metrics.extra.toInt()
        })
    }

    /**
     * Кнопка, а не только пункт меню: на странице, где ничего не показалось,
     * читателю нужен выход, а не память о меню.
     */
    private fun addOffer(target: String) {
        val button = Button(context).apply {
            text = "Open in your browser"
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
            setOnClickListener { host.openOutside(target) }
        }
        column.addView(button, LinearLayout.LayoutParams(ViewGroup.LayoutParams.WRAP_CONTENT, ViewGroup.LayoutParams.WRAP_CONTENT).apply {
            topMargin = (metrics.extra * 2).toInt()
            gravity = Gravity.START
        })
    }

    /** Загрузить все картинки: переключатель в настройках включён. */
    fun loadAll() {
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
                else -> null
            }
        }
        return null
    }

    /** Где читатель: смещение строки у верхнего края. От масштаба не зависит. */
    fun place(): Int = offsetAt(scrollY) ?: 0

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
        val y = yOf(offset) ?: return
        scrollTo(0, max(0, y - (height * align).toInt()))
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
        host.scrolled()
    }

    // ── поиск ───────────────────────────────────────────────────────────────

    /** Подсветить совпадения; `here` — то, на котором стоим. Смещения — пары подряд. */
    fun highlight(hits: IntArray, here: Int) {
        for (view in proses) view.clearMarks()
        val type = metrics.type
        var index = 0
        while (index + 1 < hits.size) {
            val from = hits[index]
            val to = hits[index + 1]
            val current = index / 2 == here
            for (view in proses) {
                val start = view.piece.start
                val end = start + view.text.length
                val a = max(from, start)
                val b = min(to, end)
                if (a < b) view.mark(a - start, b - start, if (current) type.foundHere else type.found, type.foundInk)
            }
            index += 2
        }
        if (here >= 0 && here * 2 < hits.size) scrollToOffset(hits[here * 2], 0.3f)
    }

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
        if (scaler.isInProgress) return true
        return super.dispatchTouchEvent(event)
    }

    // ── картинки ────────────────────────────────────────────────────────────

    private fun look(natural: Boolean) = Look(
        width = columnWidth(),
        // Прозрачное кладём на светлую бумагу, а не на белое: белая карточка
        // посреди слоновой кости заметна, а схеме нужен только светлый фон.
        paper = metrics.type.light.paper and 0xffffff,
        fontSize = metrics.text,
        natural = natural,
    )

    private class Look(val width: Int, val paper: Int, val fontSize: Float, val natural: Boolean)

    private class Decoded(val bitmap: Bitmap?, val trouble: String?)

    private fun fetch(source: String, look: Look, done: (Decoded) -> Unit) {
        val shown = generation
        App.work.execute {
            val bytes = try {
                Core.image(source, look.width, look.paper, look.fontSize, look.natural)
            } catch (failure: Throwable) {
                byteArrayOf(1) + "The load fell through".toByteArray()
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
                if (bitmap == null) {
                    idle(decoded.trouble ?: "The load fell through")
                    return@fetch
                }
                // Картинка выше окна вырастет из заглушки в полный рост и толкнёт
                // текст под глазами — держим место чтения.
                val above = bottom <= scrollY
                val place = if (above) place() else null
                picture(bitmap)
                place?.let { settle(it, 0f) }
            }
        }

        private fun picture(bitmap: Bitmap) {
            removeAllViews()
            val image = ImageView(context).apply {
                setImageBitmap(bitmap)
                adjustViewBounds = true
                scaleType = ImageView.ScaleType.FIT_CENTER
                contentDescription = block.alt.ifEmpty { null }
                // Полный размер — работа системного браузера: здесь картинка
                // ужата до меры текста.
                setOnClickListener { host.openOutside(block.source) }
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
            typeface = fonts.regular
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
                        typeface = if (row.header) fonts.medium else fonts.regular
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

        private fun cellText(cell: Cell, header: Boolean): CharSequence {
            val out = SpannableStringBuilder(cell.text)
            for (run in cell.runs) {
                val weight = if ("strong" in run.styles) 700 else if (header) 500 else 400
                val italic = "em" in run.styles
                val mono = "code" in run.styles
                val face = fonts.pick(mono, weight, italic)
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
        private val found = mutableListOf<Any>()

        fun setup(metrics: Metrics, palette: Palette, fonts: Fonts) {
            setTextIsSelectable(true)
            includeFontPadding = false
            setTextSize(android.util.TypedValue.COMPLEX_UNIT_PX, metrics.text)
            typeface = fonts.regular
            setTextColor(palette.ink)
            highlightColor = palette.chosen
            setLineSpacing(0f, 1f)
            if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.M) {
                breakStrategy = Layout.BREAK_STRATEGY_HIGH_QUALITY
                // Переносы ставит ядро мягкими дефисами — по языку страницы.
                // Свои Android брал бы по языку системы, а он у статьи другой:
                // поэтому язык текста — «никакой», и переносы только наши.
                hyphenationFrequency = Layout.HYPHENATION_FREQUENCY_NORMAL
            }
            textLocale = Locale.ROOT
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
                        linkAt(event.x, event.y)?.let { link ->
                            post { host.follow(link.target, false) }
                        }
                    }
                }
            }
            return super.onTouchEvent(event)
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
    }

    companion object {
        /** Сколько кадров ждём, пока картинки и таблицы займут своё место. */
        const val SETTLE_FRAMES = 45

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
            }
            line += 1
        }
    }
}
