use crate::{
    ApiError, MicrophoneDevice, MirrorPreview, PlaybackDevice, SelectedOutput, UiState, WebSettings,
};
use gloo_net::http::{Request, Response};
use serde::de::DeserializeOwned;
use web_sys::{Event, HtmlInputElement, HtmlSelectElement, Storage};
use yew::platform::spawn_local;
use yew::prelude::*;

const STORAGE_KEY: &str = "q900-bridge-settings-v1";

pub fn run() {
    yew::Renderer::<App>::new().render();
}

#[component]
fn App() -> Html {
    let server_state = use_state(|| None::<UiState>);
    let settings = use_state(WebSettings::default);
    let preview = use_state(|| None::<String>);
    let status = use_state(|| "Загружаю устройства PipeWire…".to_owned());
    let busy = use_state(|| true);

    {
        let server_state = server_state.clone();
        let settings = settings.clone();
        let status = status.clone();
        let busy = busy.clone();
        use_effect_with((), move |_| {
            spawn_local(async move {
                match get_state().await {
                    Ok(state) => {
                        let restored = load_saved_settings().filter(|saved| {
                            saved.selected_microphone.as_ref().is_none_or(|name| {
                                state.microphones.iter().any(|mic| &mic.name == name)
                            }) && saved.outputs.iter().all(|saved_output| {
                                state.outputs.iter().any(|device| {
                                    device.name == saved_output.sink && device.stereo_supported
                                })
                            })
                        });
                        if let Some(saved) = restored {
                            if save_settings_remote(&saved).await.is_ok() {
                                settings.set(saved);
                                status.set("Сохранённый выбор устройств восстановлен.".to_owned());
                            } else {
                                settings.set(settings_from_state(&state));
                                status.set("Выберите доступные устройства.".to_owned());
                            }
                        } else {
                            settings.set(settings_from_state(&state));
                            status.set("Устройства обновлены.".to_owned());
                        }
                        server_state.set(Some(state));
                    }
                    Err(error) => status.set(format!("Ошибка: {error}")),
                }
                busy.set(false);
            });
            || ()
        });
    }

    let on_microphone_change = {
        let settings = settings.clone();
        let status = status.clone();
        let preview = preview.clone();
        Callback::from(move |event: Event| {
            let selected = event.target_unchecked_into::<HtmlSelectElement>().value();
            let mut next = (*settings).clone();
            next.selected_microphone = (!selected.is_empty()).then_some(selected);
            save_settings(next, settings.clone(), status.clone(), preview.clone());
        })
    };

    let on_refresh = {
        let server_state = server_state.clone();
        let settings = settings.clone();
        let status = status.clone();
        let busy = busy.clone();
        let preview = preview.clone();
        Callback::from(move |_| {
            busy.set(true);
            status.set("Обновляю список устройств…".to_owned());
            preview.set(None);
            let server_state = server_state.clone();
            let settings = settings.clone();
            let status = status.clone();
            let busy = busy.clone();
            spawn_local(async move {
                match get_state().await {
                    Ok(state) => {
                        let still_available = settings_available(&settings, &state);
                        if !still_available {
                            status.set(
                                "Некоторые выбранные устройства недоступны. Проверьте список."
                                    .to_owned(),
                            );
                        } else {
                            status.set("Устройства обновлены.".to_owned());
                        }
                        server_state.set(Some(state));
                    }
                    Err(error) => status.set(format!("Ошибка: {error}")),
                }
                busy.set(false);
            });
        })
    };

    let on_start = {
        let settings = settings.clone();
        let server_state = server_state.clone();
        let status = status.clone();
        let busy = busy.clone();
        let preview = preview.clone();
        Callback::from(move |_| {
            if settings.outputs.len() < 2 {
                status.set("Выберите как минимум два выхода.".to_owned());
                return;
            }
            busy.set(true);
            status.set("Запускаю зеркалирование…".to_owned());
            preview.set(None);
            let selected = (*settings).clone();
            let server_state = server_state.clone();
            let status = status.clone();
            let busy = busy.clone();
            spawn_local(async move {
                let result = async {
                    save_settings_remote(&selected).await?;
                    post_control("/api/mirror/start").await
                }
                .await;
                match result {
                    Ok(state) => {
                        server_state.set(Some(state));
                        status.set("Системный звук направлен на выбранные выходы.".to_owned());
                    }
                    Err(error) => status.set(format!("Ошибка: {error}")),
                }
                busy.set(false);
            });
        })
    };

    let on_preview = {
        let settings = settings.clone();
        let status = status.clone();
        let busy = busy.clone();
        let preview = preview.clone();
        Callback::from(move |_| {
            if settings.outputs.len() < 2 {
                status.set("Выберите как минимум два выхода.".to_owned());
                return;
            }
            busy.set(true);
            status.set("Собираю план маршрутов без воспроизведения…".to_owned());
            let selected = (*settings).clone();
            let status = status.clone();
            let busy = busy.clone();
            let preview = preview.clone();
            spawn_local(async move {
                let result = async {
                    save_settings_remote(&selected).await?;
                    post_preview().await
                }
                .await;
                match result {
                    Ok(result) => {
                        preview.set(Some(result.output));
                        status.set("Предпросмотр готов; звук не воспроизводился.".to_owned());
                    }
                    Err(error) => status.set(format!("Ошибка: {error}")),
                }
                busy.set(false);
            });
        })
    };

    let on_stop = {
        let server_state = server_state.clone();
        let status = status.clone();
        let busy = busy.clone();
        Callback::from(move |_| {
            busy.set(true);
            status.set("Останавливаю зеркалирование…".to_owned());
            let server_state = server_state.clone();
            let status = status.clone();
            let busy = busy.clone();
            spawn_local(async move {
                match post_control("/api/mirror/stop").await {
                    Ok(state) => {
                        server_state.set(Some(state));
                        status.set(
                            "Зеркалирование остановлено; прежний маршрут восстановлен.".to_owned(),
                        );
                    }
                    Err(error) => status.set(format!("Ошибка: {error}")),
                }
                busy.set(false);
            });
        })
    };

    let state = (*server_state).clone();
    let selected_count = settings.outputs.len();
    let is_running = state.as_ref().is_some_and(|state| state.mirror_running);
    let can_start = selected_count >= 2 && !is_running && !*busy;
    let can_stop = is_running && !*busy;
    let status_class = if status.starts_with("Ошибка") {
        "status error"
    } else if status.contains("остановлено") || status.contains("обновлены") {
        "status success"
    } else {
        "status"
    };

    html! {
        <main>
            <header>
                <div>
                    <h1>{ "Параллельное воспроизведение" }</h1>
                    <p>{ "Выберите выходы PipeWire и добавьте задержку для каждого." }</p>
                </div>
                <div class={classes!("badge", is_running.then_some("running"))} role="status">
                    { if is_running { "Воспроизведение активно" } else { "Остановлено" } }
                </div>
            </header>

            <section class="card" aria-labelledby="mic-title">
                <div class="section-head">
                    <div>
                        <div class="step">{ "Настройка" }</div>
                        <h2 id="mic-title">{ "Микрофон для калибровки" }</h2>
                    </div>
                    <button class="secondary" type="button" onclick={on_refresh} disabled={*busy}>
                        { "Обновить устройства" }
                    </button>
                </div>
                <label for="microphone-select">{ "Источник захвата" }</label>
                { render_microphone_select(&state, &settings, on_microphone_change) }
                <p class="hint">
                    { "Выбор Logitech BRIO сохраняется для автоматической акустической калибровки. Измерение пробным сигналом ещё не подключено." }
                </p>
            </section>

            <section class="card" aria-labelledby="outputs-title">
                <div class="section-head">
                    <div>
                        <div class="step">{ "Воспроизведение" }</div>
                        <h2 id="outputs-title">{ "Выходные устройства" }</h2>
                    </div>
                    <span class="step">{ format!("{selected_count} выбрано") }</span>
                </div>
                <div class="output-list" aria-live="polite">
                    { render_outputs(&state, &settings, &status, &preview) }
                </div>
                <p class="hint">
                    { "Системный звук поступит на все выбранные выходы. Задержка добавляется к конкретному выходу." }
                </p>
            </section>

            <section class="card" aria-labelledby="control-title">
                <div class="step">{ "Управление" }</div>
                <h2 id="control-title">{ "Зеркалирование системного звука" }</h2>
                <p class={status_class} role="status" aria-live="polite">{ (*status).clone() }</p>
                <div class="actions">
                    <button class="secondary" type="button" onclick={on_preview} disabled={selected_count < 2 || *busy}>
                        { "Предпросмотр маршрутов" }
                    </button>
                    <button type="button" onclick={on_start} disabled={!can_start}>
                        { "Запустить воспроизведение" }
                    </button>
                    <button class="stop" type="button" onclick={on_stop} disabled={!can_stop}>
                        { "Остановить" }
                    </button>
                </div>
                if let Some(output) = (*preview).as_ref() {
                    <pre class="preview">{ output.clone() }</pre>
                }
            </section>

            <p class="calibration-note">
                { "Автоматическая калибровка воспроизведёт короткий тестовый сигнал и покажет измеренные задержки до их применения." }
            </p>
            <footer>{ "Локальная панель. Сервис принимает запросы только от loopback-адресов." }</footer>
        </main>
    }
}

