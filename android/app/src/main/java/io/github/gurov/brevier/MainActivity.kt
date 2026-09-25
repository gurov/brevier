package io.github.gurov.brevier

import android.app.Activity
import android.app.AlertDialog
import android.content.ActivityNotFoundException
import android.content.ClipData
import android.content.ClipboardManager
import android.content.ComponentName
import android.content.Context
import android.content.Intent
import android.content.pm.PackageManager
import android.content.res.ColorStateList
import android.graphics.drawable.GradientDrawable
import android.net.Uri
import android.os.Build
import android.os.Bundle
import android.os.Handler
import android.os.Looper
import android.provider.OpenableColumns
import android.text.Editable
import android.text.InputType
import android.text.TextUtils
import android.text.TextWatcher
import android.util.TypedValue
import android.view.Gravity
import android.view.KeyEvent
import android.view.View
import android.view.ViewGroup
import android.view.WindowInsets
import android.view.inputmethod.EditorInfo
import android.view.inputmethod.InputMethodManager
import android.widget.BaseAdapter
import android.widget.EditText
import android.widget.FrameLayout
import android.widget.ImageButton
import android.widget.ImageView
import android.widget.LinearLayout
import android.widget.ListPopupWindow
import android.widget.TextView
import java.io.File

/**
 * Окно Brevier на телефоне: вкладки, адресная строка, статья, полка.
 *
 * Логика та же, что у окна на десктопе (`src/ui/gtk.rs`), и названия
 * функций там и здесь совпадают там, где совпадает работа: `open`,
 * `showDocument`, `step`, `sync`, `rememberSession`. Отличается раскладка:
 * корешков в ряд и полки рядом со статьёй на телефоне не уместить.
 */
class MainActivity : Activity(), ArticleHost {
    private val type by lazy { Typography.shared }
    private lateinit var fonts: Fonts
    private val main = Handler(Looper.getMainLooper())

    private var dark = false
    private var images = true
    private var zoom = 0

    private val tabs = mutableListOf<Tab>()
    private var current = 0
    private var nextId = 1L

    private lateinit var root: FrameLayout
    private lateinit var bar: LinearLayout
    private lateinit var back: ImageButton
    private lateinit var field: FrameLayout
    private lateinit var address: EditText
    private lateinit var insecure: ImageView
    private lateinit var contents: ImageButton
    private lateinit var tabCount: TextView
    private lateinit var more: ImageButton
    private lateinit var stage: FrameLayout
    private lateinit var findBar: LinearLayout
    private lateinit var needle: EditText
    private lateinit var tally: TextView
    private lateinit var notice: TextView
    private lateinit var shelf: Shelf
    private lateinit var tabList: TabList
    private lateinit var settings: SettingsPage
    private lateinit var hints: ListPopupWindow

    /** Строку адреса окно правит и само — пока правит, подсказки молчат. */
    private var quiet = false
    private var hintRows: List<Pair<String, String>> = emptyList()

    /** Поиск: совпадения на открытой странице и на котором стоим. */
    private var hits = IntArray(0)
    private var hit = 0

    private val palette: Palette get() = type.palette(dark)

    override fun onCreate(saved: Bundle?) {
        super.onCreate(saved)
        fonts = Fonts.of(this)
        val stored = Core.settings()
        dark = stored.optBoolean("dark")
        images = stored.optBoolean("images", true)
        zoom = type.zoomNormal

        build()
        applyTheme()
        // На старте клавиатуру не поднимаем: программу открыли читать, а не печатать.
        if (!restoreSession()) newTab(null, ask = false)
        handle(intent)
    }

    override fun onNewIntent(intent: Intent) {
        super.onNewIntent(intent)
        handle(intent)
    }

    /**
     * Поворот экрана: строки переливаются под новую ширину, и пиксели
     * прокрутки означают уже другое место. Держим смещение в тексте.
     */
    override fun onConfigurationChanged(config: android.content.res.Configuration) {
        val view = currentTab()?.view
        val place = view?.takeIf { currentTab()?.shown != null }?.place()
        super.onConfigurationChanged(config)
        if (view != null && place != null) view.post { view.settle(place, 0f) }
    }

    override fun onPause() {
        super.onPause()
        rememberSession()
    }

    /**
     * Адрес снаружи открывается вкладкой, а не окном: ссылка означает «покажи
     * ещё одну страницу». Так же и на десктопе.
     */
    private fun handle(intent: Intent?) {
        val asked = when (intent?.action) {
            Intent.ACTION_VIEW -> intent.dataString
            Intent.ACTION_SEND -> intent.getStringExtra(Intent.EXTRA_TEXT)?.let(::firstLink)
            else -> null
        } ?: return
        intent?.action = null
        val parsed = Core.parse(asked)
        if (!parsed.optBoolean("ok")) return
        val address = parsed.getString("address")
        // Пустая начальная вкладка — не чья-то страница: адрес занимает её,
        // а не заводит рядом вторую.
        val blank = currentTab()?.takeIf { it.current() == null && !it.loading }
        if (blank != null) open(blank, address, remember = true) else newTab(address)
    }

    /** «Поделиться» приносит текст вокруг ссылки — берём саму ссылку. */
    private fun firstLink(text: String): String? =
        Regex("""https?://\S+""").find(text)?.value ?: text.trim().ifEmpty { null }

    // ── сборка окна ─────────────────────────────────────────────────────────

    private fun build() {
        root = FrameLayout(this)
        val column = LinearLayout(this).apply { orientation = LinearLayout.VERTICAL }
        root.addView(column, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))

