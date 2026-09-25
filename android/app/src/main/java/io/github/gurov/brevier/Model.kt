package io.github.gurov.brevier

import org.json.JSONArray
import org.json.JSONObject

/*
 * Страница, как её отдаёт ядро (`page.rs`): текст, участки со стилями,
 * ссылки, якоря, оглавление и объекты. Смещения — в единицах UTF-16, то есть
 * прямо в индексах строки Kotlin.
 */

class Run(val start: Int, val end: Int, val styles: List<String>)

class Link(val start: Int, val end: Int, val target: String)

/** Строка оглавления: заголовок автора или веха, поставленная ядром. */
class Mark(val level: Int, val title: String, val at: Int, val heading: Boolean)

sealed class Block(val at: Int) {
    /** Картинка. `inline` — внутри строки (формула): роста с текст, без подписи. */
    class Image(at: Int, val source: String, val alt: String, val inline: Boolean) : Block(at)

    class Table(at: Int, val align: List<String>, val rows: List<Row>) : Block(at)
}

class Row(val header: Boolean, val cells: List<Cell>)

class Cell(val text: String, val runs: List<Run>, val links: List<Link>)

class Page(
    val text: String,
    val runs: List<Run>,
    val links: List<Link>,
    val anchors: List<Pair<String, Int>>,
    val contents: List<Mark>,
    val blocks: List<Block>,
) {
    /** Где якорь. Первый с таким именем: на него и ведёт ссылка. */
    fun anchor(name: String): Int? = anchors.firstOrNull { it.first == name }?.second

    companion object {
        fun of(json: JSONObject): Page = Page(
            text = json.getString("text"),
            runs = runs(json.getJSONArray("runs")),
            links = links(json.getJSONArray("links")),
            anchors = json.getJSONArray("anchors").map { item ->
                val pair = item as JSONArray
                pair.getString(0) to pair.getInt(1)
            },
            contents = json.getJSONArray("contents").map { item ->
                val mark = item as JSONObject
                Mark(mark.getInt("level"), mark.getString("title"), mark.getInt("at"), mark.getBoolean("heading"))
            },
            blocks = json.getJSONArray("blocks").map { item -> block(item as JSONObject) },
        )

        private fun block(json: JSONObject): Block {
            val at = json.getInt("at")
            return when (json.getString("kind")) {
                "table" -> Block.Table(
                    at,
                    json.getJSONArray("align").map { it as String },
                    json.getJSONArray("rows").map { item ->
                        val row = item as JSONObject
                        Row(row.getBoolean("header"), row.getJSONArray("cells").map { cell ->
                            val c = cell as JSONObject
                            Cell(c.getString("text"), runs(c.getJSONArray("runs")), links(c.getJSONArray("links")))
                        })
                    },
                )
                else -> Block.Image(at, json.getString("source"), json.getString("alt"), json.getBoolean("inline"))
            }
        }

        private fun runs(json: JSONArray): List<Run> = json.map { item ->
            val run = item as JSONArray
            Run(run.getInt(0), run.getInt(1), run.getJSONArray(2).map { it as String })
        }

        private fun links(json: JSONArray): List<Link> = json.map { item ->
            val link = item as JSONArray
            Link(link.getInt(0), link.getInt(1), link.getString(2))
        }
    }
}

/** Точка входа на полке: строка, ведущая в другой документ. */
class Entry(val title: String, val address: String)

/**
 * Что вернуло открытие адреса: документ или отказ. Отказ тоже страница —
 * ядро раскладывает его тем же трактом, что и статью.
 */
class Loaded(
    val ok: Boolean,
    val address: String,
    val external: String,
    val title: String,
    val listing: Boolean,
    val served: Boolean,
    val kept: Boolean,
    val insecure: Boolean,
    val internal: Boolean,
    /** Решётка из адреса, уже приведённая: куда прокрутить после загрузки. */
    val anchor: String?,
    val site: List<Entry>,
    val directory: Entry?,
    /** Проект репозитория: точки входа в документацию принадлежат ему. */
    val project: String?,
    /** Адрес проверки этой страницы (`brevier:check/…`), если её есть что проверять. */
    val check: String?,
    val page: Page,
    /** Отказ: заголовок и чем открыть страницу снаружи, если это поможет. */
    val headline: String?,
    val offer: String?,
) {
    companion object {
        fun of(json: JSONObject, asked: String): Loaded {
            val ok = json.optBoolean("ok")
            return Loaded(
                ok = ok,
                address = json.optString("address", asked),
                external = json.optString("external", ""),
                title = if (ok) json.optString("title") else json.optString("headline"),
                listing = json.optBoolean("listing"),
                served = json.optBoolean("served"),
                kept = json.optBoolean("kept"),
                insecure = json.optBoolean("insecure"),
                internal = json.optBoolean("internal"),
                anchor = json.text("anchor"),
                site = json.optJSONArray("site")?.map { entry(it as JSONObject) } ?: emptyList(),
                directory = json.optJSONObject("directory")?.let(::entry),
                project = json.text("project"),
                check = json.text("check"),
                page = Page.of(json.getJSONObject("page")),
                headline = if (ok) null else json.optString("headline"),
                offer = json.text("offer"),
            )
        }

        fun entry(json: JSONObject) = Entry(json.getString("title"), json.getString("address"))
    }
}

fun <T> JSONArray.map(transform: (Any) -> T): List<T> = (0 until length()).map { transform(get(it)) }
