// Kotlin-компонент `rustls-platform-verifier`: ядро зовёт его по JNI, чтобы
// цепочку сертификатов проверил Android. Апстрим раздаёт его AAR-ом
// из maven-репозитория в ветке на GitHub, а F-Droid собирает только
// из исходников и чужих maven-репозиториев не пускает — поэтому исходник
// лежит здесь, как есть: `src/main` и лицензии — дословно из
// rustls/rustls-platform-verifier, тег v/0.7.1 (252e251), это компонент 0.2.0.
// Подъём: скопировать `android/rustls-platform-verifier/src/main` из тега,
// где `android-release-support/Cargo.toml` называет нужную версию,
// и поправить VERSION.
plugins {
    id("com.android.library")
    id("org.jetbrains.kotlin.android")
}

// Компонент обязан быть той же версии, что `rustls-platform-verifier-android`
// в Cargo.lock: несовместимая пара падает уже на телефоне. Подъём крейта
// без подъёма исходника останавливает сборку здесь.
val vendored: String = file("VERSION").readText().trim()
val locked: String = run {
    val lines = rootProject.projectDir.parentFile.resolve("Cargo.lock").readLines()
    val at = lines.indexOfFirst { it.trim() == "name = \"rustls-platform-verifier-android\"" }
    check(at >= 0) { "rustls-platform-verifier-android not found in Cargo.lock" }
    lines.drop(at + 1).first { it.trimStart().startsWith("version = ") }
        .substringAfter('"').substringBefore('"')
}
check(vendored == locked) {
    "android/rustls-platform-verifier is $vendored, but Cargo.lock has rustls-platform-verifier-android $locked: copy the component from upstream"
}

android {
    namespace = "org.rustls.platformverifier"
    compileSdk = 35

    defaultConfig {
        minSdk = 24
        // Исходник спрашивает `BuildConfig.TEST`: правда только в тестах апстрима.
        buildConfigField("boolean", "TEST", "false")
        consumerProguardFiles("consumer-rules.pro")
    }

    buildFeatures { buildConfig = true }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions { jvmTarget = "17" }
}
