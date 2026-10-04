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
    }
}

rootProject.name = "brevier"
include(":app")
// Kotlin-компонент проверяющего сертификаты — исходником, а не AAR.
include(":rustls-platform-verifier")
