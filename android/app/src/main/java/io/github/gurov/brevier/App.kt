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
        /**
         * Сеть и декодирование. Несколько потоков: картинки статьи идут
         * параллельно, и медленная не должна держать остальные. Страницы
         * и картинки делят один пул — вкладок на телефоне немного.
         */
        val work: ExecutorService = Executors.newFixedThreadPool(4)
    }
}
