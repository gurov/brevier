package io.github.gurov.brevier

import android.graphics.Canvas
import android.graphics.Paint
import android.graphics.RectF
import android.text.Layout
import android.text.SpannableStringBuilder
import android.text.Spanned
import android.text.TextPaint
import android.text.style.ForegroundColorSpan
import android.text.style.LeadingMarginSpan
import android.text.style.LineBackgroundSpan
import android.text.style.LineHeightSpan
import android.text.style.MetricAffectingSpan
import android.text.style.UnderlineSpan

/**
 * Имена стилей ядра — в спаны Android.
 *
 * Приоритет стилей тот же, что у тегов буфера в окне: у GTK побеждает тег,
 * заведённый позже, и порядок заведения — это порядок в `tags()`. Держим
 * его здесь списком: подпись оповещения перебивает курсив цитаты, ссылка —
 * цвет подсветки кода, ровно как на десктопе.
 */
private val PRIORITY = listOf(
    "body", "h1", "h2", "h3", "h4", "h5", "h6", "em", "strong", "code", "codeblock", "pad",
    "kw", "lit", "num", "com", "quote1", "quote2", "quote3", "list1", "list2", "list3",
    "link", "dim", "alert", "noteref", "note",
)

private fun ordered(styles: List<String>): List<String> =
    styles.sortedBy { PRIORITY.indexOf(it).let { at -> if (at < 0) Int.MAX_VALUE else at } }

/** Чем набран знак: гарнитура, кегль, вес, наклон, цвет. */
private class Look(
    val mono: Boolean,
    val size: Float,
    val weight: Int,
    val italic: Boolean,
    val color: Int?,
    val background: Int?,
    val underline: Boolean,
    val rise: Float,
    val tracking: Float,
)

private fun look(styles: List<String>, m: Metrics, palette: Palette): Look {
    val type = m.type
    var mono = false
    var size = 1f
    var weight = 400
    var italic = false
    var color: Int? = null
    var background: Int? = null
    var underline = false
    var rise = 0f
    var tracking = 0f
    for (style in ordered(styles)) {
        when (style) {
            "h1", "h2", "h3", "h4", "h5", "h6" -> {
                val level = style[1] - '1'
                size = type.headings[level]
                weight = type.headingWeights[level]
            }
            "em" -> italic = true
            "strong" -> weight = 700
            "code" -> {
                mono = true
                size = type.codeSize
                background = palette.panel
            }
            "codeblock" -> {
                mono = true
                size = type.codeSize
            }
            "pad" -> size = type.padSize
            "kw" -> color = palette.keyword
            "lit" -> color = palette.literal
            "num" -> color = palette.number
            "com" -> {
                color = palette.comment
                italic = true
            }
            "quote1", "quote2", "quote3" -> italic = true
            "link" -> {
                underline = true
                color = palette.link
            }
            "dim" -> color = palette.dim
            "alert" -> {
                weight = 700
                italic = false
                size = type.alertSize
                tracking = type.alertTracking
            }
            "noteref" -> {
                size = type.noterefSize
                rise = type.noterefRise
            }
            "note" -> size = type.noteSize
        }
    }
    return Look(mono, size, weight, italic, color, background, underline, m.px(rise), m.px(tracking))
}

/** Абзац: поля, висячий отступ, воздух, подложка кода, линейки цитат. */
private class Shape(
    val margin: Float,
    val hang: Float,
    val above: Float,
    val below: Float,
    val inside: Float,
    val panel: Boolean,
    val quotes: Int,
)

