package io.github.gurov.brevier

import android.content.ContentProvider
import android.content.ContentValues
import android.content.Context
import android.database.Cursor
import android.database.MatrixCursor
import android.net.Uri
import android.os.ParcelFileDescriptor
import android.provider.OpenableColumns
import java.io.File
import java.io.FileNotFoundException

/**
 * Файлы, которые читатель отдаёт другому приложению через «Поделиться» (#38).
 *
 * `FileProvider` живёт в androidx, а его у нас нет нарочно; провайдеру же
 * нужно немного: отдать на чтение файл из одной папки кэша по имени и
 * сказать его имя и размер. Чужое приложение читает только по праву,
 * выданному на один адрес (`FLAG_GRANT_READ_URI_PERMISSION`), — провайдер
 * не экспортирован.
 */
class Shared : ContentProvider() {
    override fun onCreate() = true

    private fun file(uri: Uri): File {
        val context = context ?: throw FileNotFoundException()
        val dir = File(context.cacheDir, DIR).canonicalFile
        val name = uri.lastPathSegment ?: throw FileNotFoundException()
        val file = File(dir, name).canonicalFile
        // Только прямо из своей папки: `..` в адресе не выводит наружу.
        if (file.parentFile != dir || !file.isFile) throw FileNotFoundException(name)
        return file
    }

    override fun openFile(uri: Uri, mode: String): ParcelFileDescriptor {
        if (mode != "r") throw SecurityException("read only")
        return ParcelFileDescriptor.open(file(uri), ParcelFileDescriptor.MODE_READ_ONLY)
    }

    override fun getType(uri: Uri): String? = when (uri.lastPathSegment?.substringAfterLast('.', "")?.lowercase()) {
        "png" -> "image/png"
        "jpg" -> "image/jpeg"
        "gif" -> "image/gif"
        "webp" -> "image/webp"
        "avif" -> "image/avif"
        "svg" -> "image/svg+xml"
        // Выгрузка заметок (#42).
        "zip" -> "application/zip"
        "md" -> "text/markdown"
        else -> "application/octet-stream"
    }

    override fun query(uri: Uri, projection: Array<out String>?, selection: String?, args: Array<out String>?, order: String?): Cursor {
        val file = file(uri)
        val columns = projection ?: arrayOf(OpenableColumns.DISPLAY_NAME, OpenableColumns.SIZE)
        val cursor = MatrixCursor(columns)
        cursor.addRow(columns.map { column ->
            when (column) {
                OpenableColumns.DISPLAY_NAME -> file.name
                OpenableColumns.SIZE -> file.length()
                else -> null
            }
        })
        return cursor
    }

    override fun insert(uri: Uri, values: ContentValues?): Uri? = null
    override fun update(uri: Uri, values: ContentValues?, selection: String?, args: Array<out String>?) = 0
    override fun delete(uri: Uri, selection: String?, args: Array<out String>?) = 0

    companion object {
        /** Папка в кэше приложения: всё, что в ней, можно отдать по праву. */
        const val DIR = "shared"

        fun uri(context: Context, file: File): Uri =
            Uri.Builder().scheme("content").authority("${context.packageName}.shared").appendPath(file.name).build()

        /**
         * Убрать отданное раньше. Не всё: приложение, которому отдали
         * картинку минуту назад, может ещё читать её.
         */
        fun sweep(dir: File) {
            val old = System.currentTimeMillis() - 60 * 60 * 1000L
            dir.listFiles()?.filter { it.lastModified() < old }?.forEach { it.delete() }
        }
    }
}
