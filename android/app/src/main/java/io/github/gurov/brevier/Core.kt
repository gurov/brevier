package io.github.gurov.brevier

import android.content.Context
import android.util.Log
import org.json.JSONArray
import org.json.JSONObject

/**
 * Ядро Brevier: общая библиотека и четыре функции. Контракт описан
 * в `src/android.rs`; здесь — только обёртки, чтобы остальной код не собирал
 * строки с разделителями руками.
 *
 * Всё, что ходит в сеть (`open`, `docs`, `save`, `image`), зовётся не из
 * главного потока: сеть в ядре синхронная.
 */
object Core {
    init {
        System.loadLibrary("brevier")
    }

    /** Разделитель полей аргумента `call`: его не бывает ни в адресах, ни в заголовках. */
    private const val FIELD = '\u001f'

    external fun init(context: Context, home: String)
    private external fun call(method: String, arg: String): String
    /**
     * Картинка, разобранная ядром. `null` бывает, когда нативная сторона
     * не смогла даже ответить (так ведёт себя arm64-ядро под трансляцией
     * на x86_64-эмуляторе): это отказ одной картинки, а не повод падать.
     */
    external fun image(source: String, page: String, width: Int, paper: Int, ink: Int, fontSize: Float, natural: Boolean): ByteArray?

    /** Картинка из ответа [image] — или почему её нет. */
    class Raster(val bitmap: android.graphics.Bitmap?, val trouble: String?, val depth: Int = 0)

    /**
     * Разобрать ответ [image]: последний байт — что это; `0` — RGBA с нулевого
     * байта, за ними ширина и высота (big-endian), `1` — слова отказа.
     *
     * Пиксели читаются буфером с самого начала массива (#58): со сдвигом
     * (`ByteBuffer.wrap(bytes, 9, …)`) Android 9 и раньше в
     * `copyPixelsFromBuffer` освобождал чужой адрес — SIGSEGV, процесс
     * умирал мимо обработчика исключений и мимо сторожа (#57).
     */
    fun raster(bytes: ByteArray?): Raster {
        if (bytes == null || bytes.isEmpty()) return Raster(null, "The load fell through")
        val size = bytes.size
        if (bytes[size - 1].toInt() != 0) return Raster(null, String(bytes, 0, size - 1))
        if (size < 13) return Raster(null, "The load fell through")
        val tail = java.nio.ByteBuffer.wrap(bytes, size - 13, 12)
        val w = tail.int
        val h = tail.int
        val depth = tail.int
        if (w <= 0 || h <= 0 || w.toLong() * h * 4 > size - 13) return Raster(null, "The load fell through")
        return try {
            val bitmap = android.graphics.Bitmap.createBitmap(w, h, android.graphics.Bitmap.Config.ARGB_8888)
            bitmap.copyPixelsFromBuffer(java.nio.ByteBuffer.wrap(bytes))
            Raster(bitmap, null, depth)
        } catch (failure: Throwable) {
            Raster(null, "The image did not fit in memory")
        }
    }
    private external fun find(text: String, needle: String): IntArray

    /**
     * Вызов ядра, который не роняет приложение. Паника в ядре приходит сюда
     * исключением Java (`with_env` в `src/android.rs`); не пойманное, оно
     * закрыло бы приложение со всеми вкладками из-за одной страницы. Ответ
     * тогда — `fallback`: поля `opt*` берут умолчания, `headline` объясняет,
     * а `open` без `page` уходит в «The load fell through», как и прежде.
     */
    private fun ask(method: String, arg: String, fallback: String = BROKE): String = try {
        call(method, arg)
    } catch (failure: RuntimeException) {
        Log.w(TAG, "core call $method failed", failure)
        fallback
    }

    /** Совпадения поиска (`find`); упало ядро — совпадений нет. */
    fun search(text: String, needle: String): IntArray = try {
        find(text, needle)
    } catch (failure: RuntimeException) {
        Log.w(TAG, "core find failed", failure)
        IntArray(0)
    }

