package io.github.gurov.brevier

import android.content.Context
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
    external fun image(source: String, width: Int, paper: Int, fontSize: Float, natural: Boolean): ByteArray
    external fun find(text: String, needle: String): IntArray

    private fun obj(method: String, vararg fields: Any): JSONObject =
        JSONObject(call(method, fields.joinToString(FIELD.toString())))

    private fun list(method: String, arg: String): JSONArray = JSONArray(call(method, arg))

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
        call("reading.leave", listOf(address, top.toString(), utcOffset().toString()).joinToString(FIELD.toString()))
    }
    fun suggest(typed: String): JSONArray = list("suggest", typed)
    fun titleOf(address: String): String? = obj("title", address).text("title")
    fun kept(address: String): Boolean = obj("kept", address).optBoolean("kept")
    fun bookmark(address: String, title: String): Boolean =
        obj("bookmark", utcOffset(), address, title).optBoolean("kept")

    fun settings(): JSONObject = obj("settings")
    fun saveSettings(dark: Boolean, images: Boolean, archive: Boolean) {
        val flags = listOf(dark, images, archive).map { if (it) "1" else "0" }
        call("settings.save", flags.joinToString(FIELD.toString()))
    }

    fun session(): JSONArray = list("session", "")

    /** Вкладка сессии: текущая ли, где в истории, где в тексте и сам путь. */
    class Opened(val current: Boolean, val at: Int, val place: Int, val addresses: List<String>)

    fun remember(tabs: List<Opened>) {
        val lines = tabs.joinToString("\n") { tab ->
            (listOf(if (tab.current) "1" else "0", tab.at.toString(), tab.place.toString()) + tab.addresses)
                .joinToString(FIELD.toString())
        }
        call("session.save", lines)
    }

    fun forget() {
        call("forget", "")
    }

    fun docs(address: String): JSONArray = list("docs", address)
    fun saveName(address: String): String? = obj("savename", address).text("name")
    fun save(address: String, path: String): JSONObject = obj("save", address, path)

    private fun utcOffset(): Int = java.util.TimeZone.getDefault().getOffset(System.currentTimeMillis()) / 1000
}

/**
 * Строка из JSON или `null`. Не `optString`: тот отдаёт JSON `null` строкой
 * «null», и вкладка так и называлась.
 */
fun JSONObject.text(key: String): String? = if (isNull(key)) null else optString(key).ifEmpty { null }
