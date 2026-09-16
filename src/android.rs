//! Вход для Android: приложение получает ядро общей библиотекой, и снаружи
//! у ядра ровно две функции. Модуль собирается только под Android
//! (`cfg(target_os = "android")`), поэтому ни cli, ни окно о нём не знают —
//! тулкит и платформа по-прежнему сидят на краю.
//!
//! Первая функция обязательна и не про чтение: системный проверяющий
//! сертификаты живёт в JVM, и `rustls-platform-verifier` должен получить
//! контекст приложения **до** первого запроса. Без этого решение «доверие
//! делегируем ОС» на Android не работает вовсе — а обходить проверку мы
//! не станем и здесь.
//!
//! Имена символов — контракт с java-стороной: класс
//! `io.github.gurov.brevier.spike.Core` с двумя статическими нативными
//! методами, `init(Context)` и `read(String)`.

use jni::EnvUnowned;
use jni::errors::{Error as JniError, ThrowRuntimeExAndDefault};
use jni::objects::{JClass, JObject, JString};

use crate::fetch::UserAgent;

/// Отдать платформенному проверяющему контекст приложения и поставить
/// провайдер шифров. Зовётся один раз, из `Activity.onCreate`.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_gurov_brevier_spike_Core_init<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    context: JObject<'caller>,
) {
    let outcome = unowned.with_env(|env| -> Result<(), JniError> {
        rustls_platform_verifier::android::init_with_env(env, context)?;
        crate::init_crypto();
        Ok(())
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Прочитать адрес и вернуть markdown. Ходит в сеть, поэтому java-сторона
/// зовёт это не из главного потока.
#[unsafe(no_mangle)]
pub extern "system" fn Java_io_github_gurov_brevier_spike_Core_read<'caller>(
    mut unowned: EnvUnowned<'caller>,
    _class: JClass<'caller>,
    asked: JString<'caller>,
) -> JString<'caller> {
    let outcome = unowned.with_env(|env| -> Result<JString<'caller>, JniError> {
        let asked = asked.to_string();
        JString::from_str(env, read(&asked))
    });
    outcome.resolve::<ThrowRuntimeExAndDefault>()
}

/// Тот же тракт, что и у cli: разобрать адрес, открыть, отдать markdown.
/// Отказ объясняется теми же словами, что и в окне (`failure::describe`), —
/// тексты лежат в ядре именно затем, чтобы их не писать заново на каждой
/// платформе.
fn read(asked: &str) -> String {
    let opened =
        crate::address::parse(asked).and_then(|address| crate::open(&address, UserAgent::Honest));
    match opened {
        Ok(document) => document.markdown,
        Err(error) => {
            let failure = crate::failure::describe(&error);
            format!("# {}\n\n{}", failure.headline, failure.detail)
        }
    }
}
