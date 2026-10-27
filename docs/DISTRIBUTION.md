# Распространение

## Установка через YAM по имени — уже сейчас

```sh
yam --registry https://raw.githubusercontent.com/nayvok/wakeword/main/registry/index.json install wakeword-cli
yam enable wakeword-cli
yam run wakeword-cli switch yasmina
```

Это собственный индекс Wakeword, а не запись в основном каталоге YAM. Ссылка
на архив привязана к версии релиза; YAM проверяет его SHA-256. Бинарник для aarch64,
список моделей не ограничен; фактически протестирована Мини 2 с часами.

Альтернатива — скачать `.yap` из [GitHub Releases](https://github.com/nayvok/wakeword/releases)
и выполнить `yam install ./wakeword-cli.yap`.

## Основной каталог YAM

YAM по умолчанию использует `nyakokitsu/yapkg`, ветку `registry`, файл `index.json`.
**На 27.10.2026 приём новых пакетов временно закрыт из-за миграции на v2** —
[уведомление владельца каталога](https://github.com/nyakokitsu/yapkg/blob/main/README.md).

Когда приём откроется, действующий процесс нужно перепроверить. Сейчас в
[инструкции ветки packages](https://github.com/nyakokitsu/yapkg/blob/packages/README.md)
описан такой путь:

1. Сделать fork `nyakokitsu/yapkg`, работать в ветке `packages`.
2. Добавить архив в `packages/wakeword-cli/1.2.1.yap`.
   Путь должен совпадать с `id` и `version` в `plugin.toml`.
3. Запустить их `tools/build_index.py --validate`.
4. Открыть PR **в ветку `packages`**, указав назначение, требования, проверенную
   колонку, результаты и ссылку на исходники.
5. После принятия CI каталога сформирует индекс в ветке `registry`.

До принятия пакета команда `yam install wakeword-cli` без `--registry` не заявлена.
Запрос сопровождающим от имени автора ещё не отправлен.

## Публикация новой версии Wakeword

```sh
cargo build --release --locked --target aarch64-unknown-linux-musl
python3 scripts/package.py
python3 scripts/registry.py wakeword-cli.yap
sha256sum wakeword-cli.yap > dist/SHA256SUMS
```

Сначала обновите версии Cargo и CHANGELOG. В релиз `v<version>` загрузите **этот же**
архив и `SHA256SUMS`; затем опубликуйте `registry/index.json`. Не пересобирайте
архив под уже опубликованным номером версии — обновление требует новой версии.
В индекс входит хеш точного релизного архива, а не произвольной локальной сборки.

## Как представить проект

Короткий текст для анонса:

> Wakeword — переключение локального слова активации Яндекс Станций между
> Алисой, Яндексом и Ясминой с русскими ответами. Есть CLI, JSON-контракт и пакет YAM,
> поэтому функцию можно встроить в свой интерфейс. Нужен root-доступ; проверено
> на Мини 2 с часами. Произвольные слова пока не поддерживаются. Лицензия MIT.
> https://github.com/nayvok/wakeword

Полезные первые отзывы — проверка на других прошивках/моделях и интеграция с
существующими панелями. До появления таких проверок не обещайте поддержку всех Станций.