private fun shape(styles: List<String>, m: Metrics): Shape {
    val type = m.type
    var margin = 0f
    var hang = 0f
    var above = 0f
    var below = 0f
    var inside = 0f
    var panel = false
    var quotes = 0
    for (style in ordered(styles)) {
        when (style) {
            "body" -> {
                inside = m.extra
                below = m.extra * 2
            }
            "h1", "h2", "h3", "h4", "h5", "h6" -> {
                // Воздух сверху, а не снизу: заголовок принадлежит тому, что под ним.
                above = m.extra * 3
                below = m.extra
            }
            "codeblock" -> {
                margin = 0f
                hang = m.px(type.hang)
                below = m.px(type.codeGap)
                panel = true
            }
            "pad" -> {
                panel = true
                above = m.extra
                below = m.extra
            }
            "quote1", "quote2", "quote3" -> {
                val level = style.last() - '0'
                margin = m.px(type.indent) * level
                quotes = maxOf(quotes, level)
            }
            "list1", "list2", "list3" -> {
                val level = style.last() - '0'
                margin = m.px(type.indent) * level
                hang = m.px(type.hang)
                // Пункты стоят плотнее абзацев: список — одна мысль, разбитая на части.
                below = m.extra / 2
            }
            "note" -> {
                margin = m.px(type.noteIndent)
                hang = m.px(type.noteHang)
                below = m.extra / 2
            }
        }
    }
    return Shape(margin, hang, above, below, inside, panel, quotes)
}

/**
 * Подложка под текстом: код в строке, найденное поиском. Не `BackgroundColorSpan`:
 * тот красит строку на всю высоту, вместе с воздухом между строками, а подложка
 * должна лежать только под буквами — как фон тега у GTK. Рисует её сам виджет
 * текста (`Washed`), под буквами.
 */
class Wash(val color: Int)

/** Гарнитура, кегль, наклон, разрядка, подъём — одним спаном на участок. */
class FaceSpan(
    private val face: Fonts.Face,
    private val size: Float,
    private val tracking: Float,
    private val rise: Float,
) : MetricAffectingSpan() {
    override fun updateMeasureState(paint: TextPaint) = apply(paint)
    override fun updateDrawState(paint: TextPaint) = apply(paint)

    private fun apply(paint: TextPaint) {
        paint.typeface = face.typeface
        paint.textSize = size
        paint.textSkewX = if (face.skew) -0.2f else 0f
        paint.isFakeBoldText = face.fakeBold
        if (tracking != 0f) paint.letterSpacing = tracking / size
        if (rise != 0f) paint.baselineShift -= rise.toInt()
    }
}

/**
 * Всё, что у абзаца, одним спаном: поля и висячий отступ, воздух над, внутри
 * и под абзацем, подложка панели кода и линейки цитат.
 *
 * Воздух — не межстрочный интервал `TextView`: тот один на весь текст,
 * а у абзаца, заголовка и пункта списка он разный, как у тегов окна.
 */
