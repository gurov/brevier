# Компонент зовут из Rust по JNI, и R8 этих вызовов не видит: без правила
# он выбросил бы его как мёртвый код. Правило — дословно из README апстрима
# (раздел «Proguard»), едет к приложению вместе с модулем.
-keep, includedescriptorclasses class org.rustls.platformverifier.** { *; }
