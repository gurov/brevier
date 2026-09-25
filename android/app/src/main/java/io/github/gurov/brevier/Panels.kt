package io.github.gurov.brevier

import android.annotation.SuppressLint
import android.content.Context
import android.content.res.ColorStateList
import android.graphics.drawable.GradientDrawable
import android.graphics.drawable.StateListDrawable
import android.text.TextUtils
import android.util.TypedValue
import android.view.Gravity
import android.view.View
import android.view.ViewGroup
import android.widget.BaseAdapter
import android.widget.FrameLayout
import android.widget.ImageButton
import android.widget.LinearLayout
import android.widget.ListView
import android.widget.PopupWindow
import android.widget.ScrollView
import android.widget.Switch
import android.widget.TextView

/** Пиксели из dp: подписи и поля интерфейса, а не статьи. */
fun Context.dp(value: Float): Int = TypedValue.applyDimension(TypedValue.COMPLEX_UNIT_DIP, value, resources.displayMetrics).toInt()

/** Кнопка-значок: без рамки, с отметкой нажатия в тёплом ряду бумаги. */
fun Context.iconButton(icon: Int, description: String, palette: Palette, click: () -> Unit): ImageButton =
    ImageButton(this).apply {
        setImageResource(icon)
        contentDescription = description
        tooltip(description)
        imageTintList = ColorStateList.valueOf(palette.ink)
        background = pressable(palette.touched, dp(20f).toFloat())
        val pad = dp(8f)
        setPadding(pad, pad, pad, pad)
        setOnClickListener { click() }
    }

fun View.tooltip(text: String) {
    if (android.os.Build.VERSION.SDK_INT >= 26) tooltipText = text
}

/** Фон, темнеющий под пальцем. */
fun pressable(touched: Int, radius: Float): StateListDrawable = StateListDrawable().apply {
    addState(intArrayOf(android.R.attr.state_pressed), GradientDrawable().apply {
        setColor(touched)
        cornerRadius = radius
    })
    addState(intArrayOf(), GradientDrawable().apply { setColor(0) })
}

/** Подпись интерфейса: гарнитура та же, что у статьи, — продукт один. */
fun Context.label(text: String, fonts: Fonts, color: Int, sp: Float = 15f, medium: Boolean = false): TextView =
    TextView(this).apply {
        this.text = text
        typeface = if (medium) fonts.medium else fonts.regular
        setTextColor(color)
        setTextSize(TypedValue.COMPLEX_UNIT_SP, sp)
    }

/** Строка полки: оглавление прокручивает, строка проекта и сайта уводит. */
sealed class ShelfRow {
    class Header(val title: String) : ShelfRow()
    class Jump(val title: String, val level: Int, val at: Int, val heading: Boolean) : ShelfRow()
    class Open(val title: String, val address: String, val dim: Boolean) : ShelfRow()
}

/**
 * Полка — оглавление и навигация, выдвижная справа. На десктопе она стоит
 * рядом со статьёй; на телефоне рядом места нет, и она выезжает поверх.
 *
 * Групп три, в том же порядке, что в окне: точки входа в документацию проекта,
 * оглавление открытой страницы, навигация сайта. Группа проекта и группа
 * сайта подписаны всегда — их строки уводят со страницы, и знать об этом
 * читатель должен до нажатия.
 */
@SuppressLint("ViewConstructor")
class Shelf(context: Context, private val fonts: Fonts, private val pick: (ShelfRow) -> Unit) : FrameLayout(context) {
    private val scrim = View(context)
    private val panel = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
    private val title: TextView
    private val list = ListView(context)
    private var rows: List<ShelfRow> = emptyList()
    private var here = -1
    private lateinit var palette: Palette

