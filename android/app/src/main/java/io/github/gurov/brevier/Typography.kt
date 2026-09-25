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

    fun palette(dark: Boolean) = if (dark) this.dark else light

    companion object {
        private fun floats(json: JSONObject, key: String) =
            json.getJSONArray(key).map { (it as Number).toFloat() }

        fun color(hex: String): Int = Color.parseColor(hex)

        /** Одна на процесс: модель неизменна, спрашивать ядро дважды незачем. */
        val shared: Typography by lazy { Typography(Core.typography()) }
    }
}

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

    /** Линейка цитаты: приглушённая краска вполсилы — как в окне. */
    val quoteRule: Int get() = Color.argb(128, Color.red(dim), Color.green(dim), Color.blue(dim))
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
class Metrics(context: Context, val type: Typography, val zoom: Float) {
    val text: Float = TypedValue.applyDimension(
        TypedValue.COMPLEX_UNIT_SP, type.textSize * zoom, context.resources.displayMetrics,
    )
    /** Сколько наших пикселей в пикселе десктопа. */
    val unit: Float = text / type.textPx
    /** Воздух между строками и абзацами — та же величина, что `extra` в окне. */
    val extra: Float = (type.lineHeight - 1f) * type.textSize * unit
    /** Мера: колонка не шире этого. На телефоне она обычно — весь экран. */
    val measure: Float = type.measureInEms * text

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
    fun pick(mono: Boolean, weight: Int, italic: Boolean): Face {
        if (mono) return Face(this.mono, skew = italic, fakeBold = weight >= 600)
        return when {
            weight >= 600 -> if (italic) Face(boldItalic) else Face(bold)
            weight >= 500 -> Face(medium, skew = italic)
            weight >= 400 -> if (italic) Face(this.italic) else Face(regular)
            else -> Face(light, skew = italic)
        }
    }

    class Face(val typeface: Typeface, val skew: Boolean = false, val fakeBold: Boolean = false)

    companion object {
        @Volatile
        private var loaded: Fonts? = null

        fun of(context: Context): Fonts =
            loaded ?: synchronized(this) { loaded ?: Fonts(context.applicationContext).also { loaded = it } }
    }
}
