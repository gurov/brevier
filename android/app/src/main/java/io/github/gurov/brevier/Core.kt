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
    external fun image(source: String, width: Int, paper: Int, fontSize: Float, natural: Boolean): ByteArray?
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
    fun saveSettings(dark: Boolean, images: Boolean, archive: Boolean) {
        val flags = listOf(dark, images, archive).map { if (it) "1" else "0" }
        ask("settings.save", flags.joinToString(FIELD.toString()))
    }

    /** Набор читателя (#4) — в `settings.tsv` рядом с темой. */
    fun saveType(type: ReaderType) {
        val fields = listOf(type.face, type.size.toString(), type.measure.toString(), type.leading.toString(), if (type.justify) "1" else "0")
        ask("type.save", fields.joinToString(FIELD.toString()))
    }

    fun session(): JSONArray = list("session", "")

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

    private fun utcOffset(): Int = java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000
}

/**
 * Строка из JSON или `null`. Не `optString`: тот отдаёт JSON `null` строкой
 * «null», и вкладка так и называлась.
 */
fun JSONObject.text(key: String): String? = if (isNull(key)) null else optString(key).ifEmpty { null }