    init {
        visibility = GONE
        addView(scrim, LayoutParams(LayoutParams.MATCH_PARENT, LayoutParams.MATCH_PARENT))
        scrim.setOnClickListener { hide() }
        title = TextView(context).apply {
            text = "Contents"
            typeface = fonts.medium
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 13f)
            setPadding(context.dp(16f), context.dp(14f), context.dp(16f), context.dp(8f))
        }
        panel.addView(title)
        list.divider = null
        list.adapter = Adapter()
        list.setOnItemClickListener { _, _, position, _ ->
            val row = rows.getOrNull(position) ?: return@setOnItemClickListener
            if (row !is ShelfRow.Header) pick(row)
        }
        panel.addView(list, LinearLayout.LayoutParams(LayoutParams.MATCH_PARENT, 0, 1f))
        val width = minOf(context.dp(320f), (context.resources.displayMetrics.widthPixels * 0.82f).toInt())
        addView(panel, LayoutParams(width, LayoutParams.MATCH_PARENT, Gravity.END))
    }

    fun paint(palette: Palette) {
        this.palette = palette
        scrim.setBackgroundColor(0x55000000)
        panel.setBackgroundColor(palette.shelf)
        title.setTextColor(palette.dim)
        (list.adapter as Adapter).notifyDataSetChanged()
    }

    fun fill(rows: List<ShelfRow>) {
        this.rows = rows
        here = -1
        (list.adapter as Adapter).notifyDataSetChanged()
    }

    val empty: Boolean get() = rows.isEmpty()
    val open: Boolean get() = visibility == VISIBLE

    fun show() {
        visibility = VISIBLE
        if (here >= 0) list.setSelection(maxOf(0, here - 2))
    }

    fun hide() {
        visibility = GONE
    }

    /** Отметить строку того места, где читатель сейчас. */
    fun follow(offset: Int?) {
        var found = -1
        if (offset != null) {
            for ((index, row) in rows.withIndex()) {
                if (row is ShelfRow.Jump) {
                    if (row.at <= offset) found = index else break
                }
            }
        }
        if (found != here) {
            here = found
            (list.adapter as Adapter).notifyDataSetChanged()
        }
    }

    private inner class Adapter : BaseAdapter() {
        override fun getCount() = rows.size
        override fun getItem(position: Int) = rows[position]
        override fun getItemId(position: Int) = position.toLong()
        override fun isEnabled(position: Int) = rows[position] !is ShelfRow.Header
        override fun getViewTypeCount() = 1
        override fun getItemViewType(position: Int) = 0

        override fun getView(position: Int, convert: View?, parent: ViewGroup): View {
            val view = (convert as? TextView) ?: TextView(context).apply {
                // Многоточие — только тому, что не влезло в пять строк: по
                // обрезанному заголовку раздел не опознать.
                maxLines = 5
                ellipsize = TextUtils.TruncateAt.END
            }
            val side = context.dp(16f)
            when (val row = rows[position]) {
                is ShelfRow.Header -> {
                    view.text = row.title
                    view.typeface = fonts.regular
                    view.setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
                    view.setTextColor(palette.dim)
                    view.setPadding(side, context.dp(14f), side, context.dp(4f))
                    view.background = null
                }
                is ShelfRow.Jump -> {
                    view.text = row.title
                    view.typeface = fonts.regular
                    view.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                    // Веха — не структура автора, а наша выжимка. Пусть это видно.
                    view.setTextColor(if (row.heading) palette.ink else palette.dim)
                    view.setPadding(side + context.dp(12f) * (row.level - 1).coerceAtLeast(0), context.dp(7f), side, context.dp(7f))
                    view.background = if (position == here) GradientDrawable().apply { setColor(palette.chosen) } else pressable(palette.touched, 0f)
                }
                is ShelfRow.Open -> {
                    view.text = row.title
                    view.typeface = fonts.regular
                    view.setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
                    view.setTextColor(if (row.dim) palette.dim else palette.ink)
                    view.setPadding(side, context.dp(7f), side, context.dp(7f))
                    view.background = pressable(palette.touched, 0f)
                }
            }
            return view
        }
    }
}

