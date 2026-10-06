import java.util.Properties

plugins {
    id("com.android.application")
    id("org.jetbrains.kotlin.android")
}

// Корень репозитория: ядро, его Cargo.lock и гарнитуры лежат там, а не здесь.
val repo: File = rootProject.projectDir.parentFile

// Версия приложения — версия ядра: у Brevier одна версия на все платформы.
val coreVersion: String = Regex("""(?m)^version\s*=\s*"([^"]+)"""")
    .find(repo.resolve("Cargo.toml").readText())
    ?.groupValues?.get(1)
    ?: error("no version in Cargo.toml")

// Номер версии для Android выводится из неё же (0.5.2 → 502), но записан
// числом: F-Droid узнаёт о выпуске, читая его из этого файла регэкспом,
// и выражение Gradle не вычислит. Gradle сверяет число с версией ниже.
val derivedVersionCode: Int = coreVersion.split('.').fold(0) { code, part -> code * 100 + part.toInt() }

// Какие ABI собирать: `-Pabis=x86_64` ускоряет сборку для эмулятора.
val abis: List<String> = (findProperty("abis") as String?)
    ?.split(',')?.map { it.trim() }?.filter { it.isNotEmpty() }
    ?: listOf("arm64-v8a", "x86_64")

val coreDir = layout.buildDirectory.dir("core")

// Подпись выпуска. Ключ в репозиторий не едет: путь к файлу свойств
// (`storeFile`, `storePassword`, `keyAlias`) называет переменная
// BREVIER_SIGNING — на машине мейнтейнера это ~/Android/keys, в CI файл
// собирается из секретов. Без неё выпуск не собирается вовсе: неподписанный
// APK не поставить, а подписанный отладочным ключом раздавать нельзя.
val signing: Properties? = System.getenv("BREVIER_SIGNING")
    ?.takeIf { it.isNotBlank() }
    ?.let { path -> Properties().apply { file(path).inputStream().use { load(it) } } }

// …кроме выпуска для F-Droid: тот собирает без подписи и подписывает сам —
// или, раз сборка воспроизводима, переносит на свой файл нашу подпись
// из Releases. Отсюда явное `-Punsigned`, и только оно.
val unsigned: Boolean = (findProperty("unsigned") as String?)?.let { it != "false" } ?: false

// Имя файла — то, что человек скачает: brevier-0.2.0-arm64.apk, а не
// app-release.apk. ABI в имени, когда она одна; отладочная сборка помечена.
fun apkName(buildType: String): String {
    val abi = when (abis) {
        listOf("arm64-v8a") -> "-arm64"
        listOf("x86_64") -> "-x86_64"
        else -> ""
    }
    val kind = when {
        buildType == "debug" -> "-debug"
        unsigned -> "-unsigned"
        else -> ""
    }
    return "brevier-$coreVersion$abi$kind.apk"
}

// Ядро собирает cargo, а не Gradle: это та же команда, что у канарейки в CI.
// NDK — тот, что назван `ndkVersion` ниже, а не первый попавшийся: на раннере
// GitHub ANDROID_NDK_HOME смотрит в свой, у F-Droid свой, и библиотека
// выходила бы разной.
val buildCore by tasks.registering(Exec::class) {
    description = "Builds the Brevier core as a shared library for each ABI"
    workingDir = repo
    environment("ANDROID_NDK_HOME", android.ndkDirectory.path)
    commandLine(listOf("sh", "android/core.sh", coreDir.get().asFile.path) + abis)
    inputs.dir(repo.resolve("src"))
    inputs.dir(repo.resolve("assets/hyph"))
    inputs.file(repo.resolve("Cargo.toml"))
    inputs.file(repo.resolve("Cargo.lock"))
    inputs.file(repo.resolve("android/core.sh"))
    inputs.file(repo.resolve("android/rust-toolchain.toml"))
    inputs.property("abis", abis)
    inputs.property("ndk", android.ndkVersion)
    outputs.dir(coreDir)
}

android {
    namespace = "io.github.gurov.brevier"
    compileSdk = 35

    defaultConfig {
        applicationId = "io.github.gurov.brevier"
        // API 24 — та же нижняя граница, что у канарейки в CI: ниже системный
        // проверяющий сертификаты не знает отзыва.
        minSdk = 24
        targetSdk = 35
        versionName = coreVersion
        versionCode = 600
        check(versionCode == derivedVersionCode) {
            "versionCode is $versionCode, but version $coreVersion means $derivedVersionCode: update it in android/app/build.gradle.kts"
        }
        ndk { abiFilters += abis }
    }

    // Тот же NDK в CI, у F-Droid и на машине мейнтейнера: иначе ядро
    // собирается разными компиляторами, и воспроизводимой сборки нет.
    ndkVersion = "29.0.14206865"

    // `core.sh` уже стрипает ядро тем же NDK; второй проход AGP лишний.
    packaging { jniLibs { keepDebugSymbols += "**/libbrevier.so" } }

    // Блок зависимостей для Google Play — шифрованный ключом Google, читать
    // его некому, кроме Play. F-Droid такой APK не принимает.
    dependenciesInfo {
        includeInApk = false
        includeInBundle = false
    }

    sourceSets["main"].apply {
        // Гарнитуры и их лицензия — те же файлы, что вшиты в десктоп: одна
        // типографика на обеих платформах, и OFL едет вместе со шрифтами.
        assets.srcDir(repo.resolve("assets/fonts"))
        jniLibs.srcDir(coreDir)
    }

    signingConfigs {
        if (signing != null) {
            create("release") {
                storeFile = file(signing.getProperty("storeFile"))
                storePassword = signing.getProperty("storePassword")
                keyAlias = signing.getProperty("keyAlias", "brevier")
                // PKCS12 держит один пароль на хранилище и ключ.
                keyPassword = signing.getProperty("keyPassword", storePassword)
            }
        }
    }

    buildTypes {
        release {
            // R8 выбрасывает неиспользуемый код (просьба ревьюера F-Droid).
            // Всё, что зовут по JNI, он не видит, — правила в proguard-rules.pro
            // и у модуля rustls-platform-verifier.
            isMinifyEnabled = true
            proguardFiles(getDefaultProguardFile("proguard-android-optimize.txt"), "proguard-rules.pro")
            signingConfig = if (unsigned) null else signingConfigs.findByName("release")
            // Корень Gradle — `android/`, а git — уровнем выше: AGP писал
            // в APK «NO_SUPPORTED_VCS_FOUND», то есть ничего полезного.
            vcsInfo { include = false }
        }
    }

    compileOptions {
        sourceCompatibility = JavaVersion.VERSION_17
        targetCompatibility = JavaVersion.VERSION_17
    }

    kotlinOptions { jvmTarget = "17" }
}

tasks.named("preBuild") { dependsOn(buildCore) }

@Suppress("DEPRECATION")
android.applicationVariants.all {
    val type = buildType.name
    outputs.all {
        (this as com.android.build.gradle.internal.api.BaseVariantOutputImpl).outputFileName = apkName(type)
    }
}

// Выпуск без ключа — сразу и словами, а не неподписанным файлом в конце.
gradle.taskGraph.whenReady {
    if (signing == null && !unsigned && allTasks.any { it.project == project && it.name.contains("Release") && it.name.startsWith("assemble") }) {
        throw GradleException("A release is signed: set BREVIER_SIGNING to a properties file with storeFile, storePassword and keyAlias (or pass -Punsigned for an unsigned build)")
    }
}

dependencies {
    // Версию с Cargo.lock сверяет сам модуль.
    implementation(project(":rustls-platform-verifier"))
}
