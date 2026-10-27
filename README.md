# Wakeword

Переключение локального слова активации Яндекс Станции: **Алиса / Яндекс / Ясмина**.
Для сервера остаётся Алиса, ответы — по-русски. Управление через CLI и JSON API;
пакет для YAM, без собственного UI и постоянно работающего сервиса.

Проверено на **Станции Мини 2 (`yandexmini_2`, aarch64)**. Нужны root-доступ,
записываемый `/system/vendor/quasar` и модели из прошивки либо их исходные бэкапы.
На стоковую колонку без доступа к файловой системе пакет не устанавливается.

## Установка через YAM

Скачайте или [соберите](CONTRIBUTING.md) `wakeword-cli.yap` и установите через YAM:

```sh
yam install ./wakeword-cli.yap
yam enable wakeword-cli
yam run wakeword-cli list
yam run wakeword-cli switch yasmina
```

В «Доме с Алисой» оставьте имя **Алиса**. `switch` кратко перезапускает quasar;
дождитесь завершения команды. Для возврата: `yam run wakeword-cli switch alisa`.
Если ваш YAM не передаёт аргументы binary-пакету, используйте CLI напрямую.

## CLI на колонке

Бинарник пакета: `/data/quazar/yam/data/plugins/wakeword-cli/wakeword`.
Ниже `wakeword` означает этот бинарник или его копию в `PATH`.

```sh
wakeword list                 # доступные имена, * — выбранное
wakeword switch yasmina       # выбрать и применить
wakeword switch yandex
wakeword switch alisa         # вернуть исходную Алису и проверку активации
wakeword status               # версии файлов и CRC32
wakeword version
```

Раздельное применение: `wakeword set yasmina`, затем `wakeword apply`.
`current` показывает сохранённый выбор; `set` сам по себе модель не меняет.

## Для своего UI

```sh
wakeword --json list
wakeword --json status
wakeword --json switch yasmina
```

В stdout — один JSON-ответ, в stderr — диагностика. [Контракт API и YAM HTTP](docs/API.md),
[Python-клиент CLI](examples/cli_client.py), [Python-клиент YAM](examples/yam_client.py).
Коды выхода: `0` — успех, `1` — ошибка операции, `2` — аргументы, `3` — занято.

## Что меняется

Плагин сохраняет оригиналы, заменяет файлы локального споттера и сохраняет ожидаемую
CRC32 кэша. Для Яндекса/Ясмины отключает повторную серверную проверку активации,
которая иначе отклоняет новое слово. При возврате на Алису восстанавливает оригиналы
и прежнюю настройку проверки. [Технические детали](docs/INTERNALS.md).

- Имя в ответе «меня зовут Алиса» не меняется.
- При новой серверной версии модели может понадобиться повторный `apply`.
- `status` проверяет файлы; голосовую активацию проверяют отдельно.
- Произвольные слова и обучение моделей — [roadmap](ROADMAP.md).
- YAM — основная проверенная интеграция. BetterYAM заявляет совместимость binary `.yap`,
  но запуск Wakeword через него и поддержка Mini 2 не проверены:
  [BetterYAM](https://github.com/Yandex-Station-Universal-Repo/BetterYAM).

[Участие в разработке](CONTRIBUTING.md) · [История версий](CHANGELOG.md)
