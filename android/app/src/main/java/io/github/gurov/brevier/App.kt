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