fn render_microphone_select(
    state: &Option<UiState>,
    settings: &UseStateHandle<WebSettings>,
    onchange: Callback<Event>,
) -> Html {
    let selected = settings.selected_microphone.clone().unwrap_or_default();
    let microphones = state
        .as_ref()
        .map(|state| state.microphones.as_slice())
        .unwrap_or_default();
    let selected_is_available = microphones.iter().any(|mic| mic.name == selected);
    html! {
        <select id="microphone-select" value={selected.clone()} onchange={onchange}>
            <option value="">{ "Выберите источник микрофона" }</option>
            if !selected.is_empty() && !selected_is_available {
                <option value={selected.clone()}>{ format!("{} · недоступен", selected) }</option>
            }
            { for microphones.iter().map(|mic| microphone_option(mic, &selected)) }
        </select>
    }
}

fn microphone_option(microphone: &MicrophoneDevice, selected: &str) -> Html {
    html! {
        <option value={microphone.name.clone()} selected={microphone.name == selected}>
            { format!("{} · {} · {}", microphone.description, microphone.sample_spec, microphone.state) }
        </option>
    }
}

fn render_outputs(
    state: &Option<UiState>,
    settings: &UseStateHandle<WebSettings>,
    status: &UseStateHandle<String>,
    preview: &UseStateHandle<Option<String>>,
) -> Html {
    let Some(state) = state else {
        return html! { <div class="empty">{ "Подключаюсь к PipeWire…" }</div> };
    };
    if state.outputs.is_empty() {
        return html! { <div class="empty">{ "Выходные устройства PipeWire не найдены." }</div> };
    }

    html! {
        { for state.outputs.iter().enumerate().map(|(index, output)| {
            output_row(index, output, settings, status, preview)
        }) }
    }
}