        bar = LinearLayout(this).apply {
            gravity = Gravity.CENTER_VERTICAL
            setPadding(dp(4f), dp(4f), dp(4f), dp(4f))
        }
        column.addView(bar, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))

        stage = FrameLayout(this)
        column.addView(stage, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, 0, 1f))

        findBar = LinearLayout(this).apply {
            gravity = Gravity.CENTER_VERTICAL
            visibility = View.GONE
            setPadding(dp(12f), dp(2f), dp(4f), dp(2f))
        }
        column.addView(findBar, LinearLayout.LayoutParams(LinearLayout.LayoutParams.MATCH_PARENT, LinearLayout.LayoutParams.WRAP_CONTENT))

        notice = TextView(this).apply {
            visibility = View.GONE
            typeface = fonts.regular
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 14f)
            setPadding(dp(14f), dp(10f), dp(14f), dp(10f))
        }
        root.addView(notice, FrameLayout.LayoutParams(FrameLayout.LayoutParams.WRAP_CONTENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.BOTTOM or Gravity.CENTER_HORIZONTAL).apply {
            bottomMargin = dp(20f)
            leftMargin = dp(16f)
            rightMargin = dp(16f)
        })

        shelf = Shelf(this, fonts) { row -> pickShelf(row) }
        root.addView(shelf, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        tabList = TabList(this, fonts,
            choose = { index -> tabList.visibility = View.GONE; switchTo(index) },
            close = { index -> closeTab(index); fillTabList() },
            fresh = { tabList.visibility = View.GONE; newTab(null) },
        )
        root.addView(tabList, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        settings = SettingsPage(this, fonts,
            changed = { dark, images -> changeSettings(dark, images) },
            forget = { forgetEverything() },
            browser = { askToBeBrowser() },
        )
        root.addView(settings, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))

        address = EditText(this).apply {
            isSingleLine = true
            inputType = InputType.TYPE_CLASS_TEXT or InputType.TYPE_TEXT_VARIATION_URI
            imeOptions = EditorInfo.IME_ACTION_GO or EditorInfo.IME_FLAG_NO_EXTRACT_UI
            hint = "address"
            typeface = fonts.regular
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            setSelectAllOnFocus(true)
            ellipsize = TextUtils.TruncateAt.END
            setOnEditorActionListener { _, action, event ->
                val go = action == EditorInfo.IME_ACTION_GO ||
                    (event?.keyCode == KeyEvent.KEYCODE_ENTER && event.action == KeyEvent.ACTION_DOWN)
                if (go) go(text.toString())
                go
            }
            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
                override fun afterTextChanged(s: Editable?) {
                    if (!quiet && hasFocus()) offerHints(s.toString())
                }
            })
            setOnFocusChangeListener { _, focused -> if (!focused) hints.dismiss() }
        }
        insecure = ImageView(this).apply {
            setImageResource(R.drawable.ic_insecure)
            visibility = View.GONE
            contentDescription = INSECURE
            tooltip(INSECURE)
            setOnClickListener { notice(INSECURE) }
        }
        field = FrameLayout(this)
        field.addView(address, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.WRAP_CONTENT, Gravity.CENTER_VERTICAL))
        field.addView(insecure, FrameLayout.LayoutParams(dp(18f), dp(18f), Gravity.CENTER_VERTICAL or Gravity.START).apply { leftMargin = dp(10f) })

        hints = ListPopupWindow(this).apply {
            anchorView = field
            isModal = false
            inputMethodMode = ListPopupWindow.INPUT_METHOD_NEEDED
            setOnItemClickListener { _, _, position, _ ->
                val chosen = hintRows.getOrNull(position) ?: return@setOnItemClickListener
                dismiss()
                go(chosen.first)
            }
        }

        tabCount = TextView(this).apply {
            gravity = Gravity.CENTER
            typeface = fonts.medium
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 12f)
            contentDescription = "Tabs"
            tooltip("Tabs")
            setOnClickListener { showTabList() }
        }

        setContentView(root)
        root.setOnApplyWindowInsetsListener { view, insets -> fitInsets(view, insets) }
    }

    /**
     * Под edge-to-edge (target 35) системные панели лежат поверх окна:
     * отступаем от них сами, и от клавиатуры тоже.
     */
    private fun fitInsets(view: View, insets: WindowInsets): WindowInsets {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            val bars = insets.getInsets(WindowInsets.Type.systemBars() or WindowInsets.Type.displayCutout())
            val ime = insets.getInsets(WindowInsets.Type.ime())
            view.setPadding(bars.left, bars.top, bars.right, maxOf(bars.bottom, ime.bottom))
        } else {
            @Suppress("DEPRECATION")
            view.setPadding(insets.systemWindowInsetLeft, insets.systemWindowInsetTop, insets.systemWindowInsetRight, insets.systemWindowInsetBottom)
        }
        return insets
    }

    /** Панель: назад, адрес, полка, вкладки, меню. Значки перекрашиваются с темой. */
    private fun fillBar() {
        bar.removeAllViews()
        val p = palette
        back = iconButton(R.drawable.ic_back, "Back", p) { step(backwards = true) }
        bar.addView(back)
        bar.addView(field, LinearLayout.LayoutParams(0, dp(40f), 1f).apply {
            leftMargin = dp(2f)
            rightMargin = dp(2f)
        })
        contents = iconButton(R.drawable.ic_contents, "Contents", p) { toggleShelf() }
        bar.addView(contents)
        bar.addView(tabCount, LinearLayout.LayoutParams(dp(40f), dp(40f)))
        more = iconButton(R.drawable.ic_menu, "Menu", p) { showMenu() }
        bar.addView(more)
    }

    private fun fillFindBar() {
        findBar.removeAllViews()
        val p = palette
        needle = EditText(this).apply {
            isSingleLine = true
            hint = "find on page"
            typeface = fonts.regular
            setTextSize(TypedValue.COMPLEX_UNIT_SP, 15f)
            imeOptions = EditorInfo.IME_ACTION_SEARCH or EditorInfo.IME_FLAG_NO_EXTRACT_UI
            setTextColor(p.ink)
            setHintTextColor(p.dim)
            background = null
            addTextChangedListener(object : TextWatcher {
                override fun beforeTextChanged(s: CharSequence?, start: Int, count: Int, after: Int) {}
                override fun onTextChanged(s: CharSequence?, start: Int, before: Int, count: Int) {}
                override fun afterTextChanged(s: Editable?) = find(s.toString(), restart = true)
            })
            setOnEditorActionListener { _, _, _ -> stepHit(forward = true); true }
        }
        tally = label("", fonts, p.dim, 13f)
        findBar.addView(needle, LinearLayout.LayoutParams(0, LinearLayout.LayoutParams.WRAP_CONTENT, 1f))
        findBar.addView(tally)
        findBar.addView(iconButton(R.drawable.ic_up, "Previous", p) { stepHit(forward = false) })
        findBar.addView(iconButton(R.drawable.ic_down, "Next", p) { stepHit(forward = true) })
        findBar.addView(iconButton(R.drawable.ic_close, "Close search", p) { closeFind() })
    }

    // ── тема ────────────────────────────────────────────────────────────────

    /**
     * Тема. Шапка и поля — в тот же тёплый ряд, что и бумага: холодная панель
     * системы рядом со слоновой костью выдаёт склейку.
     */
    private fun applyTheme() {
        val p = palette
        root.setBackgroundColor(p.shelf)
        bar.setBackgroundColor(p.shelf)
        findBar.setBackgroundColor(p.shelf)
        address.setTextColor(p.ink)
        address.setHintTextColor(p.dim)
        address.highlightColor = p.chosen
        field.background = GradientDrawable().apply {
            setColor(p.paper)
            setStroke(dp(1f), p.rule)
            cornerRadius = dp(20f).toFloat()
        }
        address.background = null
        insecure.imageTintList = ColorStateList.valueOf(p.dim)
        tabCount.setTextColor(p.ink)
        tabCount.background = GradientDrawable().apply {
            setStroke(dp(1.5f), p.ink)
            cornerRadius = dp(4f).toFloat()
        }.let { frame ->
            android.graphics.drawable.InsetDrawable(frame, dp(9f))
        }
        notice.setTextColor(p.paper)
        notice.background = GradientDrawable().apply {
            setColor(p.ink)
            cornerRadius = dp(6f).toFloat()
        }
        fillBar()
        fillFindBar()
        shelf.paint(p)
        tabList.paint(p)
        // Панели системы прозрачные (тема), окно рисуется под ними на любой
        // версии, а не только на Android 15, где это обязательно: отступы от
        // панелей тогда считает одна `fitInsets`, и нигде не удваиваются.
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.R) {
            // На Android 15 это и так всегда `false`, оттуда и пометка «устарело».
            @Suppress("DEPRECATION")
            window.setDecorFitsSystemWindows(false)
            val light = android.view.WindowInsetsController.APPEARANCE_LIGHT_STATUS_BARS or
                android.view.WindowInsetsController.APPEARANCE_LIGHT_NAVIGATION_BARS
            window.insetsController?.setSystemBarsAppearance(if (dark) 0 else light, light)
        } else {
            @Suppress("DEPRECATION")
            window.decorView.systemUiVisibility = View.SYSTEM_UI_FLAG_LAYOUT_STABLE or
                View.SYSTEM_UI_FLAG_LAYOUT_FULLSCREEN or View.SYSTEM_UI_FLAG_LAYOUT_HIDE_NAVIGATION or
                if (dark) 0 else View.SYSTEM_UI_FLAG_LIGHT_STATUS_BAR
        }
        sync()
    }

    private fun metrics() = Metrics(this, type, type.zoomSteps[zoom])

    // ── вкладки ─────────────────────────────────────────────────────────────

    /** `ask` — поднять клавиатуру к адресу: читатель сам попросил пустую вкладку. */
    private fun newTab(address: String?, switch: Boolean = true, ask: Boolean = true) {
        val tab = Tab(nextId++)
        tabs += tab
        // Пустая вкладка — не пустой экран: `render` покажет начальную страницу,
        // тем же трактом, что и статью.
        if (switch) switchTo(tabs.size - 1)
        if (address != null) {
            open(tab, address, remember = true)
        } else if (switch && ask) {
            this.address.requestFocus()
            showKeyboard(this.address)
        }
        rememberSession()
    }

    private fun closeTab(index: Int) {
        if (index !in tabs.indices) return
        val tab = tabs.removeAt(index)
        // Уходя, бросаем загрузку: слушать её больше некому.
        tab.generation += 1
        tab.view = null
        if (tabs.isEmpty()) {
            current = 0
            newTab(null)
            return
        }
        if (index < current || current >= tabs.size) current -= 1
        current = current.coerceIn(0, tabs.size - 1)
        switchTo(current)
        rememberSession()
    }

    /**
     * Вывести вкладку на экран. Виджет статьи есть только у видимой вкладки:
     * фоновая отдаёт его, запомнив место чтения, а вернувшись — рисуется
     * заново из памяти, сразу в текущих теме и масштабе. Десять вкладок
     * с Википедией иначе держали бы десять деревьев виджетов, и система
     * выгрузила бы приложение.
     */
    private fun switchTo(index: Int) {
        if (index !in tabs.indices) return
        val tab = tabs[index]
        val old = tabs.getOrNull(current)
        if (old != null && old !== tab) release(old)
        current = index
        val fresh = tab.view == null
        val view = tab.view ?: ArticleView(this, this).also { tab.view = it }
        if (view.parent !== stage) {
            stage.removeAllViews()
            (view.parent as? ViewGroup)?.removeView(view)
            stage.addView(view, FrameLayout.LayoutParams(FrameLayout.LayoutParams.MATCH_PARENT, FrameLayout.LayoutParams.MATCH_PARENT))
        }
        if (tab.pending) wakeTab(tab) else if (fresh) render(tab)
        closeFind()
        sync()
        rememberSession()
    }

    /** Отпустить виджет фоновой вкладки, запомнив, где в ней читали. */
    private fun release(tab: Tab) {
        val view = tab.view ?: return
        if (tab.shown != null && tab.resume == null) tab.resume = view.place()
        tab.view = null
    }

    private fun currentTab(): Tab? = tabs.getOrNull(current)

    /** Открыть вкладку, которая до сих пор была только вкладкой. */
    private fun wakeTab(tab: Tab) {
        if (!tab.pending) return
        tab.pending = false
        tab.current()?.let { open(tab, it, remember = false) }
    }

    private fun showTabList() {
        fillTabList()
        tabList.visibility = View.VISIBLE
        hideKeyboard()
    }

    private fun fillTabList() {
        tabList.fill(tabs.map { it.title to (it.current() ?: "") }, current)
    }

    // ── открытие ────────────────────────────────────────────────────────────

    /** Набранное в строке: разобрать, открыть в текущей вкладке. */
    private fun go(typed: String) {
        val text = typed.trim()
        if (text.isEmpty()) return
        hints.dismiss()
        currentTab()?.view?.requestFocus()
        address.clearFocus()
        hideKeyboard()
        val parsed = Core.parse(text)
        val tab = currentTab() ?: return
        if (!parsed.optBoolean("ok")) {
            val failure = Loaded.of(parsed, text)
            tab.shown = failure
            tab.title = failure.title
            render(tab)
            sync()
            return
        }
        open(tab, parsed.getString("address"), remember = true)
    }

    private fun open(tab: Tab, address: String, remember: Boolean, fresh: Boolean = false) {
        if (remember) {
            // Место на покидаемой странице — чтобы «назад» вернул сюда.
            val view = tab.view
            if (tab.shown != null && view != null) tab.setPlace(view.place())
            tab.visit(address)
            tab.prune()
        }
        tab.generation += 1
        val generation = tab.generation
        // «Назад» и «вперёд» по уже показанной странице — из памяти, без сети.
        // Свежий заход кэш обходит: там читатель просит именно новую загрузку.
        val cached = if (!remember && !fresh && !address.startsWith("brevier:")) tab.pages[address] else null
        if (cached != null) {
            showDocument(tab, cached)
            return
        }

        tab.loading = true
        tab.shown = null
        tab.title = "Loading…"
        sync()
        tickLoading(tab, generation, 1)

        App.work.execute {
            val loaded = try {
                Loaded.of(Core.open(address, fresh), address)
            } catch (failure: Throwable) {
                null
            }
            main.post {
                if (tab.generation != generation || tab !in tabs) return@post
                when {
                    loaded == null -> {
                        tab.loading = false
                        tab.title = "The load fell through"
                        tab.view?.show(message("The load fell through"), metrics(), dark, eager = false)
                        sync()
                    }
                    loaded.ok -> {
                        if (!loaded.internal) tab.pages[address] = loaded
                        showDocument(tab, loaded)
                    }
                    else -> showDocument(tab, loaded)
                }
            }
        }
    }

    /** «Loading» с точками: признак жизни, а не доля загруженного. */
    private fun tickLoading(tab: Tab, generation: Long, dots: Int) {
        if (tab.generation != generation || !tab.loading) return
        tab.dots = dots
        tab.view?.show(message("Loading" + ".".repeat(dots)), metrics(), dark, eager = false)
        main.postDelayed({ tickLoading(tab, generation, dots % LOADING_DOTS + 1) }, 350)
    }

    /** Страница-сообщение, собранная здесь: для загрузки ядро не нужно. */
    private fun message(text: String) = Page(text, listOf(Run(0, text.length, listOf("h2"))), emptyList(), emptyList(), emptyList(), emptyList())

    /** Начальная страница: у ядра, один раз на запуск. */
    private val intro by lazy { Core.intro() }

    /**
     * Нарисовать вкладку тем, что у неё есть: документом, загрузкой или
     * начальной страницей. Место — отложенное (`resume`), иначе решётка
     * из адреса, если страница только что приехала.
     */
    private fun render(tab: Tab) {
        val view = tab.view ?: return
        val shown = tab.shown
        when {
            shown != null -> {
                view.show(shown.page, metrics(), dark, eager = images && shown.ok, offer = shown.offer)
                val resume = tab.resume
                tab.resume = null
                val anchor = if (tab.anchorPending) shown.anchor?.let { shown.page.anchor(it) } else null
                tab.anchorPending = false
                when {
                    resume != null && resume > 0 -> view.settle(resume, 0f)
                    anchor != null -> view.settle(anchor, ANCHOR_ALIGN)
                }
            }
            tab.loading || tab.pending -> view.show(message("Loading" + ".".repeat(tab.dots)), metrics(), dark, eager = false)
            tab.current() == null -> {
                tab.title = intro.optString("title", "Brevier")
                view.show(Page.of(intro.getJSONObject("page")), metrics(), dark, eager = false)
            }
        }
    }

    /**
     * Разложить готовый документ по вкладке и привести окно в порядок. Общий
     * путь для свежей загрузки и показа из кэша «назад/вперёд». Фоновая
     * вкладка только запоминает документ — нарисуется, когда её покажут.
     */
    private fun showDocument(tab: Tab, loaded: Loaded) {
        tab.loading = false
        tab.shown = loaded
        tab.title = loaded.title.ifEmpty { loaded.address }
        tab.anchorPending = true
        render(tab)
        if (tab == currentTab()) {
            // Список ссылок показываем как есть, но говорим, что это он.
            if (loaded.listing) notice(LISTING)
            // Из копии — не то, что на сайте сейчас; об этом строкой.
            else if (loaded.copy != null) notice("${loaded.copy} Reload, in the menu, loads the page afresh.")
            else if (loaded.served) notice(SERVED_MARKDOWN)
        }
        if (loaded.ok) seekEntries(tab, loaded)
        sync()
        rememberSession()
    }

    /** Перерисовать видимую вкладку под новую ступень или тему, не теряя места чтения. */
    private fun redraw(tab: Tab) {
        val view = tab.view ?: return
        if (tab.shown != null) tab.resume = view.place()
        render(tab)
        if (hits.isNotEmpty() && tab == currentTab()) find(needle.text.toString(), restart = false)
    }

    /** Загрузить открытую страницу заново, из сети, на том же месте. */
    private fun reload() {
        val tab = currentTab() ?: return
        val address = tab.current() ?: return
        val view = tab.view
        if (tab.shown != null && view != null) tab.resume = view.place()
        open(tab, address, remember = false, fresh = true)
    }

    /** Шаг по истории текущей вкладки, с возвратом на прежнее место. */
    private fun step(backwards: Boolean) {
        val tab = currentTab() ?: return
        val view = tab.view ?: return
        if (tab.shown != null) tab.setPlace(view.place())
        val address = (if (backwards) tab.back() else tab.forward()) ?: return
        val place = tab.place()
        tab.resume = if (place > 0) place else null
        open(tab, address, remember = false)
    }

    /**
     * Точки входа в документацию проекта — фоном: статья уже на экране,
     * полка дополнится, когда ответ придёт. Ищем на проект, а не на файл.
     */
    private fun seekEntries(tab: Tab, loaded: Loaded) {
        val project = loaded.project
        if (project == null) {
            tab.entries = emptyList()
            tab.entriesFor = null
            return
        }
        if (tab.entriesFor == project) return
        tab.entries = emptyList()
        tab.entriesFor = project
        val asked = loaded.address
        App.work.execute {
            val found = try {
                Core.docs(asked).map { Loaded.entry(it as org.json.JSONObject) }
            } catch (failure: Throwable) {
                emptyList()
            }
            main.post {
                if (tab.entriesFor != project) return@post
                tab.entries = found
                if (tab == currentTab()) sync()
            }
        }
    }

    // ── окно под вкладку ────────────────────────────────────────────────────

    /** Привести окно в соответствие с открытой вкладкой. */
    private fun sync() {
        val tab = currentTab() ?: return
        if (!::back.isInitialized) return
        val shown = tab.shown
        back.isEnabled = tab.canGoBack()
        back.alpha = if (tab.canGoBack()) 1f else 0.35f
        setAddress(tab.current() ?: "")
        val insecureNow = (tab.current() ?: "").startsWith("http://")
        insecure.visibility = if (insecureNow) View.VISIBLE else View.GONE
        address.setPadding(if (insecureNow) dp(34f) else dp(14f), 0, dp(12f), 0)
        tabCount.text = if (tabs.size > 99) ":D" else tabs.size.toString()
        title = if (tab.current() == null) "Brevier" else "${tab.title} — Brevier"
        fillShelf(tab, shown)
        contents.isEnabled = !shelf.empty
        contents.alpha = if (shelf.empty) 0.35f else 1f
        if (tabList.visibility == View.VISIBLE) fillTabList()
    }

    private fun setAddress(text: String) {
        if (address.hasFocus()) return
        quiet = true
        address.setText(text)
        quiet = false
    }

    /**
     * Полка: проект, оглавление, сайт. Группа проекта и группа сайта подписаны
     * всегда; оглавление подписывается только под проектом.
     */
    private fun fillShelf(tab: Tab, shown: Loaded?) {
        val rows = mutableListOf<ShelfRow>()
        val project = tab.entries + listOfNotNull(shown?.directory)
        val marks = shown?.page?.contents ?: emptyList()
        if (project.isNotEmpty()) {
            rows += ShelfRow.Header("In this repository")
            project.forEach { rows += ShelfRow.Open(it.title, it.address, dim = false) }
        }
        if (project.isNotEmpty() && marks.isNotEmpty()) rows += ShelfRow.Header("On this page")
        marks.forEach { rows += ShelfRow.Jump(it.title, it.level, it.at, it.heading) }
        val site = shown?.site ?: emptyList()
        if (site.isNotEmpty()) {
            rows += ShelfRow.Header("On this site")
            site.forEach { rows += ShelfRow.Open(it.title, it.address, dim = true) }
        }
        shelf.fill(rows)
        if (shelf.empty) shelf.hide()
        scrolled()
    }

    private fun toggleShelf() {
        if (shelf.open) shelf.hide() else if (!shelf.empty) {
            shelf.show()
            scrolled()
            hideKeyboard()
        }
    }

    private fun pickShelf(row: ShelfRow) {
        shelf.hide()
        when (row) {
            is ShelfRow.Jump -> currentTab()?.view?.settle(row.at, ANCHOR_ALIGN)
            is ShelfRow.Open -> currentTab()?.let { open(it, row.address, remember = true) }
            is ShelfRow.Header -> {}
        }
    }

    // ── ArticleHost ─────────────────────────────────────────────────────────

    override fun follow(target: String, fresh: Boolean) {
        val tab = currentTab() ?: return
        val answer = Core.follow(tab.current() ?: "", target)
        val jump = answer.optString("jump").ifEmpty { null }
        if (jump != null) {
            val page = tab.view?.page
            val at = page?.anchor(jump)
            if (at != null) {
                tab.view?.settle(at, ANCHOR_ALIGN)
                return
            }
            // Такого якоря на странице нет — ссылка отрабатывает как обычная.
            val parsed = Core.parse(target)
            if (parsed.optBoolean("ok")) openLink(tab, parsed.getString("address"), fresh)
            return
        }
        val address = answer.optString("open").ifEmpty { null }
        if (address != null) openLink(tab, address, fresh)
        else notice(answer.optString("error", "Not a link we can open"))
    }

    private fun openLink(tab: Tab, address: String, fresh: Boolean) {
        if (fresh) {
            newTab(address, switch = false)
            notice("Opened in a new tab")
            sync()
        } else {
            open(tab, address, remember = true)
        }
    }

    /**
     * Что сделать со ссылкой. Сверху — куда она ведёт: на десктопе адрес
     * показывает подсказка при наведении, а у пальца наведения нет.
     */
    override fun linkMenu(target: String, x: Int, y: Int) {
        val menu = Menu(this, fonts, palette)
        menu.caption(target)
        menu.item("Open in a new tab") { follow(target, fresh = true) }
        menu.item("Copy link") {
            val clipboard = getSystemService(Context.CLIPBOARD_SERVICE) as ClipboardManager
            clipboard.setPrimaryClip(ClipData.newPlainText(null, target))
            notice("Link copied")
        }
        menu.item("Open in your browser") { openOutside(target) }
        menu.showAt(root, x, y)
    }

    /**
     * Отдать адрес браузеру системы — но не нам самим: если Brevier выбран
     * браузером по умолчанию, простой `VIEW` вернулся бы сюда же.
     */
    override fun openOutside(target: String) {
        val uri = Uri.parse(target)
        val intent = Intent(Intent.ACTION_VIEW, uri).addCategory(Intent.CATEGORY_BROWSABLE)
        // `MATCH_ALL`, а не умолчание: когда браузер по умолчанию выбран,
        // Android на веб-ссылку отвечает только им — то есть нами же,
        // и остальных браузеров без этого флага не видно вовсе.
        val handlers = packageManager.queryIntentActivities(intent, PackageManager.MATCH_ALL)
            .filter { it.activityInfo.packageName != packageName }
        val chosen = when (handlers.size) {
            0 -> {
                notice("No other browser is registered for links")
                return
            }
            // Другой браузер один — им и открываем, без лишнего вопроса.
            1 -> Intent(intent)
                .setComponent(ComponentName(handlers[0].activityInfo.packageName, handlers[0].activityInfo.name))
                .addFlags(Intent.FLAG_ACTIVITY_NEW_TASK)
            // Несколько — «твой браузер» уже не один: спрашиваем систему,
            // убрав из списка самих себя.
            else -> Intent.createChooser(intent, "Open in your browser").putExtra(
                Intent.EXTRA_EXCLUDE_COMPONENTS,
                arrayOf(ComponentName(this, MainActivity::class.java)),
            )
        }
        try {
            startActivity(chosen)
        } catch (failure: ActivityNotFoundException) {
            notice("No other browser is registered for links")
        }
    }

    override fun scrolled() {
        val view = currentTab()?.view ?: return
        if (!shelf.open) return
        // Мерим не по верхней кромке, а по той строке, куда ставит заголовок
        // прыжок по оглавлению, плюс полстроки запаса.
        val probe = view.scrollY + (view.height * ANCHOR_ALIGN).toInt() + (metrics().text * type.lineHeight / 2).toInt()
        shelf.follow(view.offsetAt(probe))
    }

    override fun zoom(step: Int) {
        val next = (zoom + step).coerceIn(0, type.zoomSteps.size - 1)
        if (next == zoom) return
        zoom = next
        currentTab()?.let { redraw(it) }
        notice("Zoom ${zoomLabel()}")
    }

    private fun zoomLabel() = "${Math.round(type.zoomSteps[zoom] * 100)}%"

    // ── подсказки ───────────────────────────────────────────────────────────

    private fun offerHints(typed: String) {
        if (typed.isBlank()) {
            hints.dismiss()
            return
        }
        val found = Core.suggest(typed)
        hintRows = found.map { item ->
            val hint = item as org.json.JSONObject
            hint.getString("address") to hint.getString("title")
        }
        if (hintRows.isEmpty()) {
            hints.dismiss()
            return
        }
        hints.setAdapter(HintAdapter())
        hints.setBackgroundDrawable(GradientDrawable().apply {
            setColor(palette.shelf)
            setStroke(dp(1f), palette.rule)
            cornerRadius = dp(8f).toFloat()
        })
        hints.width = field.width
        // Высота — по строкам, а не до клавиатуры: пустой хвост списка
        // закрывал страницу и ничего не предлагал.
        hints.height = hintRows.size * dp(if (hintRows.any { it.second.isNotEmpty() }) 60f else 40f) + dp(8f)
        if (!hints.isShowing) hints.show() else hints.show()
    }

    private inner class HintAdapter : BaseAdapter() {
        override fun getCount() = hintRows.size
        override fun getItem(position: Int) = hintRows[position]
        override fun getItemId(position: Int) = position.toLong()
        override fun getView(position: Int, convert: View?, parent: ViewGroup): View {
            val (target, title) = hintRows[position]
            return LinearLayout(this@MainActivity).apply {
                orientation = LinearLayout.VERTICAL
                setPadding(dp(14f), dp(8f), dp(14f), dp(8f))
                if (title.isNotEmpty()) addView(label(title, fonts, palette.ink, 14f).apply {
                    maxLines = 1
                    ellipsize = TextUtils.TruncateAt.END
                })
                addView(label(target, fonts, palette.dim, 12f).apply {
                    maxLines = 1
                    ellipsize = TextUtils.TruncateAt.MIDDLE
                })
            }
        }
    }

    // ── меню ────────────────────────────────────────────────────────────────

    private fun showMenu() {
        hideKeyboard()
        val tab = currentTab()
        val shown = tab?.shown
        val menu = Menu(this, fonts, palette)
        menu.icon(R.drawable.ic_forward, "Forward", enabled = tab?.canGoForward() == true) { step(backwards = false) }
        val keepable = shown != null && shown.ok && !shown.internal
        val kept = keepable && Core.kept(tab!!.current() ?: "")
        menu.icon(if (kept) R.drawable.ic_star else R.drawable.ic_star_border, "Keep this page", enabled = keepable) { keepPage() }
        menu.icon(R.drawable.ic_save, "Save the article", enabled = shown != null && shown.ok) { askWhereToSave() }
        menu.icon(R.drawable.ic_search, "Find on page", enabled = tab?.view?.page != null) { openFind() }
        menu.item("New tab") { newTab(null) }
        menu.zoom({ zoomLabel() }) { step -> zoom(step) }
        menu.item("Open in your browser") {
            val target = tab?.shown?.external?.ifEmpty { null }
            if (target == null) notice("Nothing to open outside") else openOutside(target)
        }
        menu.item("Reload") { reload() }
        // Отчёт — новой вкладкой: его читают рядом со страницей, а не вместо неё.
        menu.item("Check this page") {
            val target = tab?.shown?.check
            if (target == null) notice("Only a web page can be checked") else newTab(target)
        }
        menu.item("History") { newTab("brevier:history") }
        menu.item("Bookmarks") { newTab("brevier:bookmarks") }
        menu.item("Settings") { showSettings() }
        menu.show(more)
    }

    private fun showSettings() = settings.show(palette, dark, images, isBrowser())

    /**
     * Brevier ли браузер по умолчанию. С Android 10 это роль, у которой
     * есть держатель; раньше — то, кого система выберет для веб-ссылки.
     */
    private fun isBrowser(): Boolean =
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            getSystemService(android.app.role.RoleManager::class.java)
                ?.isRoleHeld(android.app.role.RoleManager.ROLE_BROWSER) == true
        } else {
            val web = Intent(Intent.ACTION_VIEW, Uri.parse("https://example.com")).addCategory(Intent.CATEGORY_BROWSABLE)
            packageManager.resolveActivity(web, PackageManager.MATCH_DEFAULT_ONLY)?.activityInfo?.packageName == packageName
        }

    /**
     * Попросить систему сделать Brevier браузером. Решает читатель, в диалоге
     * системы; если диалога нет (Android до 10, оболочка производителя, два
     * отказа подряд) — открываем экран приложений по умолчанию.
     */
    private fun askToBeBrowser() {
        if (Build.VERSION.SDK_INT >= Build.VERSION_CODES.Q) {
            val roles = getSystemService(android.app.role.RoleManager::class.java)
            if (roles != null && roles.isRoleAvailable(android.app.role.RoleManager.ROLE_BROWSER) &&
                !roles.isRoleHeld(android.app.role.RoleManager.ROLE_BROWSER)
            ) {
                try {
                    @Suppress("DEPRECATION")
                    startActivityForResult(roles.createRequestRoleIntent(android.app.role.RoleManager.ROLE_BROWSER), ROLE)
                    return
                } catch (failure: ActivityNotFoundException) {
                    // Дальше — экран настроек.
                }
            }
        }
        try {
            startActivity(Intent(android.provider.Settings.ACTION_MANAGE_DEFAULT_APPS_SETTINGS))
        } catch (failure: ActivityNotFoundException) {
            notice("Choose the browser in the system settings, under default apps")
        }
    }

    /** Закладку ставят руками — звёздочкой, про ту страницу, что на экране. */
    private fun keepPage() {
        val tab = currentTab() ?: return
        val address = tab.current() ?: return
        val kept = Core.bookmark(address, tab.title)
        notice(if (kept) "Kept in bookmarks" else "Removed from bookmarks")
    }

    private fun changeSettings(dark: Boolean, images: Boolean) {
        val themeChanged = dark != this.dark
        val imagesOn = images && !this.images
        this.dark = dark
        this.images = images
        Core.saveSettings(dark, images)
        if (themeChanged) {
            applyTheme()
            showSettings()
            currentTab()?.let { redraw(it) }
        }
        if (imagesOn) currentTab()?.view?.loadAll()
    }

    /** Забыть всё прочитанное — с вопросом, потому что назад это не отыграть. */
    private fun forgetEverything() {
        AlertDialog.Builder(this)
            .setTitle("Forget everything you have read?")
            .setMessage("The list of pages goes away, the address bar stops suggesting them, and the saved copies of pages are deleted. Bookmarks and open tabs stay.")
            .setNegativeButton("Cancel", null)
            .setPositiveButton("Forget") { _, _ ->
                Core.forget()
                notice("The list of pages you have read is empty now")
            }
            .show()
    }

    // ── поиск ───────────────────────────────────────────────────────────────

    private fun openFind() {
        findBar.visibility = View.VISIBLE
        needle.requestFocus()
        showKeyboard(needle)
        if (needle.text.isNotEmpty()) find(needle.text.toString(), restart = true)
    }

    private fun closeFind() {
        if (!::needle.isInitialized || findBar.visibility != View.VISIBLE) return
        findBar.visibility = View.GONE
        hits = IntArray(0)
        currentTab()?.view?.highlight(hits, -1)
        hideKeyboard()
    }

    /** Найти всё и встать на ближайшее совпадение от того места, что видно. */
    private fun find(text: String, restart: Boolean) {
        val view = currentTab()?.view ?: return
        val page = view.page ?: return
        hits = if (text.isEmpty()) IntArray(0) else Core.find(page.text, text)
        val total = hits.size / 2
        if (restart) {
            val top = view.place()
            hit = (0 until total).firstOrNull { hits[it * 2] >= top } ?: 0
        } else {
            hit = hit.coerceIn(0, maxOf(0, total - 1))
        }
        showHit()
    }

    private fun stepHit(forward: Boolean) {
        val total = hits.size / 2
        if (total == 0) return
        hit = if (forward) (hit + 1) % total else (hit + total - 1) % total
        showHit()
    }

    private fun showHit() {
        val view = currentTab()?.view ?: return
        val total = hits.size / 2
        tally.text = when {
            total == 0 && needle.text.isNotEmpty() -> "no matches"
            total == 0 -> ""
            else -> "${hit + 1} of $total"
        }
        view.highlight(hits, if (total == 0) -1 else hit)
    }

    // ── сохранение ──────────────────────────────────────────────────────────

    /**
     * Спросить, куда класть статью. Имя предлагает ядро: есть картинки —
     * архив, нет — просто текст. Читатель волен переименовать, и расширение
     * из диалога старше нашего.
     */
    private fun askWhereToSave() {
        val address = currentTab()?.shown?.address ?: run {
            notice("Nothing to save yet")
            return
        }
        val name = Core.saveName(address) ?: run {
            notice("Nothing to save yet")
            return
        }
        pendingSave = address
        val intent = Intent(Intent.ACTION_CREATE_DOCUMENT).apply {
            addCategory(Intent.CATEGORY_OPENABLE)
            type = if (name.endsWith(".zip")) "application/zip" else "text/markdown"
            putExtra(Intent.EXTRA_TITLE, name)
        }
        try {
            @Suppress("DEPRECATION")
            startActivityForResult(intent, SAVE)
        } catch (failure: ActivityNotFoundException) {
            notice("Not saved: there is no place to save files")
        }
    }

    private var pendingSave: String? = null

    @Deprecated("Deprecated in Java")
    override fun onActivityResult(requestCode: Int, resultCode: Int, data: Intent?) {
        @Suppress("DEPRECATION")
        super.onActivityResult(requestCode, resultCode, data)
        if (requestCode == ROLE) {
            if (settings.visibility == View.VISIBLE) showSettings()
            if (isBrowser()) notice("Links from other apps now open in Brevier")
            return
        }
        if (requestCode != SAVE) return
        val uri = data?.data
        val address = pendingSave
        pendingSave = null
        if (resultCode != RESULT_OK || uri == null || address == null) return
        saveTo(uri, address)
    }

    /** Записать статью: ядро пишет во временный файл, сюда же копируем в выбранное место. */
    private fun saveTo(uri: Uri, address: String) {
        notice("Saving…")
        val name = displayName(uri) ?: "article.md"
        App.work.execute {
            val temp = File(cacheDir, name)
            val said = try {
                val result = Core.save(address, temp.path)
                if (!result.optBoolean("ok")) {
                    "Not saved: ${result.optString("headline")}"
                } else {
                    contentResolver.openOutputStream(uri)?.use { out -> temp.inputStream().use { it.copyTo(out) } }
                    val images = result.optInt("images")
                    val missed = result.optInt("missed")
                    buildString {
                        append("Saved as $name")
                        if (images > 0) append(" · ${count(images, "image", "images")}")
                        if (missed > 0) append(" · ${count(missed, "image", "images")} could not be fetched")
                    }
                }
            } catch (failure: Throwable) {
                "Not saved: the write was interrupted"
            } finally {
                temp.delete()
            }
            main.post { notice(said) }
        }
    }

    private fun displayName(uri: Uri): String? =
        contentResolver.query(uri, arrayOf(OpenableColumns.DISPLAY_NAME), null, null, null)?.use { cursor ->
            if (cursor.moveToFirst()) cursor.getString(0) else null
        }

    private fun count(howMany: Int, one: String, many: String) = if (howMany == 1) "$howMany $one" else "$howMany $many"

    // ── сессия ──────────────────────────────────────────────────────────────

    /** Запомнить открытое: вкладки, их путь и место в тексте. */
    private fun rememberSession() {
        val tabs = tabs.map { tab ->
            Core.Opened(
                current = tab == currentTab(),
                at = maxOf(0, tab.at),
                // Место, которое ещё не применили, старше того, что показывает
                // виджет: у вкладки, которая пока не грузилась, он ответит «ноль».
                place = tab.resume ?: tab.view?.takeIf { tab.shown != null }?.place() ?: 0,
                addresses = tab.addresses.toList(),
            )
        }
        App.work.execute { Core.remember(tabs) }
    }

    /**
     * Открыть заново то, что было открыто. Грузим ровно одну вкладку — ту,
     * что была впереди; остальные ждут, пока на них переключатся.
     */
    private fun restoreSession(): Boolean {
        val session = Core.session()
        var front = -1
        for (index in 0 until session.length()) {
            val item = session.getJSONObject(index)
            val path = item.getJSONArray("addresses").map { it as String }
            if (path.isEmpty()) continue
            val tab = Tab(nextId++)
            tab.restore(path, item.getInt("at"))
            tab.resume = item.getInt("place").takeIf { it > 0 }
            tab.pending = true
            tab.title = tab.current()?.let { Core.titleOf(it) ?: it } ?: "New tab"
            tabs += tab
            if (item.getBoolean("current")) front = tabs.size - 1
        }
        if (tabs.isEmpty()) return false
        switchTo(if (front >= 0) front else 0)
        return true
    }

    // ── мелочи ──────────────────────────────────────────────────────────────

    /** Строка состояния внизу: сама и убирается. */
    private fun notice(said: String) {
        notice.text = said
        notice.visibility = View.VISIBLE
        main.removeCallbacksAndMessages(NOTICE)
        main.postAtTime({ if (notice.text == said) notice.visibility = View.GONE }, NOTICE, android.os.SystemClock.uptimeMillis() + 8000)
    }

    private fun showKeyboard(view: View) {
        view.post {
            (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).showSoftInput(view, InputMethodManager.SHOW_IMPLICIT)
        }
    }

    private fun hideKeyboard() {
        val focus = currentFocus ?: root
        (getSystemService(Context.INPUT_METHOD_SERVICE) as InputMethodManager).hideSoftInputFromWindow(focus.windowToken, 0)
    }

    @Deprecated("Deprecated in Java")
    override fun onBackPressed() {
        when {
            hints.isShowing -> hints.dismiss()
            settings.visibility == View.VISIBLE -> settings.visibility = View.GONE
            tabList.visibility == View.VISIBLE -> tabList.visibility = View.GONE
            shelf.open -> shelf.hide()
            findBar.visibility == View.VISIBLE -> closeFind()
            address.hasFocus() -> {
                currentTab()?.view?.requestFocus()
                address.clearFocus()
                hideKeyboard()
                sync()
            }
            currentTab()?.canGoBack() == true -> step(backwards = true)
            // Вкладки и место в них уже в сессии: уходим, не закрываясь.
            else -> moveTaskToBack(true)
        }
    }

    /** Клавиатура — у планшетов и телефонов с чехлом: те же сочетания, что на десктопе. */
    override fun dispatchKeyEvent(event: KeyEvent): Boolean {
        if (event.action == KeyEvent.ACTION_DOWN && event.isCtrlPressed) {
            val done = when (event.keyCode) {
                KeyEvent.KEYCODE_L -> { address.requestFocus(); address.selectAll(); true }
                KeyEvent.KEYCODE_T -> { newTab(null); true }
                KeyEvent.KEYCODE_W -> { closeTab(current); true }
                KeyEvent.KEYCODE_TAB, KeyEvent.KEYCODE_PAGE_DOWN -> {
                    val step = if (event.isShiftPressed || event.keyCode == KeyEvent.KEYCODE_PAGE_UP) -1 else 1
                    switchTo((current + step + tabs.size) % tabs.size); true
                }
                KeyEvent.KEYCODE_PAGE_UP -> { switchTo((current - 1 + tabs.size) % tabs.size); true }
                KeyEvent.KEYCODE_F -> { openFind(); true }
                KeyEvent.KEYCODE_S -> { askWhereToSave(); true }
                KeyEvent.KEYCODE_H -> { newTab("brevier:history"); true }
                KeyEvent.KEYCODE_D -> { keepPage(); true }
                KeyEvent.KEYCODE_R -> { reload(); true }
                KeyEvent.KEYCODE_O -> { currentTab()?.shown?.external?.ifEmpty { null }?.let(::openOutside); true }
                KeyEvent.KEYCODE_PLUS, KeyEvent.KEYCODE_EQUALS, KeyEvent.KEYCODE_NUMPAD_ADD -> { zoom(+1); true }
                KeyEvent.KEYCODE_MINUS, KeyEvent.KEYCODE_NUMPAD_SUBTRACT -> { zoom(-1); true }
                KeyEvent.KEYCODE_0 -> { zoom(type.zoomNormal - zoom); true }
                else -> false
            }
            if (done) return true
        }
        if (event.action == KeyEvent.ACTION_DOWN && event.keyCode == KeyEvent.KEYCODE_F5) {
            reload()
            return true
        }
        if (event.action == KeyEvent.ACTION_DOWN && event.isAltPressed) {
            when (event.keyCode) {
                KeyEvent.KEYCODE_DPAD_LEFT -> { step(backwards = true); return true }
                KeyEvent.KEYCODE_DPAD_RIGHT -> { step(backwards = false); return true }
            }
        }
        return super.dispatchKeyEvent(event)
    }

    companion object {
        private const val SAVE = 1
        private const val ROLE = 2
        private val NOTICE = Any()
        /** Докуда растёт «Loading…», прежде чем начать сначала. */
        private const val LOADING_DOTS = 10
        /** Куда по высоте окна ставить заголовок, к которому прыгнули. */
        const val ANCHOR_ALIGN = 0.1f
        const val LISTING = "A list of links, not an article — pick one to read."
        const val SERVED_MARKDOWN = "Served as Markdown by the site — the author's exact text."
        const val INSECURE = "Not secure: this page came over plain http, so anyone on the way can read and change it."
    }
}