class ShapeSpan(
    private val margin: Int,
    private val hang: Int,
    private val above: Int,
    private val below: Int,
    private val inside: Int,
    private val panel: Int?,
    private val quotes: Int,
    private val rule: Int,
    private val ruleX: Float,
    private val ruleStep: Float,
    private val ruleWidth: Float,
    private val ruleInset: Float,
) : LeadingMarginSpan, LineHeightSpan.WithDensity, LineBackgroundSpan {

    // Маркер пункта стоит у поля, перенос — под текстом, а не под маркером.
    override fun getLeadingMargin(first: Boolean): Int = if (first) margin else margin + hang

    override fun drawLeadingMargin(
        c: Canvas, p: Paint, x: Int, dir: Int, top: Int, baseline: Int, bottom: Int,
        text: CharSequence, start: Int, end: Int, first: Boolean, layout: Layout?,
    ) {
        if (quotes == 0 || text !is Spanned || text.getSpanStart(this) > start) return
        val spanEnd = text.getSpanEnd(this)
        val last = end >= spanEnd
        val was = p.color
        val style = p.style
        p.color = rule
        p.style = Paint.Style.FILL
        val from = top + if (first) ruleInset else 0f
        val to = bottom - if (last) ruleInset else 0f
        for (level in 0 until quotes) {
            val left = x + dir * (ruleX + ruleStep * level)
            c.drawRect(RectF(left, from, left + ruleWidth, maxOf(to, from + 1f)), p)
        }
        p.color = was
        p.style = style
    }

    /*
     * Высоту строки считаем сами, по спанам в ней. Метрикам, которые приносит
     * Android, верить нельзя: внутри абзаца он отдаёт следующей строке то, что
     * этот же спан вернул для прошлой, — уже с прибавкой, — и воздух рос от
     * строки к строке.
     */
    private val work = TextPaint()
    private val probe = Paint.FontMetricsInt()

    override fun chooseHeight(
        text: CharSequence, start: Int, end: Int, spanstartv: Int, lineHeight: Int, fm: Paint.FontMetricsInt,
        paint: TextPaint,
    ) {
        if (text !is Spanned) return
        natural(text, start, end, paint, fm)
        adjust(text, start, end, fm)
    }

    override fun chooseHeight(
        text: CharSequence, start: Int, end: Int, spanstartv: Int, lineHeight: Int, fm: Paint.FontMetricsInt,
    ) {
        if (text is Spanned) adjust(text, start, end, fm)
    }

    /** Естественные метрики строки: самое высокое и самое низкое из её участков. */
    private fun natural(text: Spanned, start: Int, end: Int, paint: TextPaint, fm: Paint.FontMetricsInt) {
        var ascent = 0
        var descent = 0
        var top = 0
        var bottom = 0
        var seen = false
        fun take(shift: Int) {
            ascent = minOf(ascent, probe.ascent + shift)
            top = minOf(top, probe.top + shift)
            descent = maxOf(descent, probe.descent + shift)
            bottom = maxOf(bottom, probe.bottom + shift)
            seen = true
        }
        for (face in text.getSpans(start, end, FaceSpan::class.java)) {
            if (text.getSpanEnd(face) <= start || text.getSpanStart(face) >= end) continue
            work.set(paint)
            face.updateMeasureState(work)
            work.getFontMetricsInt(probe)
            take(work.baselineShift)
        }
        for (thing in text.getSpans(start, end, android.text.style.ReplacementSpan::class.java)) {
            val from = text.getSpanStart(thing)
            val to = text.getSpanEnd(thing)
            if (to <= start || from >= end) continue
            work.set(paint)
            work.getFontMetricsInt(probe)
            thing.getSize(work, text, from, to, probe)
            take(0)
        }
        if (!seen) {
            paint.getFontMetricsInt(probe)
            take(0)
        }
        fm.ascent = ascent
        fm.descent = descent
        fm.top = top
        fm.bottom = bottom
    }

    /** Воздух над первой строкой абзаца, под последней и между строками. */
    private fun adjust(text: Spanned, start: Int, end: Int, fm: Paint.FontMetricsInt) {
        if (start == text.getSpanStart(this)) {
            fm.ascent -= above
            fm.top -= above
        }
        val add = if (end >= text.getSpanEnd(this)) below else inside
        fm.descent += add
        fm.bottom += add
    }

    override fun drawBackground(
        canvas: Canvas, paint: Paint, left: Int, right: Int, top: Int, baseline: Int, bottom: Int,
        text: CharSequence, start: Int, end: Int, lineNumber: Int,
    ) {
        val color = panel ?: return
        val was = paint.color
        paint.color = color
        canvas.drawRect(left.toFloat(), top.toFloat(), right.toFloat(), bottom.toFloat(), paint)
        paint.color = was
    }
}

/** Кусок статьи между объектами: свой `TextView`. Смещения — в тексте страницы. */
class Piece(val start: Int, val end: Int)

/**
 * Разрезать страницу по объектам-блокам. Иллюстрация и таблица — не текст,
 * им нужен свой виджет, а виджет в середине `TextView` не поставить; формула
 * в строке остаётся в тексте спаном.
 *
 * Перевод строки перед объектом и после него уходит: в окне на них висят
 * поля абзаца, а здесь отступы у самого объекта.
 */