fn output_row(
    index: usize,
    output: &PlaybackDevice,
    settings: &UseStateHandle<WebSettings>,
    status: &UseStateHandle<String>,
    preview: &UseStateHandle<Option<String>>,
) -> Html {
    let selected = settings.outputs.iter().any(|item| item.sink == output.name);
    let delay_ms = settings
        .outputs
        .iter()
        .find(|item| item.sink == output.name)
        .map(|item| item.delay_ms)
        .unwrap_or_default();
    let sink_name = output.name.clone();
    let on_toggle = {
        let settings = settings.clone();
        let status = status.clone();
        let preview = preview.clone();
        let sink_name = sink_name.clone();
        Callback::from(move |event: Event| {
            let checked = event.target_unchecked_into::<HtmlInputElement>().checked();
            let mut next = (*settings).clone();
            if checked {
                if !next.outputs.iter().any(|item| item.sink == sink_name) {
                    next.outputs.push(SelectedOutput {
                        sink: sink_name.clone(),
                        delay_ms: 0,
                    });
                }
            } else {
                next.outputs.retain(|item| item.sink != sink_name);
            }
            save_settings(next, settings.clone(), status.clone(), preview.clone());
        })
    };
    let on_delay = {
        let settings = settings.clone();
        let status = status.clone();
        let preview = preview.clone();
        let sink_name = output.name.clone();
        Callback::from(move |event: Event| {
            let value = event.target_unchecked_into::<HtmlInputElement>().value();
            let Ok(delay_ms) = value.parse::<u16>() else {
                status.set("Введите задержку в миллисекундах от 0 до 2000.".to_owned());
                return;
            };
            if delay_ms > 2000 {
                status.set("Задержка должна быть от 0 до 2000 мс.".to_owned());
                return;
            }
            let mut next = (*settings).clone();
            if let Some(item) = next.outputs.iter_mut().find(|item| item.sink == sink_name) {
                item.delay_ms = delay_ms;
                save_settings(next, settings.clone(), status.clone(), preview.clone());
            }
        })
    };
    let id = format!("output-{index}");
    let channel_map = if output.positions.is_empty() {
        format!("{} каналов", output.channels)
    } else {
        output.positions.join(" / ")
    };
    let details = format!(
        "{} · {} · {} · {}",
        output.transport, channel_map, output.sample_spec, output.state
    );

    html! {
        <article class="output">
            <input
                id={id.clone()}
                type="checkbox"
                checked={selected}
                disabled={!output.stereo_supported}
                aria-label={format!("Воспроизводить через {}", output.description)}
                onchange={on_toggle}
            />
            <div>
                <label class="output-name" for={id}>{ output.description.clone() }</label>
                <div class="output-detail">{ details }</div>
                if !output.stereo_supported {
                    <div class="output-detail unsupported">
                        { format!("Стерео недоступно: {}", output.stereo_error.as_deref().unwrap_or("неизвестная раскладка")) }
                    </div>
                }
            </div>
            <div class="delay">
                <label for={format!("delay-{index}")}>{ "Задержка, мс" }</label>
                <input
                    id={format!("delay-{index}")}
                    type="number"
                    min="0"
                    max="2000"
                    step="1"
                    value={delay_ms.to_string()}
                    disabled={!selected || !output.stereo_supported}
                    aria-label={format!("Задержка для {}, миллисекунды", output.description)}
                    onchange={on_delay}
                />
            </div>
        </article>
    }
}

