package io.github.gurov.brevier

import android.app.Application
import java.util.concurrent.ExecutorService
import java.util.concurrent.Executors

/**
 * Приложение: поднимает ядро раньше любой активности. Проверяющий
 * сертификаты обязан получить контекст до первого запроса, а папка
 * хранилища — до первого чтения истории.
 */
class App : Application() {
    override fun onCreate() {
        super.onCreate()
        Core.init(this, filesDir.path)
        // Необработанное исключение закрывает приложение, и ядро о нём
        // не знает: его пометки (#57) снимаются, как только кончилась работа
        // ядра, а падает потом Kotlin — на картинке, на раскладке. Перед тем
        // как закрыться, называем ядру страницу на экране: следующий запуск
        // не откроет её сам, а спросит.
        val previous = Thread.getDefaultUncaughtExceptionHandler()
        Thread.setDefaultUncaughtExceptionHandler { thread, failure ->
            MainActivity.showing?.let { address -> runCatching { Core.fell(address) } }
            previous?.uncaughtException(thread, failure)
        }
    }

    companion object {
        /** Страницы и всё, что читатель попросил сам: сохранить, поделиться. */
        val work: ExecutorService = Executors.newFixedThreadPool(4)

        /**
         * Картинки — своим пулом: их на странице десятки, и каждая на плохой
         * сети идёт полминуты. В общем пуле за ними стояла следующая страница
         * и запись сессии. Несколько потоков: медленная не держит остальные.
         */
        val pictures: ExecutorService = Executors.newFixedThreadPool(4)

        /**
         * Сессия — по одной и по порядку: иначе старый снимок мог лечь поверх
         * нового. `restoreSession` ждёт этой очереди: окно, пересозданное
         * посреди загрузки, иначе читало сессию до записи, адрес снаружи был
         * уже взят из интента, и читатель оставался на начальной странице.
         */
        val session: ExecutorService = Executors.newSingleThreadExecutor()
    }
}