    private const val TAG = "Brevier"
    private const val BROKE = "{\"error\":\"panic\",\"headline\":\"Brevier ran into a bug\"}"

    private fun obj(method: String, vararg fields: Any): JSONObject =
        JSONObject(ask(method, fields.joinToString(FIELD.toString())))

    private fun list(method: String, arg: String): JSONArray = JSONArray(ask(method, arg, "[]"))

    fun typography(): JSONObject = obj("typography")
    fun intro(): JSONObject = obj("intro")
    fun parse(typed: String): JSONObject = obj("parse", typed)

    /**
     * Смещение от UTC в секундах — ядро пишет его в журнал рядом со временем.
     * `fresh` — мимо недельной копии на диске, прямо из сети (перезагрузка).
     */
    fun open(address: String, fresh: Boolean = false): JSONObject =
        obj("open", utcOffset(), address, if (fresh) "1" else "0")
    fun follow(here: String, target: String): JSONObject = obj("follow", here, target)

    /**
     * Секунда чтения (#19): видимое `[from, to)` в UTF-16. Ответ — доли
     * прочитанного по разделам полки, в порядке её строк.
     */
    fun readingTick(address: String, from: Int, to: Int): FloatArray {
        val shares = obj("reading.tick", address, from, to, utcOffset()).optJSONArray("shares")
            ?: return FloatArray(0)
        return FloatArray(shares.length()) { shares.optDouble(it).toFloat() }
    }

    /**
     * Вехи раздела, где читатель (#19): место `at` и экран `screen` — сколько знаков
     * на нём в среднем (`ArticleView.screenUnits`), — в UTF-16. Ответ — номер раздела
     * среди строк оглавления (`-1` — делить нечего) и вехи.
     */
    fun readingInner(address: String, at: Int, screen: Int): Pair<Int, List<ShelfRow.Jump>> {
        val json = obj("reading.inner", address, at, screen)
        val marks = json.optJSONArray("marks") ?: return -1 to emptyList()
        return json.optInt("section", -1) to (0 until marks.length()).map { index ->
            val mark = marks.getJSONObject(index)
            ShelfRow.Jump(
                mark.text("title") ?: "",
                mark.optInt("level"),
                mark.optInt("at"),
                heading = false,
                place = mark.text("place"),
                inner = true,
            )
        }
    }

    /** Уход со страницы (#19): запомненная записывается с местом у верха экрана. */
    fun readingLeave(address: String, top: Int) {
        ask("reading.leave", listOf(address, top.toString(), utcOffset().toString()).joinToString(FIELD.toString()))
    }
    fun suggest(typed: String): JSONArray = list("suggest", typed)
    fun titleOf(address: String): String? = obj("title", address).text("title")
    fun kept(address: String): Boolean = obj("kept", address).optBoolean("kept")
    fun bookmark(address: String, title: String): Boolean =
        obj("bookmark", utcOffset(), address, title).optBoolean("kept")

    fun settings(): JSONObject = obj("settings")
    /**
     * `contrast` — выбор читателя (#44); `null` — не выбирал, решает система.
     * `slideBar` — уезжает ли шапка, пока читают (#45).
     */
    fun saveSettings(
        dark: Boolean, images: Boolean, archive: Boolean, contrast: Boolean?, slideBar: Boolean, notes: Boolean,
        paper: String, darkPaper: String, fullScreen: Boolean, turning: Turning,
        ownAgent: Boolean, agent: String,
    ) {
        val flags = listOf(dark, images, archive).map { if (it) "1" else "0" } +
            when (contrast) { true -> "1"; false -> "0"; null -> "" } +
            (if (slideBar) "1" else "0") +
            (if (notes) "1" else "0") +
            paper + darkPaper +
            (if (fullScreen) "1" else "0") +
            turning.words() +
            (if (ownAgent) "1" else "0") +
            agent.replace(FIELD, ' ')
        ask("settings.save", flags.joinToString(FIELD.toString()))
    }

