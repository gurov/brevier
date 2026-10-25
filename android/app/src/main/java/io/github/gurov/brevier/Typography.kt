package io.github.gurov.brevier

import android.content.Context
import android.graphics.Color
import android.graphics.Typeface
import android.util.TypedValue
import org.json.JSONObject

/**
 * Типографская модель ядра (`outline.rs`, `palette.rs`). Kotlin чисел
 * не повторяет: кегли, мера, отступы и цвета приходят из ядра одним ответом.
 */
class Typography(json: JSONObject) {
    val textSize = json.getDouble("textSize").toFloat()
    /** Кегль десктопа в его пикселях: от него считаются все отступы модели. */
    val textPx = json.getDouble("textPx").toFloat()
    val lineHeight = json.getDouble("lineHeight").toFloat()
    val headings = floats(json, "headings")
    val headingWeights = json.getJSONArray("headingWeights").map { (it as Number).toInt() }
    val measureInEms = json.getDouble("measureInEms").toFloat()
    val zoomSteps = floats(json, "zoomSteps")
    val zoomNormal = json.getInt("zoomNormal")
    val indent = json.getDouble("indent").toFloat()
    val hang = json.getDouble("hang").toFloat()
    /** Висячий отступ пункта с номером в 1, 2, 3 цифры (#13). */
    val numberHangs = floats(json, "numberHangs")
    val noteIndent = json.getDouble("noteIndent").toFloat()
    val noteHang = json.getDouble("noteHang").toFloat()
    val codeGap = json.getDouble("codeGap").toFloat()
    val codeSize = json.getDouble("codeSize").toFloat()
    val padSize = json.getDouble("padSize").toFloat()
    val noteSize = json.getDouble("noteSize").toFloat()
    val noterefSize = json.getDouble("noterefSize").toFloat()
    val noterefRise = json.getDouble("noterefRise").toFloat()
    val alertSize = json.getDouble("alertSize").toFloat()
    val alertTracking = json.getDouble("alertTracking").toFloat()
    val ruleWidth = json.getDouble("ruleWidth").toFloat()
    val ruleX = json.getDouble("ruleX").toFloat()
    val ruleInset = json.getDouble("ruleInset").toFloat()
    val quoteLevels = json.getInt("quoteLevels")
    val found = color(json.getString("found"))
    val foundHere = color(json.getString("foundHere"))
    val foundInk = color(json.getString("foundInk"))
    val light = Palette(json.getJSONObject("light"))
    val dark = Palette(json.getJSONObject("dark"))
    /** Высокий контраст (#44): белая бумага и чёрная краска, в тёмной — наоборот. */
    val lightHigh = Palette(json.getJSONObject("lightHigh"))
    val darkHigh = Palette(json.getJSONObject("darkHigh"))

    fun palette(theme: Theme) = when {
        theme.dark && theme.high -> darkHigh
        theme.dark -> dark
        theme.high -> lightHigh
        else -> light
    }

    /** Висячий отступ по ширине маркера: 0 — «•», 1..3 — цифр в номере (`outline::hang`). */
    fun hang(width: Int) = if (width == 0) hang else numberHangs[minOf(width, numberHangs.size) - 1]

    companion object {
        private fun floats(json: JSONObject, key: String) =
            json.getJSONArray(key).map { (it as Number).toFloat() }

        fun color(hex: String): Int = Color.parseColor(hex)

        /** Одна на процесс: модель неизменна, спрашивать ядро дважды незачем. */
        val shared: Typography by lazy { Typography(Core.typography()) }
    }
}

/** Чем печатать: тема и контраст — как `palette::Theme` в ядре. */
data class Theme(val dark: Boolean, val high: Boolean)

/** Цвета темы. */
class Palette(json: JSONObject) {
    val paper = Typography.color(json.getString("paper"))
    val ink = Typography.color(json.getString("ink"))
    val shelf = Typography.color(json.getString("shelf"))
    val link = Typography.color(json.getString("link"))
    val dim = Typography.color(json.getString("dim"))
    val panel = Typography.color(json.getString("panel"))
    val keyword = Typography.color(json.getString("keyword"))
    val literal = Typography.color(json.getString("literal"))
    val number = Typography.color(json.getString("number"))
    val comment = Typography.color(json.getString("comment"))
    val rule = Typography.color(json.getString("rule"))
    val chosen = Typography.color(json.getString("chosen"))
    val touched = Typography.color(json.getString("touched"))
    /** Заметка (#39): подложка отмеченного и черта под приписанным. */
    val note = Typography.color(json.getString("note"))
    val noteRule = Typography.color(json.getString("noteRule"))

    /** Линейка цитаты: приглушённая краска вполсилы — как в окне. */
    val quoteRule: Int get() = Color.argb(128, Color.red(dim), Color.green(dim), Color.blue(dim))
}

/**
 * Набор читателя (#4): гарнитура, кегль, мера в кеглях, интерлиньяж и выключка.
 * Числа и их подписи — из ядра (`outline::Type`), здесь только носим.
 */
