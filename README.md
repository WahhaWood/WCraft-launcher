# WCraft Launcher

Лаунчер игроков: Tauri 2 (Rust) + Svelte 5. Вход, установку и запуск игры делает CLI
`horizon-launcher`; лаунчер его находит (или скачивает из релизов Horizon), показывает состояние
и одной кнопкой доводит до игры на сервере WCraft.

## Команды

```sh
npm install
npm run dev                      # только интерфейс в браузере, вместо Rust — src/lib/mock.ts
npx tauri dev                    # окно лаунчера с настоящим ядром
npm run check                    # svelte-check
(cd src-tauri && cargo test)     # тесты ядра
npx tauri build --bundles appimage > build.log 2>&1; tail -n 40 build.log
```

Sidecar `src-tauri/binaries/horizon-launcher-<target-triple>[.exe]` не хранится в git: собрать из
подмодуля (`cd ../horizon-cli && cargo build --release -p horizon-launcher`) и скопировать
с суффиксом триплета.

`target/` в debug занимает несколько гигабайт; при нехватке места собирайте в tmpfs:
`CARGO_TARGET_DIR=/tmp/wcraft-target npx tauri build ...`.
Windows с Linux: `scripts/build-windows.sh` (в основном репозитории проекта) кладёт один файл `dist/WCraft.exe`.
Цель MSVC через `cargo-xwin`, поэтому загрузчик WebView2 и vcruntime линкуются статически, DLL рядом не нужны.
`WCRAFT_EMBED_CLI=<horizon-launcher.exe>` вшивает CLI в сжатом виде; при первом запуске он распаковывается
в `%APPDATA%\wcraft-launcher\cli\<hash>\`. Без WebView2 Runtime лаунчер предлагает скачать установщик Microsoft.

Проверки на живом Horizon (по запросу, `--ignored`): `cargo test -- --ignored bootstrap`
(скачивает CLI с GitHub), `cargo test -- --ignored setup` (создаёт instance WCraft у себя).

## Карта

| Файл | Что в нём |
|---|---|
| `src-tauri/wcraft.json` | сборка: адрес сервера, закреплённая версия Horizon, имя instance |
| `src-tauri/src/horizon.rs` | поиск и запуск CLI, статус, `setup` (install → prepare → instance), `play` |
| `src-tauri/src/bootstrap.rs` | скачивание `horizon-launcher` из релизов, проверка SHA-256 |
| `src-tauri/src/skins.rs` | библиотека скинов; стандартные вшиты из `src-tauri/skins/` |
| `src-tauri/src/shaders.rs` | шейдеры instance WCraft: каталог из реестра Horizon, установка и включение через CLI (`mods`, `packs`) |
| `src-tauri/src/ping.rs` | статус сервера (Server List Ping) |
| `src-tauri/src/update.rs` | самообновление: подписанный `latest.json` с сервиса, замена .exe / AppImage |
| `src-tauri/src/lib.rs` | команды Tauri, поведение окна во время игры |
| `src/lib/state.svelte.ts` | состояние интерфейса и действия |
| `src/lib/skin.ts` | картинки скинов для карточек, проверка загружаемого PNG |
| `src/pages/` | Играть, Скины, Шейдеры, Настройки, Журнал |

Данные лаунчера: `~/.local/share/wcraft-launcher` (Windows: `%APPDATA%\wcraft-launcher`),
переопределяется `WCRAFT_HOME`.

Скриншоты страниц без окна: `npm run dev`, затем
`firefox --headless --screenshot out.png "http://localhost:1420/?shot&page=skins"`
(параметры мока: `fresh`, `offline`, `click`, `busy`, `defaults`, `shaders`).

## Что здесь публично и почему это безопасно

Лаунчер открыт целиком. Секретов в нём нет: аккаунты, вайтлист, сессии и проверка версии лаунчера
решаются на сервере, клиент ничему не доверяет. Самообновление проверяет Ed25519-подпись манифеста
ключом WCraft (в `src-tauri/src/update.rs` лежит только публичный ключ), так что форк с чужим
адресом на игроков WCraft не влияет. Официальные сборки — только из Releases этого репозитория.
