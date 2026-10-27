# JSON API v1

Основной интерфейс интеграции — запуск бинарника с `--json`. Отдельного HTTP-сервера
Wakeword нет. UI может вызывать CLI через свой backend, SSH или binary runtime YAM.

```sh
wakeword --json list
wakeword --json current
wakeword --json status
wakeword --json set yasmina
wakeword --json apply
wakeword --json --request-id ui-42 switch yasmina
```

Один вызов → один JSON-объект в stdout. Диагностика и вывод `q` идут в stderr.
Вызовы с применением обычно занимают 15–20 секунд; оставляйте запас таймаута.

## Ответы

```json
{"api_version":1,"ok":true,"command":"switch","request_id":"ui-42","data":{"selected":"yasmina","applied":true}}
```

```json
{"api_version":1,"ok":false,"command":"switch","request_id":null,"error":{"code":"unknown_name","message":"имя 'computer' не найдено"}}
```

`request_id` — переданное значение или `null`. Клиент должен игнорировать неизвестные
поля. `api_version` — версия контракта, `version` команды — версия программы.
Машиночитаемая схема конверта: [response.schema.json](response.schema.json).

| Команда | `data` |
| --- | --- |
| `list` | `selected`, `names: [{name, builtin, selected}]`, `capabilities` |
| `current` | `selected` |
| `set NAME` | `selected`, `applied: false` |
| `switch NAME` | `selected`, `applied: true` после успешного применения |
| `apply` | `selected`, `applied: true` после успешного применения |
| `status` | `selected`, `server_activation_validation_disabled`, `models`, `verification: "files_only"` |
| `version` | `version` |

`NAME`: `alisa`, `yandex`, `yasmina`. Строки произвольных имён не обучают модель.
`list.capabilities.custom_training` всегда `false` в этой версии.

У модели в `status` есть `path`, `version`, `expected_crc32`, `actual_crc32`,
`checksum_matches`. Для встроенной папки, не участвующей в проверке кэша,
три последних поля равны `null`. `selected` — выбранная настройка, а не измерение
работающего детектора. Даже успешный `status` не доказывает голосовую активацию.

## Ошибки и конкуренция

| Exit | Значение | Примеры `error.code` |
| --- | --- | --- |
| 0 | Успех команды | — |
| 1 | Операция не выполнена | `unknown_name`, `config_read_failed`, `config_write_failed`, `apply_failed`, `status_failed`, `lock_failed` |
| 2 | Ошибка вызова | `invalid_arguments`, `unknown_command` |
| 3 | Другой CLI меняет состояние | `busy` |

Не разбирайте текст `message`: он предназначен человеку и может меняться.
`switch` сохраняет новый выбор после успешного применения; ошибки предварительной
проверки сохраняют прежний выбор. Ошибка после начала установки не гарантирует
откат всех файлов — проверьте `status`. Параллельные изменения CLI защищены OS lock.
Читатели могут видеть промежуточное состояние во время применения.

## Через YAM CLI

```sh
yam run wakeword-cli --json list
yam run wakeword-cli --json switch yasmina
```

YAM может добавлять собственный вывод и объединять stdout/stderr в журнале.
Для машинной обработки используйте ответ Wakeword с `api_version: 1` и своим
`request_id`, а не разбирайте весь вывод `yam run` как один JSON.

## Через HTTP API YAM

```http
POST /api/plugins/wakeword-cli/invoke
Content-Type: application/json

{"args":["--json","--request-id","ui-42","switch","yasmina"]}
```

YAM принимает запуск асинхронно (`202` в исследованном upstream). Ответ на POST —
не результат переключения. Читайте `GET /api/plugins/wakeword-cli/logs` до JSON-строки
с `request_id: "ui-42"`; её `ok` определяет результат. Для состояния процесса
используйте `GET /api/plugins`, поле `running` нужного пакета.
Готовый пример: [yam_client.py](../examples/yam_client.py).

**Вызовы одного пакета через YAM нужно выполнять последовательно.** Новый `/invoke`
в исследованном YAM отменяет предыдущий процесс, в том числе во время применения.
Не запускайте `status` через тот же пакет, пока идёт `switch`: опрашивайте состояние
YAM и логи. Пример проверяет занятость, но согласовывать разные UI-клиенты должен
общий backend. При таймауте клиент не посылает команду остановки.

Если установленный `yamd` не поддерживает `args`, используйте CLI напрямую.
`WAKEWORD_CONFIG` меняет только путь собственных настроек; это не эмулятор колонки.
Для Rust-клиента доступен crate `wakeword_core`: `Manager`, `inspect`, `switch`.
Перед изменениями через библиотеку держите `Manager::try_lock()` до конца операции.

Источники интеграции: [формат пакетов YAM](https://github.com/nyakokitsu/yam/blob/main/docs/PACKAGES.md),
[реализация invoke/runtime](https://github.com/nyakokitsu/yam/blob/main/src/main.rs).
Совместимость BetterYAM с этим способом вызова на Mini 2 не проверена.