fun pieces(page: Page): List<Any> {
    val text = page.text
    val out = mutableListOf<Any>()
    var cursor = 0
    val blocks = page.blocks.filter { it !is Block.Image || !it.inline }.sortedBy { it.at }
    for (block in blocks) {
        var end = block.at
        if (end > cursor && text[end - 1] == '\n') end -= 1
        if (end > cursor) out += Piece(cursor, end)
        out += block
        cursor = block.at + 1
        if (cursor < text.length && text[cursor] == '\n') cursor += 1
    }
    if (cursor < text.length) out += Piece(cursor, text.length)
    return out
}

/**
 * Набрать кусок: текст со спанами. Последний перевод строки убираем —
 * `TextView` нарисовал бы после него пустую строку. Если последний абзац
 * сам пустой (поле панели кода, черта), перевод строки заменяем пробелом
 * нулевой ширины: абзац остаётся, со своими кеглем и подложкой.
 */
fun typeset(
    page: Page,
    piece: Piece,
    m: Metrics,
    palette: Palette,
    fonts: Fonts,
    inline: (Block.Image) -> Any?,
): SpannableStringBuilder {
    val text = page.text
    var end = piece.end
    val raw = StringBuilder(text.substring(piece.start, end))
    if (raw.isNotEmpty() && raw.last() == '\n') {
        if (raw.length >= 2 && raw[raw.length - 2] != '\n') {
            raw.setLength(raw.length - 1)
            end -= 1
        } else {
            raw.setCharAt(raw.length - 1, '​')
        }
    }
    val out = SpannableStringBuilder(raw)
    val base = piece.start
    val flags = Spanned.SPAN_EXCLUSIVE_EXCLUSIVE

    for (run in page.runs) {
        val from = maxOf(run.start, base)
        val to = minOf(run.end, end)
        if (from >= to) continue
        val look = look(run.styles, m, palette)
        val face = fonts.pick(look.mono, look.weight, look.italic)
        val a = from - base
        val b = to - base
        out.setSpan(FaceSpan(face, m.text * look.size, look.tracking, look.rise), a, b, flags)
        look.color?.let { out.setSpan(ForegroundColorSpan(it), a, b, flags) }
        look.background?.let { out.setSpan(Wash(it), a, b, flags) }
        if (look.underline) out.setSpan(UnderlineSpan(), a, b, flags)
    }

    for (block in page.blocks) {
        if (block !is Block.Image || !block.inline) continue
        if (block.at < base || block.at >= end) continue
        inline(block)?.let { out.setSpan(it, block.at - base, block.at - base + 1, flags) }
    }

    // Абзацы: от начала до перевода строки включительно.
    var start = 0
    while (start < out.length) {
        var stop = out.indexOf('\n', start)
        stop = if (stop < 0) out.length else stop + 1
        val styles = stylesAt(page, base + start, base + stop)
        val shape = shape(styles, m)
        out.setSpan(
            ShapeSpan(
                margin = shape.margin.toInt(),
                hang = shape.hang.toInt(),
                above = shape.above.toInt(),
                below = shape.below.toInt(),
                inside = shape.inside.toInt(),
                panel = if (shape.panel) palette.panel else null,
                quotes = shape.quotes,
                rule = palette.quoteRule,
                ruleX = m.px(m.type.ruleX),
                ruleStep = m.px(m.type.indent),
                ruleWidth = maxOf(1f, m.px(m.type.ruleWidth)),
                ruleInset = m.px(m.type.ruleInset),
            ),
            start, stop, Spanned.SPAN_PARAGRAPH,
        )
        start = stop
    }
    return out
}

private fun CharSequence.indexOf(ch: Char, from: Int): Int {
    for (i in from until length) if (this[i] == ch) return i
    return -1
}

/**
 * Стили абзаца — по первому знаку, у которого они есть, как у GTK: свойства
 * строки там берутся по тегам в её начале. Знак объекта стилей не несёт,
 * поэтому ищем первый участок со стилями.
 */
private fun stylesAt(page: Page, from: Int, to: Int): List<String> {
    for (run in page.runs) {
        if (run.end <= from) continue
        if (run.start >= to) break
        if (run.styles.isNotEmpty()) return run.styles
    }
    return emptyList()
}