/** Список вкладок: на телефоне корешков в ряд не уместить, поэтому — листом. */
@SuppressLint("ViewConstructor")
class TabList(
    context: Context,
    private val fonts: Fonts,
    private val choose: (Int) -> Unit,
    private val close: (Int) -> Unit,
    private val fresh: () -> Unit,
) : LinearLayout(context) {
    private val head = LinearLayout(context)
    private val list = ListView(context)
    private val caption: TextView
    private var items: List<Pair<String, String>> = emptyList()
    private var current = 0
    private lateinit var palette: Palette
    private var add: ImageButton? = null
    private var hide: ImageButton? = null

    init {
        orientation = VERTICAL
        visibility = GONE
        isClickable = true
        head.gravity = Gravity.CENTER_VERTICAL
        caption = TextView(context).apply {
            text = "Tabs"
            typeface = fonts.medium
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 17f)
        }
        head.setPadding(context.dp(16f), context.dp(8f), context.dp(8f), context.dp(8f))
        head.addView(caption, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
        addView(head)
        list.divider = null
        list.adapter = Adapter()
        list.setOnItemClickListener { _, _, position, _ -> choose(position) }
        addView(list, LayoutParams(LayoutParams.MATCH_PARENT, 0, 1f))
    }

    fun paint(palette: Palette) {
        this.palette = palette
        setBackgroundColor(palette.shelf)
        caption.setTextColor(palette.ink)
        add?.let { head.removeView(it) }
        hide?.let { head.removeView(it) }
        add = context.iconButton(R.drawable.ic_add, "New tab", palette) { fresh() }.also { head.addView(it) }
        hide = context.iconButton(R.drawable.ic_close, "Close the list", palette) { visibility = GONE }.also { head.addView(it) }
        (list.adapter as Adapter).notifyDataSetChanged()
    }

    fun fill(items: List<Pair<String, String>>, current: Int) {
        this.items = items
        this.current = current
        (list.adapter as Adapter).notifyDataSetChanged()
        list.setSelection(maxOf(0, current - 2))
    }

    private inner class Adapter : BaseAdapter() {
        override fun getCount() = items.size
        override fun getItem(position: Int) = items[position]
        override fun getItemId(position: Int) = position.toLong()

        override fun getView(position: Int, convert: View?, parent: ViewGroup): View {
            val row = LinearLayout(context).apply {
                gravity = Gravity.CENTER_VERTICAL
                setPadding(context.dp(16f), context.dp(6f), context.dp(4f), context.dp(6f))
                background = if (position == current) GradientDrawable().apply { setColor(palette.chosen) } else pressable(palette.touched, 0f)
            }
            val text = LinearLayout(context).apply { orientation = VERTICAL }
            val (title, address) = items[position]
            text.addView(context.label(title, fonts, palette.ink, 15f).apply {
                maxLines = 2
                ellipsize = TextUtils.TruncateAt.END
            })
            if (address.isNotEmpty()) text.addView(context.label(address, fonts, palette.dim, 12f).apply {
                maxLines = 1
                ellipsize = TextUtils.TruncateAt.MIDDLE
            })
            row.addView(text, LayoutParams(0, LayoutParams.WRAP_CONTENT, 1f))
            row.addView(context.iconButton(R.drawable.ic_close, "Close tab", palette) { close(position) })
            // Нажатие — на самой строке: в строке с кнопкой `ListView` своё
            // нажатие строке не отдаёт.
            row.setOnClickListener { choose(position) }
            return row
        }
    }
}

/**
 * Настройки — страницей поверх статьи, как вкладка настроек на десктопе:
 * у каждой строки место для причины, которую в переключатель не положить.
 */