    /** Набор читателя (#4) — в `settings.tsv` рядом с темой. */
    fun saveType(type: ReaderType) {
        val fields = listOf(
            type.face, type.size.toString(), type.measure.toString(), type.leading.toString(),
            if (type.justify) "1" else "0", type.paragraphs.toString(),
        )
        ask("type.save", fields.joinToString(FIELD.toString()))
    }

    fun session(): JSONArray = list("session", "")

    /**
     * Приложение падает на этой странице (#57): пометку — ядру, сразу. Иначе
     * сессия подняла бы её снова, и приложение падало бы на каждом запуске.
     */
    fun fell(address: String) {
        ask("fell", address)
    }

    /**
     * Картинка от запроса до готового `Bitmap` — под пометкой сторожа (#58):
     * нативное падение в самом Android (`copyPixelsFromBuffer` на Android 9)
     * проходит мимо обработчика исключений, и без пометки следующий запуск
     * открыл бы ту же страницу и упал снова.
     */
    fun <T> holdingPicture(work: () -> T): T {
        ask("picture.hold", "")
        try {
            return work()
        } finally {
            ask("picture.release", "")
        }
    }

    /** Вкладка сессии: текущая ли, где в истории, где в тексте и сам путь. */
    class Opened(val current: Boolean, val at: Int, val place: Int, val addresses: List<String>)

    fun remember(tabs: List<Opened>) {
        val lines = tabs.joinToString("\n") { tab ->
            (listOf(if (tab.current) "1" else "0", tab.at.toString(), tab.place.toString()) + tab.addresses)
                .joinToString(FIELD.toString())
        }
        ask("session.save", lines)
    }

    fun forget() {
        ask("forget", "")
    }

    fun docs(address: String): JSONArray = list("docs", address)
    fun saveName(address: String): String? = obj("savename", address).text("name")
    fun save(address: String, path: String): JSONObject = obj("save", address, path)

    /**
     * Картинку — себе (#38): ядро пишет её в `dir` под именем с расширением
     * по содержимому. Ответ — `ok`, `path`, `name`, `mime` или `headline`.
     * Может пойти в сеть: зовётся не из главного потока.
     */
    fun keepPicture(source: String, dir: String): JSONObject = obj("picture.keep", source, dir)

    /** Заметки открытой статьи (#39): найденные — в порядке текста, потерявшие место — в конце. */
    fun notes(address: String): List<NoteSpot> {
        val found = list("notes.of", address)
        return (0 until found.length()).map { NoteSpot.of(found.getJSONObject(it)) }
    }

    /** Заметка по выделению `[from, to)` в UTF-16. Ответ — `ok` и `id` или `headline`. */
    fun addNote(address: String, from: Int, to: Int, text: String): JSONObject =
        obj("notes.add", utcOffset(), address, from, to, text)

    fun editNote(id: Long, text: String): Boolean = obj("notes.edit", id, text).optBoolean("ok")
    fun removeNote(id: Long): Boolean = obj("notes.remove", id).optBoolean("ok")

    /**
     * Картинку из файла `path` — в заметку (#42); файл ядро потом стирает.
     * Ответ — `ok` или `headline`.
     */
    fun attachPicture(id: Long, path: String): JSONObject = obj("notes.attach", id, path)
    fun detachPicture(id: Long, path: String): Boolean = obj("notes.detach", id, path).optBoolean("ok")

    /** Заметки страницы `address` (пусто — все) — файлом в папке `dir`: `ok` и `path` или `headline`. */
    fun exportNotes(address: String, dir: String): JSONObject = obj("notes.export", address, dir)

    private fun utcOffset(): Int = java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000
}

/**
 * Строка из JSON или `null`. Не `optString`: тот отдаёт JSON `null` строкой
 * «null», и вкладка так и называлась.
 */
fun JSONObject.text(key: String): String? = if (isNull(key)) null else optString(key).ifEmpty { null }