fn settings_from_state(state: &UiState) -> WebSettings {
    WebSettings {
        selected_microphone: state
            .selected_microphone
            .clone()
            .or_else(|| state.default_microphone.clone()),
        outputs: state.selected_outputs.clone(),
    }
}

fn settings_available(settings: &WebSettings, state: &UiState) -> bool {
    settings
        .selected_microphone
        .as_ref()
        .is_none_or(|selected| {
            state
                .microphones
                .iter()
                .any(|microphone| &microphone.name == selected)
        })
        && settings.outputs.iter().all(|selected| {
            state
                .outputs
                .iter()
                .any(|device| device.name == selected.sink && device.stereo_supported)
        })
}

fn save_settings(
    value: WebSettings,
    settings: UseStateHandle<WebSettings>,
    status: UseStateHandle<String>,
    preview: UseStateHandle<Option<String>>,
) {
    settings.set(value.clone());
    preview.set(None);
    remember_settings(&value);
    status.set("Сохраняю выбор устройств…".to_owned());
    spawn_local(async move {
        match save_settings_remote(&value).await {
            Ok(()) => status.set("Выбор устройств сохранён.".to_owned()),
            Err(error) => status.set(format!("Ошибка: {error}")),
        }
    });
}

async fn get_state() -> Result<UiState, String> {
    let response = Request::get("/api/state")
        .send()
        .await
        .map_err(|error| error.to_string())?;
    decode_response(response).await
}

async fn save_settings_remote(settings: &WebSettings) -> Result<(), String> {
    let response = Request::post("/api/settings")
        .json(settings)
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    decode_response::<UiState>(response).await.map(|_| ())
}

async fn post_control(path: &str) -> Result<UiState, String> {
    let response = Request::post(path)
        .header("Content-Type", "application/json")
        .body("{}")
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    decode_response(response).await
}

async fn post_preview() -> Result<MirrorPreview, String> {
    let response = Request::post("/api/mirror/preview")
        .header("Content-Type", "application/json")
        .body("{}")
        .map_err(|error| error.to_string())?
        .send()
        .await
        .map_err(|error| error.to_string())?;
    decode_response(response).await
}

async fn decode_response<T: DeserializeOwned>(response: Response) -> Result<T, String> {
    if response.ok() {
        response.json().await.map_err(|error| error.to_string())
    } else {
        let status = response.status();
        match response.json::<ApiError>().await {
            Ok(error) => Err(error.error),
            Err(_) => Err(format!("Ошибка HTTP {status}")),
        }
    }
}

fn load_saved_settings() -> Option<WebSettings> {
    let stored = browser_storage()?.get_item(STORAGE_KEY).ok()??;
    serde_json::from_str(&stored).ok()
}

fn remember_settings(settings: &WebSettings) {
    if let (Some(storage), Ok(serialized)) = (browser_storage(), serde_json::to_string(settings)) {
        let _ = storage.set_item(STORAGE_KEY, &serialized);
    }
}

fn browser_storage() -> Option<Storage> {
    web_sys::window()?.local_storage().ok()?
}