@SuppressLint("ViewConstructor", "UseSwitchCompatOrMaterialCode")
class SettingsPage(
    context: Context,
    private val fonts: Fonts,
    private val changed: (dark: Boolean, images: Boolean) -> Unit,
    private val forget: () -> Unit,
    private val browser: () -> Unit,
) : ScrollView(context) {
    private val column = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
    private var dark = false
    private var images = true

    init {
        visibility = GONE
        isClickable = true
        isFillViewport = true
        addView(column)
    }

    fun show(palette: Palette, dark: Boolean, images: Boolean, isBrowser: Boolean) {
        this.dark = dark
        this.images = images
        setBackgroundColor(palette.shelf)
        column.removeAllViews()
        val pad = context.dp(18f)
        column.setPadding(pad, context.dp(8f), pad, pad)

        val head = LinearLayout(context).apply { gravity = Gravity.CENTER_VERTICAL }
        head.addView(context.label("Settings", fonts, palette.ink, 17f, medium = true), LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        head.addView(context.iconButton(R.drawable.ic_close, "Close settings", palette) { visibility = GONE })
        column.addView(head)

        column.addView(section("Reading", palette))
        column.addView(row(
            "Dark theme", "Warm dark, in the same row as the ivory paper.",
            switch(palette, dark) { on -> this.dark = on; changed(this.dark, this.images) }, palette,
        ))
        column.addView(row(
            "Images",
            "Off means no decoding at all: after JavaScript is gone, the image decoder is the one serious attack surface left.",
            switch(palette, images) { on -> this.images = on; changed(this.dark, this.images) }, palette,
        ))

        column.addView(section("Links", palette))
        column.addView(row(
            "Default browser",
            if (isBrowser) {
                "Brevier is your browser: links from other apps open here, as new tabs. " +
                    "What needs JavaScript or a login goes on to your other browser — Open in your browser, in the menu."
            } else {
                "Links from other apps would open here, as new tabs. " +
                    "What needs JavaScript or a login goes on to your other browser — Open in your browser, in the menu."
            },
            button(if (isBrowser) "Change" else "Make default", palette.ink, palette) { browser() }, palette,
        ))

        column.addView(section("History", palette))
        column.addView(row(
            "Forget everything you have read",
            "The list at brevier:history goes away, the address bar stops suggesting those pages, and the saved copies of pages are deleted. Bookmarks and open tabs stay.",
            button("Forget", 0xffb3261e.toInt(), palette) { forget() }, palette,
        ))
        visibility = VISIBLE
    }

    private fun button(label: String, ink: Int, palette: Palette, act: () -> Unit) = TextView(context).apply {
        text = label
        typeface = fonts.medium
        setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
        setTextColor(ink)
        val side = context.dp(14f)
        setPadding(side, context.dp(8f), side, context.dp(8f))
        background = GradientDrawable().apply {
            setStroke(context.dp(1f), palette.rule)
            cornerRadius = context.dp(6f).toFloat()
        }
        setOnClickListener { act() }
    }

    private fun section(title: String, palette: Palette) =
        context.label(title, fonts, palette.dim, 13f, medium = true).apply {
            setPadding(0, context.dp(18f), 0, context.dp(4f))
        }

    private fun row(title: String, why: String, control: View, palette: Palette): View {
        val row = LinearLayout(context).apply {
            gravity = Gravity.CENTER_VERTICAL
            setPadding(0, context.dp(10f), 0, context.dp(10f))
        }
        val text = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
        text.addView(context.label(title, fonts, palette.ink, 15f))
        text.addView(context.label(why, fonts, palette.dim, 13f).apply { setPadding(0, context.dp(2f), context.dp(12f), 0) })
        row.addView(text, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(control)
        return row
    }

    private fun switch(palette: Palette, on: Boolean, flip: (Boolean) -> Unit) = Switch(context).apply {
        isChecked = on
        val states = arrayOf(intArrayOf(android.R.attr.state_checked), intArrayOf())
        thumbTintList = ColorStateList(states, intArrayOf(palette.link, palette.dim))
        trackTintList = ColorStateList(states, intArrayOf(palette.chosen, palette.rule))
        setOnCheckedChangeListener { _, checked -> flip(checked) }
    }
}

/**
 * Меню под «⋮»: ряд значков для того, что трогают часто, и список — для
 * того, что редко. Панель телефона не свалка: на ней то, что нужно на каждой
 * странице, остальное здесь.
 */
class Menu(
    private val context: Context,
    private val fonts: Fonts,
    private val palette: Palette,
) {
    private val popup = PopupWindow(context)
    private val column = LinearLayout(context).apply { orientation = LinearLayout.VERTICAL }
    private val icons = LinearLayout(context).apply { gravity = Gravity.CENTER_VERTICAL }

    init {
        column.background = GradientDrawable().apply {
            setColor(palette.shelf)
            setStroke(context.dp(1f), palette.rule)
            cornerRadius = context.dp(8f).toFloat()
        }
        column.setPadding(context.dp(4f), context.dp(4f), context.dp(4f), context.dp(6f))
        column.addView(icons)
        popup.contentView = column
        popup.isFocusable = true
        popup.isOutsideTouchable = true
        popup.setBackgroundDrawable(null)
        popup.width = context.dp(250f)
        popup.height = ViewGroup.LayoutParams.WRAP_CONTENT
        if (android.os.Build.VERSION.SDK_INT >= 21) popup.elevation = context.dp(6f).toFloat()
    }

    fun icon(icon: Int, description: String, enabled: Boolean = true, act: () -> Unit) {
        val button = context.iconButton(icon, description, palette) {
            popup.dismiss()
            act()
        }
        button.isEnabled = enabled
        button.alpha = if (enabled) 1f else 0.35f
        icons.addView(button, LinearLayout.LayoutParams(0, context.dp(48f), 1f))
    }

    fun item(title: String, act: () -> Unit) {
        val row = context.label(title, fonts, palette.ink, 15f).apply {
            setPadding(context.dp(16f), context.dp(11f), context.dp(16f), context.dp(11f))
            background = pressable(palette.touched, context.dp(4f).toFloat())
            setOnClickListener {
                popup.dismiss()
                act()
            }
        }
        column.addView(row, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))
    }

    /** Строка масштаба: «−  100%  +», меню при нажатии не закрывается. */
    fun zoom(level: () -> String, step: (Int) -> Unit) {
        val row = LinearLayout(context).apply {
            gravity = Gravity.CENTER_VERTICAL
            setPadding(context.dp(16f), 0, context.dp(4f), 0)
        }
        val shown = context.label(level(), fonts, palette.ink, 15f)
        row.addView(context.label("Zoom", fonts, palette.ink, 15f), LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        row.addView(context.iconButton(R.drawable.ic_remove, "Zoom out", palette) { step(-1); shown.text = level() })
        row.addView(shown.apply { gravity = Gravity.CENTER; minWidth = context.dp(52f) })
        row.addView(context.iconButton(R.drawable.ic_add, "Zoom in", palette) { step(+1); shown.text = level() })
        column.addView(row)
    }

    /** Подпись над пунктами: адрес ссылки, одной строкой, с многоточием в середине. */
    fun caption(text: String) {
        column.removeView(icons)
        val row = context.label(text, fonts, palette.dim, 12f).apply {
            maxLines = 2
            ellipsize = TextUtils.TruncateAt.END
            setPadding(context.dp(16f), context.dp(10f), context.dp(16f), context.dp(6f))
        }
        column.addView(row, 0)
        popup.width = minOf(context.dp(300f), context.resources.displayMetrics.widthPixels - context.dp(32f))
    }

    fun show(anchor: View) {
        popup.showAsDropDown(anchor, -context.dp(206f), -context.dp(4f))
    }

    /** Показать у точки на экране — там, где палец. Не за краем экрана. */
    fun showAt(root: View, x: Int, y: Int) {
        val screen = context.resources.displayMetrics
        column.measure(
            View.MeasureSpec.makeMeasureSpec(popup.width, View.MeasureSpec.EXACTLY),
            View.MeasureSpec.makeMeasureSpec(screen.heightPixels, View.MeasureSpec.AT_MOST),
        )
        val height = column.measuredHeight
        val left = x.coerceIn(context.dp(8f), maxOf(context.dp(8f), screen.widthPixels - popup.width - context.dp(8f)))
        val top = if (y + height + context.dp(16f) > screen.heightPixels) maxOf(context.dp(24f), y - height) else y
        popup.showAtLocation(root, Gravity.NO_GRAVITY, left, top)
    }
}
