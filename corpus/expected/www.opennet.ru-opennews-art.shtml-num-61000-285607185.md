# Релиз Firefox 125

[Состоялся](https://www.mozilla.org/en-US/firefox/125.0.1/releasenotes/) релиз web-браузера [Firefox 125](https://www.mozilla.org/en-US/firefox/125.0.1/releasenotes/) и сформировано обновление [ветки](https://www.mozilla.org/en-US/firefox/organizations/all/) с длительным сроком поддержки - [115.10.0](https://www.mozilla.org/en-US/firefox/115.10.0/releasenotes/). Из-за наличия выявленных на поздней стадии проблем сборка 125.0 была [отменена](https://www.opennet.ru/opennews/art.shtml?num=61001), и в качестве релиза объявлен выпуск 125.0.1. На стадию [бета-тестирования](https://firefox.com/channel) [переведена](https://www.mozilla.org/en-US/firefox/126.0beta/releasenotes/) ветка Firefox 126, релиз которой намечен на 14 мая.

[Основные](https://www.mozilla.org/en-US/firefox/125.0/releasenotes/) [новшества](https://developer.mozilla.org/en-US/docs/Mozilla/Firefox/Releases/125) в [Firefox 125](<https://bugzilla.mozilla.org/buglist.cgi?query_format=advanced&resolution=FIXED&target_milestone=125%20Branch&limit=0&short_desc_type=anywords&short_desc=Allow Enable Add Drop Support Implement>):

- Во встроенном PDF-просмотрщике включена по умолчанию функция выделения текста выбранным цветом и рамкой.

  [![](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713288554.png "")](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713288543.png)

- На странице Firefox View, упрощающей доступ к ранее просматриваемому содержимому, в секции с открытыми вкладками обеспечено отображение закреплённых вкладок и добавлена поддержка индикаторов состояния, например, дающих понять, что в определённой вкладке воспроизводится звук или видео, а также позволяющих через нажатие на индикатор отключить или вернуть звук. Аналогичные индикаторы также добавлены для закладок и уведомлений.

  [![](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713288584.png "")](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713288573.png)

- Реализована возможность быстрого перехода по ссылке, сохранённой в буфере обмена. Если во время нажатия на адресную строку в буфере обмена находится URL, автоматически данный URL будет показан в качестве начальной рекомендации для перехода.

  [![](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713288619.png "")](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713288611.png)

- Добавлена поддержка воспроизведения защищённого контента (EME, Encrypted Media Extensions) с использованием кодека AV1, который используется некоторыми провайдерами потокового вещания для распространения контента более высокого качества.
- При заполнении адресов в web-формах обеспечен вывод запроса на сохранение адреса (пока только для пользователей из США и Канады). В дальнейшем планируется использовать сохранённые данные для автозаполнения адресов.
- Включена блокировка загрузки файлов с URL, которые находятся в списках потенциально опасного контента.
- На системах, в которых используются дополнения с реализацией контейнеров вкладок, [добавлена](https://bugzilla.mozilla.org/show_bug.cgi?id=1882815) поддержка поиска из адресной строки вкладок, размещённых в разных контейнерах.
- В настройки добавлена опция, позволяющая автоматически определять наличие прокси при помощи протокола WPAD (Web Proxy Auto-Discovery ), несмотря на наличие включённых настроек для подключения через системный прокси.
- Изменено поведение обхода кнопок-переключателей (radio buttons) - если ни одна опция в кнопках не выбрана, то нажатие клавиши табуляции теперь активирует фокус только на первой опции, а следующее нажатие переводит фокус ввода на другой элемент, а не циклично перебирает все варианты опций. При этом клавиши со стрелками по-прежнему позволяют перемещаться по опциям одного элемента.
- Добавлена поддержка атрибута [popover](https://html.spec.whatwg.org/#the-popover-attribute), позволяющего создавать элементы, показываемые поверх других элементов web-интерфейса. Например, при помощи нового атрибута можно создавать меню действий, выводить подсказки для заполнения форм, создавать обучающие интерфейсы и реализовать захват содержимого. В отличие от элемента "dialog" элементы с атрибутом "popover" не используют модальный режим, поддерживают события и легко отменяются. Местоположение, каскадирование и фокус ввода выбираются и обрабатываются автоматически.
- В WebAssembly по умолчанию включён режим "multi-memory", позволяющий wasm-модулям использовать и импортировать несколько независимых линейных области памяти.
- В JavaScript добавлена поддержка сегментирования Unicode-текста (Unicode Text Segmentation), реализованная при помощи объекта [Intl.Segmenter](https://developer.mozilla.org/en-US/docs/Web/JavaScript/Reference/Global_Objects/Intl/Segmenter). Объект позволяет точно сегментировать текст в строке с учётом локали, например, для разделения слов в языках не использующих пробел для разделения слов.
- В реализацию интерфейсов HTMLCanvasElement и OffscreenCanvas добавлена поддержка событий [ContextLost](https://developer.mozilla.org/docs/Web/API/HTMLCanvasElement/contextlost_event) и [ContextRestored](https://developer.mozilla.org/docs/Web/API/HTMLCanvasElement/contextrestored_event), позволяющих обработать в пользовательском коде ситуации потери и восстановления контекста при аппаратном ускорении отрисовки.
- Включена поддержка метода [navigator.clipboard.readText()](https://developer.mozilla.org/docs/Web/API/Clipboard_API) для чтения из буфера обмена c запросом подтверждения операции (после вызова API пользователю показывается контекстное меню вставки для подтверждения действия).
- В CSS-свойство "[transform-box](https://developer.mozilla.org/docs/Web/CSS/transform-box)" добавлена поддержка значений [stroke-box](https://developer.mozilla.org/docs/Web/CSS/transform-box#stroke-box) и [content-box](https://developer.mozilla.org/docs/Web/CSS/transform-box#content-box), позволяющих изменить метод вычисления эталонной области для операций трансформации, например, для реализации расширенных графических эффектов.
- В CSS-свойстве "align-content" [реализована](https://developer.mozilla.org/docs/Web/CSS/CSS_box_alignment/Box_alignment_in_block_abspos_tables) возможность работы с блочными контейнерами. Например "display: block" и "display: list-item" теперь могут быть выровнены при помощи "align-content" без использования контейнеров flex и grid.
- Прекращена поддержка метода SVGAElement.text, вместо которого рекомендуется использовать более широко распространённый метод SVGAElement.textContent.
- В инструментах для web-разработчиков в нижней части панели отладчика реализовано новое выпадающее меню с действиями, связанными с Source Map. В about:config возвращена настройка "devtools.debugger.features.overlay" для отключения выводимого поверх контента индикатора приостановки выполнения JavaScript-кода отладчиком (Pause Debugger Overlay).

  [![](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713286819.png "")](https://www.mozilla.org/media/img/firefox/releasenotes/note-images/125_devtools_sourcemaps_menu.png)

- В [версии для Android](https://www.mozilla.org/en-US/firefox/android/125.0/releasenotes/) вкладки Custom Tabs, открываемые внутри других приложений, теперь учитывают выбор настройки темы оформления (тёмная тема, светлая тема и системный выбор темы). Улучшено меню с настройками логинов и паролей. Скрыта кнопка "Open in App", если Firefox выбран в качестве системного просмотрщика PDF.

Кроме новшеств и исправления ошибок в Firefox 125 устранено [18 уязвимостей](https://www.mozilla.org/security/advisories/mfsa2024-18/) (12 помечены как опасные). 11 уязвимостей (4 собраны под CVE-2024-3865) вызваны проблемами работы с памятью, такими как переполнения буферов и обращение к уже освобождённым областям памяти. Потенциально данные проблемы способны привести к выполнению кода злоумышленника при открытии специально оформленных страниц.

В бета-версии [Firefox 126](https://www.mozilla.org/en-US/firefox/126.0beta/releasenotes/) предложен новый упрощённый и унифицированный диалог для очистки данных пользователя, в котором улучшено разделение данных на категории и добавлены сведения о размере данных, сохранённых за выбранный промежуток времени.

[![](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713289550.png "")](https://www.opennet.ru/opennews/pics_base/CFD0C5CECEC5D4_1713289507.png)

1. [Главная ссылка к новости (https://www.mozilla.org/en-US/...)](https://www.mozilla.org/en-US/firefox/125.0.1/releasenotes/)
2. [OpenNews: Релиз Firefox 124](https://www.opennet.ru/opennews/art.shtml?num=60811)
3. [OpenNews: Во вкладках Firefox появится функция предпросмотра эскиза сайта](https://www.opennet.ru/opennews/art.shtml?num=60589)
4. [OpenNews: В Firefox появится группировка вкладок](https://www.opennet.ru/opennews/art.shtml?num=60759)
5. [OpenNews: Планы в отношении поддержки в Firefox второй и третьей версий манифеста Chrome](https://www.opennet.ru/opennews/art.shtml?num=60786)
6. [OpenNews: В Firefox добавлена поддержка машинного перевода выделенных фрагментов текста](https://www.opennet.ru/opennews/art.shtml?num=60956)

Лицензия: [CC BY 3.0](https://creativecommons.org/licenses/by/3.0/) Короткая ссылка: https://opennet.ru/61000-firefox Ключевые слова: [firefox](https://www.opennet.ru/keywords/firefox.html)

- [1.1](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#1), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (1), 20:11, 16/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=1&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#1 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

+3 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> Включена блокировка загрузки файлов с URL, которые находятся в списках потенциально опасного контента.

Как отключить?

- [2.2](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#2), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (2), 20:14, 16/04/2024 \[[^](#1 "к родителю")\] \[[^^](#1 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=2&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

возможно browser.safebrowsing.downloads.remote.enabled

- [3.32](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#32), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (32), 22:06, 16/04/2024 \[[^](#2 "к родителю")\] \[[^^](#1 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=32&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

И еще обнулить browser.safebrowsing.downloads.remote.url

- [**3.103**](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#103), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (103), 10:55, 22/04/2024 \[[^](#2 "к родителю")\] \[[^^](#1 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=103&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

FYI эта настройка отключает не саму блокировку опасных URL при копировании, а сам список опасных URLs, т.е. перестанет работать не только защита при копировании, но и при перехода не опасный сайт. К слову вероятность что в список "опасных" попадет что-то полезное низкая, так что можно не замарачиваться. И сам список опасных загружается онлайн и используется оффлайн

- [2.66](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#66), [rvs2016](https://www.opennet.ru/~rvs2016) (ok), 11:01, 17/04/2024 \[[^](#1 "к родителю")\] \[[^^](#1 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=66&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/cf324929e38a284fc52c6fa3b7b077a7.jpg)](https://www.opennet.ru/~rvs2016)\>> Включена блокировка загрузки файлов с URL,
\>> которые находятся в списках потенциально
\>> опасного контента.
\> Как отключить?

А он, кстати, загрузку "блокирует" как?
1\. Говорит типа я загружать не хочу, но если Вы не смотря на это загружать всё-равно хотите, то так и быть загружу \[загрузить\]
или
2\. Говорит типа я загружать не хочу и не буду это делать, даже если Вы на этом захотели бы настаивать?

- [2.88](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#88), [Bob](https://www.opennet.ru/~Bob) (??), 08:35, 18/04/2024 \[[^](#1 "к родителю")\] \[[^^](#1 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=88&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

А зачем? В новом обновлении уберут с about:config, как всегда.

- [1.3](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#3), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (3), 20:15, 16/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=3&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#3 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

–32 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Единственное, в чем превосходит хром -- удобные средства просмотра accessibility tree. Во всем остальном довольно убогий браузер. Но за a11y tree жирный плюс. А так двоечка.

Финальная оценка: двоечка с жирным плюсом.

- [2.5](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#5), [anonu](https://www.opennet.ru/~anonu) (?), 20:29, 16/04/2024 \[[^](#3 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=5&news_key=61000)\]

+11 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

напомнить о внедрении манифеста в "лудщый бравузер на свете"? на самом деле который ничего кроме дерьма из себя не представляет, в котором не особо стараются добавить кастомизируемость (на протяжении ВСЕЙ ЕГО ИСТОРИИ), напомню что в хром только в примерно 20 версии завезли распечатку страниц, лол!

да, и конечно спрошу, а почему ТОР браузер не на хромеоговне собирается?

ещё вопросы^W утверждения о том что FX не браузер а дерьмо?

- [3.7](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#7), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (3), 20:40, 16/04/2024 \[[^](#5 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=7&news_key=61000)\]

–6 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Напомнить, почему это не проблема 0 прямо сейчас v2 доступен 1 корпы не смог... большой текст свёрнут, [показать](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=show_thread&om=133431&forum=vsluhforumID3&omm=7)

- [4.12](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#12), [anonu](https://www.opennet.ru/~anonu) (?), 20:48, 16/04/2024 \[[^](#7 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=12&news_key=61000)\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> Кастомизируемость чего именно тебе не хватает?

например поддержка TRR, возможность отключения всего рекламного дерьма и телеметрии (ну, как в FX), недостаточно?

\> А еще пока файрфокс успешно осваивал рынок и одолевал IE, хромиума даже в проекте не было. И что? Мы в 2024 году, а в 2024 году хромиум уже давно не в версии 20.

я тебе о причине неприязне, ты мне о шишках, логика - первый сорт!

\> файрфокс успешно осваивал рынок и одолевал IE

ты хоть раз ИЕ запускал? я за 20 лет работы на винде ни разу это дерьмо не запускал, и ничё - жив, вроде
даже не встречал сайтов которые ие-онли

- [5.18](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#18), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (3), 21:04, 16/04/2024 \[[^](#12 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=18&news_key=61000)\]

–3 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Про 127 0 0 1 53 слыхал Нахрена вообще собственный DoH-клиент в бравзере Тот ... большой текст свёрнут, [показать](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=show_thread&om=133431&forum=vsluhforumID3&omm=18)

- [6.24](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#24), [anonu](https://www.opennet.ru/~anonu) (?), 21:24, 16/04/2024 \[[^](#18 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=24&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> Кстати, в файрфоксе это все отключается настолько неочевидно, что плодятся проекты вроде librefox/waterfox.

создаёшь папку distribution рядом с firefox.exe, внутри создаёшь policies.json и в него всю конфигурацию ([https://mozilla.github.io/policy-templates/](https://mozilla.github.io/policy-templates/)) пишешь, ещё неочевидные проблемы есть?

- [7.27](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#27), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (3), 21:32, 16/04/2024 \[[^](#24 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=27&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> создаёшь папку distribution рядом с firefox.exe, внутри создаёшь policies.json

Слишком много телодвижений, чтобы получить прайваси в бравзире, чей рекламный слоган -- "прайваси-фёрст бравзир". Обычный пользователь этого прайваси в итоге не получит, пока не позовет "компьютерного мастера васю" из объявления в падике.

- [8.29](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#29), [anonu](https://www.opennet.ru/~anonu) (?), 21:38, 16/04/2024 \[[^](#27 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=29&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

ничего не мешает самому всё это настроить, тут даже интелектом запредельным совс... текст свёрнут, [показать](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=show_thread&om=133431&forum=vsluhforumID3&omm=29)

- [8.30](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#30), [anonu](https://www.opennet.ru/~anonu) (?), 21:41, 16/04/2024 \[[^](#27 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=30&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

а как же тогда about config если не сумел в policy-templates privacy first, бла... текст свёрнут, [показать](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=show_thread&om=133431&forum=vsluhforumID3&omm=30)

- [9.31](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#31), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (31), 22:00, 16/04/2024 \[[^](#30 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=31&news_key=61000)\]

+5 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Проблема в том, что мазиллушка нигде не предупреждает, что браузер по дефолту ни... текст свёрнут, [показать](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=show_thread&om=133431&forum=vsluhforumID3&omm=31)

- [10.73](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#73), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (81), 15:48, 17/04/2024 \[[^](#31 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=73&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

По умолчанию firefox гораздо более приватный чем chrome Значит реклама огнелиса... текст свёрнут, [показать](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=show_thread&om=133431&forum=vsluhforumID3&omm=73)

- [10.74](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#74), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (81), 15:52, 17/04/2024 \[[^](#31 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=74&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Ну и главный фатальный недостаток chrome то что он проприетарный, а значит ни о ... текст свёрнут, [показать](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=show_thread&om=133431&forum=vsluhforumID3&omm=74)

- [6.25](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#25), [anonu](https://www.opennet.ru/~anonu) (?), 21:27, 16/04/2024 \[[^](#18 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=25&news_key=61000)\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> Про 127.0.0.1:53 слыхал? Нахрена вообще собственный DoH-клиент в бравзере? (Тот же вопрос можно адресовать хрому.)

потому что нормальный DNS умеет блокировать хосты по маскам (в отличии виндового), вот почему!

- [6.49](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#49), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (49), 00:19, 17/04/2024 \[[^](#18 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=49&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> Например, недостаточно быстро прикрутили поддержку \<dialog>

Это ты еще мягко сказал. В Хроме с 2014, а в Лисе только с 2022! Мда.

- [4.34](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#34), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (32), 22:13, 16/04/2024 \[[^](#7 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=34&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\>Кастомизируемость чего именно тебе не хватает?

Поделись, пожалуйста, как убрать из тулбара кнопку включения боковой панели.
Я ни когда не пользуюсь боковой панелью и на самом видном месте передо мной торчит кнопка, которую я ни когда не нажимаю.
Буду премного благодарен.

- [4.37](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#37), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (32), 22:42, 16/04/2024 \[[^](#7 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=37&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\>Мы в 2024 году, а в 2024 году хромиум уже давно не в версии 20

Напомнить как в 2024 году в хромиуме настраивается прокси?

- [5.59](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#59), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (59), 08:18, 17/04/2024 \[[^](#37 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=59&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

По дефолту берётся общесистемный, если он тебе не нравится - ставишь подходящий аддон.

- [2.8](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#8), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (8), 20:42, 16/04/2024 \[[^](#3 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=8&news_key=61000)\]

+6 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> Единственное, в чем превосходит хром

это там, где uBlock всё?

- [2.62](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#62), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (62), 09:56, 17/04/2024 \[[^](#3 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=62&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\>Единственное, в чем превосходит хром

Хром был хорош только в одном, на релизе у него была очень красивая иконка, превосходившая остальные браузеры. Больше ничего хорошего в хроме не было, и с тех пор даже иконка скурвилась.

- [3.72](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#72), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (72), 13:17, 17/04/2024 \[[^](#62 "к родителю")\] \[[^^](#3 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=72&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

У FF тож, кстати, скурвилась. Раньше красивая детализированная панда вокруг глобуса была, а щас какая-то шляпа

- [1.4](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#4), [rvs2016](https://www.opennet.ru/~rvs2016) (ok), 20:26, 16/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=4&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#4 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/cf324929e38a284fc52c6fa3b7b077a7.jpg)](https://www.opennet.ru/~rvs2016)\> Включена блокировка загрузки файлов с URL,
\> которые находятся в списках потенциально
\> опасного контента.

Чё - наваяли систему индульгенций что ли?
Кто индульгенцию купит, того в списки опасного контента не внесут? :-)

- [2.41](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#41), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (41), 23:39, 16/04/2024 \[[^](#4 "к родителю")\] \[[^^](#4 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=41&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Нет, там всё проще.

Уже не первый год блокируется загрузка файлов по HTTP с HTTPS-страниц.

Теперь будет блокироваться загрузка файлов по HTTP во всех случаях. Но у пользователя, как и раньше, есть возможность выбрать принудительную загрузку.

- [3.26](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#26), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (26), 21:29, 16/04/2024 \[[^](#9 "к родителю")\] \[[^^](#6 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=26&news_key=61000)\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

А чем он плох? Я туда статьи интересные сохраняю, потом читаю. Есть синхронизация с android, eink-читалкой. Очень удобно.

- [4.28](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#28), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (28), 21:33, 16/04/2024 \[[^](#26 "к родителю")\] \[[^^](#6 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=28&news_key=61000)\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Тем что в этом кармане дырка, уже много раз были новости

- [1.13](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#13), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (13), 20:51, 16/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=13&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#13 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> Добавлена поддержка атрибута popover, позволяющего создавать элементы, показываемые поверх других элементов web-интерфейса.

Это ещё позволяет их показывать без скриптов, голым HTML. Например, можно использовать для мобильного меню.

- [1.15](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#15), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (15), 21:00, 16/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=15&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#15 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

\> Добавлена поддержка воспроизведения защищённого контента

Опять всё испортили, ведь всем же известно, что самый защищённый контент - это контент который не воспроизводится

- [1.36](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#36), [YetAnotherOnanym](https://www.opennet.ru/~YetAnotherOnanym) (ok), 22:35, 16/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=36&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#36 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

+5 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/e76ba89a5ed1bc4c42f914f1210fb028.jpg)](https://www.opennet.ru/~YetAnotherOnanym)\> Если во время нажатия на адресую строку в буфере обмена находится URL, автоматически данный URL будет показан в качестве начальной рекомендации для перехода.

Как же они достали делать то, что их не просят и совать нос куда не нужно.

- [2.45](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#45), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (41), 23:44, 16/04/2024 \[[^](#36 "к родителю")\] \[[^^](#36 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=45&news_key=61000)\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Очень удобная штука с тачпада, когда мыши нет. Получается чуть быстрее, чем добираться до пункта контекстного меню "вставить и отправить".

- [4.51](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#51), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (41), 01:42, 17/04/2024 \[[^](#50 "к родителю")\] \[[^^](#36 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=51&news_key=61000)\]

–1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Про математику слышал?

Посчитай, сколько кликов занимает твой вариант (3: кликнуть в адресную строку, нажать Ctrl+V, нажать Enter), а сколько мой (2: кликнуть в адресную строку, нажать подсказку)

- [5.54](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#54), [dchusovitin](https://www.opennet.ru/~dchusovitin) (ok), 05:00, 17/04/2024 \[[^](#51 "к родителю")\] \[[^^](#36 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=54&news_key=61000)\]

+3 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/028457c410c68d9f2ccf22f5805b0fce.jpg)](https://www.opennet.ru/~dchusovitin)Сделать фокус на адресной строке можно через CTLR+L, тогда не нужна мышка, что удобно.
Итого - CTRL-L - CTRL-V (либо стрелка вниз, в этой версии) + Enter

- [6.75](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#75), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (81), 16:00, 17/04/2024 \[[^](#54 "к родителю")\] \[[^^](#36 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=75&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Почему вы решили что все должны пользоваться клавиатурой и запоминать горячие клавиши на каждый чих, нравится пользуйтесь, но не надо выставлять это как единственно верный способ взаимодействия с программой.

- [5.60](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#60), [YetAnotherOnanym](https://www.opennet.ru/~YetAnotherOnanym) (ok), 09:19, 17/04/2024 \[[^](#51 "к родителю")\] \[[^^](#36 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=60&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/e76ba89a5ed1bc4c42f914f1210fb028.jpg)](https://www.opennet.ru/~YetAnotherOnanym)Три телодвижения, доведённые до автоматизма, выполняются намного легче, чем два, если после каждого надо переключать мозг на вопрос "а что дальше?".

- [6.85](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#85), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (41), 20:04, 17/04/2024 \[[^](#60 "к родителю")\] \[[^^](#36 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=85&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Так выполняйте их за меня, раз вам легче. Я не возражаю.

Об том вся эта ветка: я сразу сказал, что мне проще выполнять телодвижения именно новым способом, но тут же набежали любители поуказывать, как я должен телодвигаться по их мнению.

- [5.70](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#70), [Ананас](https://www.opennet.ru/~%E1%CE%C1%CE%C1%D3) (?), 11:58, 17/04/2024 \[[^](#51 "к родителю")\] \[[^^](#36 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=70&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Раньше был самый скоростной вариант - даблклик на тексте, мидлклик в адресной строке. Потом мозиловцы сломали это автовыделением текста в адресной строке по фокусу - буфер перетирался этим текстом. Чинить отказывались - мол нинужна. А теперь вот сделали эту менее удобную хрень. Клоунада какая-то.

- [4.67](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#67), [rvs2016](https://www.opennet.ru/~rvs2016) (ok), 11:06, 17/04/2024 \[[^](#50 "к родителю")\] \[[^^](#36 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=67&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/cf324929e38a284fc52c6fa3b7b077a7.jpg)](https://www.opennet.ru/~rvs2016)\> Про копировать-вставить хоткеями слышал?

На мобилах пока умолчательную клавиатуру не заменишь нормальной типа Hackers keyboard, так там никаких клавиш типа Ctrl (для хоткеев Ctrl+C да Ctrl+V) не будет. :-)
Ну там на мобилах есть всякие странные штуки типа щелчок по полю ввода, ещё шелчок, выбор в взлетающем меню пункта "вставить". Но это всё дольше, чем хоткеи божеские.

- [1.44](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#44), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (44), 23:43, 16/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=44&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#44 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

+4 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

М-да... заголовок на четверть экрана на скриншотах - это, конечно, огонь!

- [1.47](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#47), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (47), 00:17, 17/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=47&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#47 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

А как у него с производительностью?
Последний раз когда щупал, было не очень
Хз, мб сборка такая просто была

- [2.48](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#48), [th3m3](https://www.opennet.ru/~th3m3) (ok), 00:18, 17/04/2024 \[[^](#47 "к родителю")\] \[[^^](#47 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=48&news_key=61000)\]

–1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/9a49945d2d11ed745ea5de2d54e41352.jpg)](https://www.opennet.ru/~th3m3)На данный момент, самый быстрый браузер.

- [3.53](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#53), [tcpip](https://www.opennet.ru/~tcpip) (??), 04:58, 17/04/2024 \[[^](#48 "к родителю")\] \[[^^](#47 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=53&news_key=61000)\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Не смеши мои тапочки (пишу с Google Chrome).

- [4.69](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#69), [th3m3](https://www.opennet.ru/~th3m3) (ok), 11:40, 17/04/2024 \[[^](#53 "к родителю")\] \[[^^](#47 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=69&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/9a49945d2d11ed745ea5de2d54e41352.jpg)](https://www.opennet.ru/~th3m3)\> Не смеши мои тапочки (пишу с Google Chrome).

Пиши хоть с табуретки. И погугли новости, как Firefox остаётся самым быстрым браузером в мире.

- [5.86](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#86), [\_kp](https://www.opennet.ru/~_kp) (ok), 20:12, 17/04/2024 \[[^](#69 "к родителю")\] \[[^^](#47 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=86&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/2e27e688755b7cdde87359e8c8cc1e38.jpg)](https://www.opennet.ru/~_kp)Не так давно была тема про тест скорости браузеров. Там были и замеры, и сравнения.

Но занятно, что несмотря что на разном железе разрыв результатов теста оличался в ДЕСЯТКИ раз, это практически ни как не влияло на кофорт и скорость открытия страниц.
Я несколько результатов с разного железа отправил, и по секундомеру смотрел как сайты потяжелее открываются.
И вывод, результаты тестов скорости не оказывают практически никакого влияния, да и и скорость железа, если не брать совсем хлам, тоже особо не влияет.

Но пользователи замечают субъективную разницу в быстродействии, подлагивание браузера, удобные или неудобные мелочи.
Так же, браузера без adblock в реальной жизни не бывает, а с ним Сафари например уверенно скатился на дно.
Любимые плагины, или их отсутсвие, тоже оказывают влияние на предпочтение браузера.

По моим субективным ощущениям Chrome быстрее, и на Мак( в том числе и на intel) быстрее, чем на других ОС.
Firefox использую, регулярно, но как дополнительный браузер, ибо он постоянно забывает последний путь сохранения файлов, и багу этому более десятка лет, как и бесполезным советам по этому вопросу.

- [4.77](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#77), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (81), 16:42, 17/04/2024 \[[^](#53 "к родителю")\] \[[^^](#47 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=77&news_key=61000)\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Гугл хром и раньше отставал на тяжелы сайтах от Файрфоксе, а теперь и официально по результатам бенчмарка файрфокс стал первым.

- [2.76](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#76), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (81), 16:39, 17/04/2024 \[[^](#47 "к родителю")\] \[[^^](#47 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=76&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Велась работа по ускорению и он обогнал Chrome.

- [4.83](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#83), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (82), 19:52, 17/04/2024 \[[^](#82 "к родителю")\] \[[^^](#47 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=83&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

А ещё, в ff отличная плавная прокрутка, после которой прокрутка в хромах режет глаз

- [5.84](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#84), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (82), 19:53, 17/04/2024 \[[^](#83 "к родителю")\] \[[^^](#47 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=84&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

А ещё, есть конфиг, который все ещё работает, и в нем достаточно много чего можно поменять

- [1.52](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#52), [cheburnator9000](https://www.opennet.ru/~cheburnator9000) (ok), 03:39, 17/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=52&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#52 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

+1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/af1f739f42c77366fdb0de1f37d4ff1f.jpg)](https://www.opennet.ru/~cheburnator9000)Я глазам своим не верю, но в недавних версиях они наконец-то исправили вот это безобразие когда в истории фильтруешь по search? (страницы результатов поиска в гугле) затем CTRL+A и удалить. На тысячи элементах firefox раньше зависал к едрене фене, 100% загрузка одного ядра, можно было нажать на кнопку очистить поле фильтра, секунд через 10-20 оно одуплялось, но в фоне sqlite база меееедлено чистилась. Я как не специалист по коду firefox сразу понял что проблема в коде фильтра поиска по истории, точнее в той модели отображения данных из sqlite базы, но разработчикам на это потребовалось сколько? 10 лет? если не больше.

- [1.55](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#55), [нитгитлистер](https://www.opennet.ru/~%CE%C9%D4%C7%C9%D4%CC%C9%D3%D4%C5%D2) (?), 06:03, 17/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=55&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#55 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

–1 [+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

читаю описание новшеств и коммены и диву даюсь на сколько же всё еще сырой и не доработанный этот браузер... в общем то так понимаю им пользуются те, кто не любит скучать, ну да у каждого свои развлечения)

- [2.80](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#80), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (81), 19:18, 17/04/2024 \[[^](#55 "к родителю")\] \[[^^](#55 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=80&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

В комментах сплошные троли и хейтеры которые этим браузером не пользуются, я много лет им пользуюсь, последние годы вообще использую ночные сборки и проблем никаких нет при повседневном использовании.

- [2.68](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#68), [rvs2016](https://www.opennet.ru/~rvs2016) (ok), 11:26, 17/04/2024 \[[^](#56 "к родителю")\] \[[^^](#56 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=68&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/cf324929e38a284fc52c6fa3b7b077a7.jpg)](https://www.opennet.ru/~rvs2016)\> браузер Nyxt во тема.

\# pkgs nyxt
Ищем nyxt через pkg:
pkg не нашёл, ищем в портах через port-find:
port-find ничего не нашёл тоже!

А из совсем исходников, которые даже не через порты, компилировать была халва. Время студенческого безделия закончилось почти четверть века назад уж.

- [1.58](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#58), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (57), 08:01, 17/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=58&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#58 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Юзабилити не улучшили?

Поиск по истории/закладкам всё ещё плохой?

Stop нормально не работает?

URL по прежнему теряется то там, то здесь?

- [1.87](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#87), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (87), 21:03, 17/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=87&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#87 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Когда сделаю нормальный менеджер сессий. Все какие-томоелкие рбшечки, а такой глобальной вещи толком нет. Старый сломали...

- [1.89](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#89), [semester](https://www.opennet.ru/~semester) (ok), 12:23, 18/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=89&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#89 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/34a17a1bba88d476a7109e7c05e38d8e.jpg)](https://www.opennet.ru/~semester)Подскажите как то можно настроить прокрутку. Плавность реально хуже чем в хроме. У меня kde на арче с вейландом. Встройка 780M. Firefox работает под вейландорм, а хром в режиме x-wayland. Но в хроме плавно, в фаерфокс прокрутка дерганная

- [2.90](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#90), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (90), 13:52, 18/04/2024 \[[^](#89 "к родителю")\] \[[^^](#89 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=90&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

about:config , много настроек, содержащих "smoothScroll".

- [1.91](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#91), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (91), 14:14, 18/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=91&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#91 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

duckduckgo=Firefox+как+подписать+расширение

developer.mozilla.org/ru/docs/Mozilla/Расширения и темы необходимо отправить на подпись Mozilla, прежде чем их можно будет установить в Firefox.

— подписать расширение на сайте Mozilla и установить в браузер. Для этого вам понадобится потратить несколько минут на создание учётной записи на сайте addons.mozilla.org, но зато расширением можно будет пользоваться.

На данный момент нет никакой гарантии, что нужное вам дополнение сразу заработает."

- [2.92](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#92), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (91), 14:17, 18/04/2024 \[[^](#91 "к родителю")\] \[[^^](#91 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=92&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Когда расширение браузера отправляется на подпись, оно подлежит автоматической проверке. Он также может подлежать проверке вручную, если в результате автоматической проверки будет установлено, что проверка вручную необходима. Расширение вашего браузера не будет подписано до тех пор, пока оно не пройдет автоматическую проверку, и его подпись может быть отозвана, если оно не пройдет проверку вручную. Процесс проверки следует строгим правилам, поэтому его легко проверить и избежать возможных проблем с проверкой."

Мордор какой-то.

- [1.93](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#93), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (93), 15:01, 18/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=93&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#93 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

как отключить самую верхнюю полоску, которой нет в хроме, я про то что добавляет gtk или qt, там где закрыть, распахнуть, свернуть, у хромого это встроенное и лишнее место не отъедает

- [2.95](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#95), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (95), 20:54, 18/04/2024 \[[^](#93 "к родителю")\] \[[^^](#93 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=95&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Правый клик на гамбургер-меню, customize toolbar, выключи чекбокс Title bar.

- [3.96](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#96), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (93), 21:11, 18/04/2024 \[[^](#95 "к родителю")\] \[[^^](#93 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=96&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

я не про панель меню, а про системное меню оконного менеджера, которое почти у каждого запускаемого приложения, в хромом его как то побороли

- [**4.99**](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#99), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (95), 00:20, 20/04/2024 \[[^](#96 "к родителю")\] \[[^^](#93 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=99&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

С панелью меню у меня скорее ассоциируется то, что вызывается по alt. Его я нигде не видел видимым вне клика alt по-умолчанию.
Тайтлбар (системный заголовок окна, в котором также 3 классические кнопки) в KDE и Gnome может быть виден по-умолчанию и скрывается той галкой в настройках. На винде он по-умолчанию скрыт. В итоге 3 кнопки управления окна совмещены с таб-баром с вкладками и ничего не съедает место.
Не знаю о чем ты, покопайся в DE/WM.

- [**2.101**](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#101), [Аноним](https://www.opennet.ru/~%E1%CE%CF%CE%C9%CD) (101), 00:46, 20/04/2024 \[[^](#93 "к родителю")\] \[[^^](#93 "на 1 уровень")\] \[[^^^](#lenta_nav "вверх")\] \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=101&news_key=61000)\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

В настройках тулбара (который под вкладками), жмешь на нем ПКМ - последний пункт, и внизу галочка Title Bar или Панель заголовка. Либо пункт browser.tabs.inTitlebar в about:config сбросить на 0. И по умолчанию она выключена, это ты ее сам включил.

- [1.98](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#98), [нейм](https://www.opennet.ru/~%CE%C5%CA%CD) (?), 08:46, 19/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=98&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#98 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

Ещё бы использовали системные диалоги, вместо уродских обрезок гномовских, было б хоть как-то лучше

- [**1.102**](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#102), [iZEN](https://www.opennet.ru/~iZEN) (ok), 07:51, 20/04/2024 \[[ответить](https://www.opennet.ru/cgi-bin/openforum/vsluhboard.cgi?az=post&om=133431&forum=vsluhforumID3&omm=102&news_key=61000)\] \[[﹢﹢﹢](https://www.opennet.ru/openforum/vsluhforumID3/133431.html#102 "Показать все нераскрытые сообщения в подветке")\] \[[ · · · ](# "Свернуть ветку")\]

[+](# "Полезно, одобряю")/[–](# "Мусорный комментарий")

[![](https://www.opennet.ru/avatar/f677e7c7d54e054c27d1060ea8824d1c.jpg)](https://www.opennet.ru/~iZEN)\===>>> pkg-message for firefox-125.0.2,2
On install: ## Missing features

Some features found on Windows, macOS and Linux are not implemented:

\- Encrypted Media Extensions (requires Widevine CDM binary)
\- Process sandboxing (requires Capsicum backend)
\- Reduced memory usage (requires mozjemalloc)
\- Crash Reporter (requires Google Breakpad and reproducible builds)
\- WebVR (requires open source runtime)
\- TCP fast open
\- 'about:networking#networkid' (requires link state notification)

\## Audio backend

Currently used audio backend can be inspected on 'about:support' page.
Supported backends and default probing order is as follows:
\- 'pulse-rust' if 'pulseaudio' package is installed (PULSEAUDIO option)
\- 'jack' if 'jackit' package is installed (JACK option)
\- 'sndio' if 'sndio' package is installed (SNDIO option)
\- 'alsa' if 'alsa-lib' package is installed (ALSA option)
\- 'oss' (always available)
To force a specific backend open 'about:config' page and create
\===>>> Upgrade of firefox-125.0.1,2 to firefox-125.0.2,2 complete

% freebsd-version
13.3-STABLE
