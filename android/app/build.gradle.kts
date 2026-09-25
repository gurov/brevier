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

// Kotlin-компонент проверяющего сертификаты обязан быть той же версии, что
// `rustls-platform-verifier-android` в Cargo.lock: несовместимая пара падает
// уже на телефоне. Версию читаем из lock-файла — так советует апстрим, и так
// подъём крейта не требует правки здесь.
abstract class RustlsVersion : ValueSource<String, RustlsVersion.Params> {
    interface Params : ValueSourceParameters {
        val lockFile: RegularFileProperty
    }

    override fun obtain(): String {
        val lines = parameters.lockFile.get().asFile.readLines()
        val at = lines.indexOfFirst { it.trim() == "name = \"rustls-platform-verifier-android\"" }
        val version = if (at < 0) null else lines.drop(at + 1)
            .firstOrNull { it.trimStart().startsWith("version = ") }
            ?.substringAfter('"')
            ?.substringBefore('"')
        return version ?: error("rustls-platform-verifier-android not found in Cargo.lock")
    }
}

val verifierVersion = providers.of(RustlsVersion::class.java) {
    parameters.lockFile.set(repo.resolve("Cargo.lock"))
}

configurations.configureEach {
    resolutionStrategy.eachDependency {
        if (requested.group == "org.rustls" && requested.name == "rustls-platform-verifier") {
            useVersion(verifierVersion.get())
            because("the Kotlin component must match rustls-platform-verifier-android in Cargo.lock")
        }
    }
}

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

// Имя файла — то, что человек скачает: brevier-0.2.0-arm64.apk, а не
// app-release.apk. ABI в имени, когда она одна; отладочная сборка помечена.
fun apkName(buildType: String): String {
    val abi = when (abis) {
        listOf("arm64-v8a") -> "-arm64"
        listOf("x86_64") -> "-x86_64"
        else -> ""
    }
    val debug = if (buildType == "debug") "-debug" else ""
    return "brevier-$coreVersion$abi$debug.apk"
}

// Ядро собирает cargo, а не Gradle: это та же команда, что у канарейки в CI.
val buildCore by tasks.registering(Exec::class) {
    description = "Builds the Brevier core as a shared library for each ABI"
    workingDir = repo
    commandLine(listOf("sh", "android/core.sh", coreDir.get().asFile.path) + abis)
    inputs.dir(repo.resolve("src"))
    inputs.dir(repo.resolve("assets/hyph"))
    inputs.file(repo.resolve("Cargo.toml"))
    inputs.file(repo.resolve("Cargo.lock"))
    inputs.property("abis", abis)
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
        versionCode = coreVersion.split('.').fold(0) { code, part -> code * 100 + part.toInt() }
        ndk { abiFilters += abis }
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
            // Без ProGuard: Kotlin-компонент проверяющего зовут по JNI,
            // и минификатор счёл бы его мёртвым кодом.
            isMinifyEnabled = false
            signingConfig = signingConfigs.findByName("release")
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
    if (signing == null && allTasks.any { it.project == project && it.name.contains("Release") && it.name.startsWith("assemble") }) {
        throw GradleException("A release is signed: set BREVIER_SIGNING to a properties file with storeFile, storePassword and keyAlias")
    }
}

dependencies {
    // Версию подставляет правило выше — из Cargo.lock.
    implementation("org.rustls:rustls-platform-verifier")
}