data class ReaderType(
    val face: String,
    val size: Float,
    val measure: Float,
    val leading: Float,
    val justify: Boolean,
) {
    companion object {
        fun of(json: JSONObject) = ReaderType(
            face = json.optString("face", "noto"),
            size = json.getDouble("size").toFloat(),
            measure = json.getDouble("measure").toFloat(),
            leading = json.getDouble("leading").toFloat(),
            justify = json.optBoolean("justify"),
        )
    }
}

/**
 * Модель в пикселях этого экрана на этой ступени масштаба.
 *
 * Кегль берём в `sp`: 16.5 sp на телефоне читаются так же, как 16.5 пункта
 * на мониторе в полутора метрах дальше, — и системная настройка размера
 * шрифта при этом уважается. Всё остальное, что в модели задано пикселями
 * десктопа, умножается на отношение нашего кегля к десктопному: пропорции
 * страницы одни на обеих платформах.
 */
class Metrics(context: Context, val type: Typography, val zoom: Float, val reader: ReaderType) {
    /** Кегль читателя на ступени масштаба: он множит всю модель, как и ступень. */
    val text: Float = TypedValue.applyDimension(
        TypedValue.COMPLEX_UNIT_SP, reader.size * zoom, context.resources.displayMetrics,
    )
    /** Сколько наших пикселей в пикселе десктопа. */
    val unit: Float = text / type.textPx
    /** Воздух между строками и абзацами — та же величина, что `extra` в окне. */
    val extra: Float = (reader.leading - 1f) * type.textSize * unit
    /** Мера: колонка не шире этого. На телефоне она обычно — весь экран. */
    val measure: Float = reader.measure * text
    /** Гарнитура текста: `noto`, `serif` или `sans`. */
    val face: String get() = reader.face
    /** Выключка по ширине (#6). */
    val justify: Boolean get() = reader.justify

    fun px(desktop: Float): Float = desktop * unit
}

/**
 * Гарнитуры из комплекта: те же файлы Noto, что вшиты в десктоп. Шрифт
 * выбирает не система — иначе обещание «типографику задаёт читатель, а не
 * платформа» не выполнялось бы и здесь.
 */
class Fonts(context: Context) {
    private val assets = context.assets
    private fun load(name: String): Typeface = Typeface.createFromAsset(assets, name)

    val light = load("NotoSans-Light.ttf")
    val regular = load("NotoSans-Regular.ttf")
    val italic = load("NotoSans-Italic.ttf")
    val medium = load("NotoSans-Medium.ttf")
    val bold = load("NotoSans-Bold.ttf")
    val boldItalic = load("NotoSans-BoldItalic.ttf")
    val mono = load("NotoSansMono-Regular.ttf")

    /**
     * Начертание под вес и наклон. Курсив есть только у 400 и 700; лёгкому
     * и среднему курсиву достаётся прямое начертание с наклоном — ровно
     * то, что делает Pango, когда нужного файла в комплекте нет.
     */
    fun pick(mono: Boolean, weight: Int, italic: Boolean, face: String = "noto"): Face {
        if (mono) return Face(this.mono, skew = italic, fakeBold = weight >= 600)
        if (face != "noto") return system(face, weight, italic)
        return when {
            weight >= 600 -> if (italic) Face(boldItalic) else Face(bold)
            weight >= 500 -> Face(medium, skew = italic)
            weight >= 400 -> if (italic) Face(this.italic) else Face(regular)
            else -> Face(light, skew = italic)
        }
    }

    /**
     * Гарнитура системы (#4): антиква или гротеск, которые Android называет
     * `serif` и `sans-serif`. С Android 9 вес любой; раньше — только обычный
     * и жирный, а лёгкий заголовок встаёт обычным.
     */
    private fun system(face: String, weight: Int, italic: Boolean): Face {
        val family = if (face == "serif") Typeface.SERIF else Typeface.SANS_SERIF
        val key = Triple(family, weight, italic)
        val typeface = systemFaces.getOrPut(key) {
            if (android.os.Build.VERSION.SDK_INT >= 28) {
                Typeface.create(family, weight, italic)
            } else {
                val style = when {
                    weight >= 600 && italic -> Typeface.BOLD_ITALIC
                    weight >= 600 -> Typeface.BOLD
                    italic -> Typeface.ITALIC
                    else -> Typeface.NORMAL
                }
                Typeface.create(family, style)
            }
        }
        return Face(typeface)
    }

    private val systemFaces = HashMap<Triple<Typeface, Int, Boolean>, Typeface>()

    class Face(val typeface: Typeface, val skew: Boolean = false, val fakeBold: Boolean = false)

    companion object {
        @Volatile
        private var loaded: Fonts? = null

        fun of(context: Context): Fonts =
            loaded ?: synchronized(this) { loaded ?: Fonts(context.applicationContext).also { loaded = it } }
    }
}
