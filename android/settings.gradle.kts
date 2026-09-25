// Android-фронтенд Brevier: Kotlin поверх ядра. Ядро — та же библиотека,
// что у десктопа, собранная общей библиотекой под каждую ABI (`core.sh`).
pluginManagement {
    repositories {
        google()
        mavenCentral()
        gradlePluginPortal()
    }
}

dependencyResolutionManagement {
    repositoriesMode.set(RepositoriesMode.FAIL_ON_PROJECT_REPOS)
    repositories {
        google()
        mavenCentral()
        // Kotlin-компонент платформенного проверяющего сертификаты: апстрим
        // раздаёт его maven-репозиторием из ветки на GitHub. Только эта группа —
        // остальное из этого репозитория брать незачем.
        maven {
            url = uri("https://github.com/rustls/rustls-platform-verifier/raw/maven-archive/android-release-support/maven/")
            content { includeGroup("org.rustls") }
        }
    }
}

rootProject.name = "brevier"
include(":app")
