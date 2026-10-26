use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{cell::Cell, fmt::Write as _, rc::Rc};

#[path = "theme_contract.rs"]
mod theme_contract;
use theme_contract::{MAX_CUSTOM_THEMES, MAX_THEME_JSON_BYTES, THEME_KIND, Theme};

const APPEARANCE_KIND: &str = "rpi-work-appearance";

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Paper,
    Ocean,
    Graphite,
    Reddit,
    Custom(u8),
}
impl Appearance {
    // Custom identities are stable slots; the complete imported ID lives in the library.
    pub const fn id(self) -> &'static str {
        match self {
            Self::Paper => "paper",
            Self::Ocean => "ocean",
            Self::Graphite => "graphite",
            Self::Reddit => "reddit",
            Self::Custom(0) => "custom-0",
            Self::Custom(1) => "custom-1",
            Self::Custom(2) => "custom-2",
            Self::Custom(3) => "custom-3",
            Self::Custom(4) => "custom-4",
            Self::Custom(5) => "custom-5",
            Self::Custom(6) => "custom-6",
            Self::Custom(7) => "custom-7",
            Self::Custom(8) => "custom-8",
            Self::Custom(9) => "custom-9",
            Self::Custom(10) => "custom-10",
            Self::Custom(_) => "custom-11",
        }
    }
    pub const fn name(self) -> &'static str {
        match self {
            Self::Paper => "纸与苔绿",
            Self::Ocean => "海湾蓝",
            Self::Graphite => "石墨紫",
            Self::Reddit => "Reddit 橙",
            Self::Custom(_) => "自定义主题",
        }
    }
    pub const fn all() -> [Self; 4] {
        [Self::Paper, Self::Ocean, Self::Graphite, Self::Reddit]
    }
    fn from_id(id: &str) -> Option<Self> {
        Self::all().into_iter().find(|theme| theme.id() == id)
    }
}

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub struct AppearanceSettings {
    pub theme: Appearance,
    pub mode: &'static str,
    pub density: &'static str,
}
impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: Appearance::Paper,
            mode: "light",
            density: "standard",
        }
    }
}

#[derive(Clone, Default, Debug, PartialEq)]
struct ThemeLibrary(Vec<Theme>);
impl ThemeLibrary {
    fn resolve(&self, appearance: Appearance) -> Theme {
        match appearance {
            Appearance::Custom(slot) => self
                .0
                .get(slot as usize)
                .cloned()
                .unwrap_or_else(|| theme_contract::presets().remove(0)),
            _ => theme_contract::presets()
                .into_iter()
                .find(|theme| theme.id == appearance.id())
                .unwrap(),
        }
    }
    fn find(&self, id: &str) -> Option<Appearance> {
        Appearance::from_id(id).or_else(|| {
            self.0.iter().position(|theme| theme.id == id).map(|slot| {
                Appearance::Custom(u8::try_from(slot).expect("theme slots are limited to 12"))
            })
        })
    }
    fn insert(&mut self, theme: Theme) -> Result<Appearance, String> {
        if let Some(preset) = theme_contract::presets()
            .into_iter()
            .find(|preset| preset.id == theme.id)
        {
            if preset != theme {
                return Err("主题 ID 不能覆盖内置主题，请使用新的 ID。".into());
            }
            return Ok(Appearance::from_id(&theme.id).unwrap());
        }
        if let Some(slot) = self.0.iter().position(|existing| existing.id == theme.id) {
            if self.0[slot] != theme {
                return Err("主题 ID 已存在且内容不同，请使用新的 ID。".into());
            }
            return Ok(Appearance::Custom(
                u8::try_from(slot).expect("theme slots are limited to 12"),
            ));
        }
        if self.0.len() >= MAX_CUSTOM_THEMES {
            return Err("最多保存 12 个自定义主题。".into());
        }
        let slot = self.0.len();
        self.0.push(theme);
        Ok(Appearance::Custom(
            u8::try_from(slot).expect("theme slots are limited to 12"),
        ))
    }
}

fn mode(value: Option<&str>) -> Result<&'static str, String> {
    match value {
        Some("light") => Ok("light"),
        Some("dark") => Ok("dark"),
        Some("system") => Ok("system"),
        _ => Err("明暗模式必须是 light、dark 或 system。".into()),
    }
}
fn density(value: Option<&str>) -> Result<&'static str, String> {
    match value {
        Some("standard") => Ok("standard"),
        Some("compact") => Ok("compact"),
        _ => Err("density 必须是 standard 或 compact。".into()),
    }
}

fn restored(value: &Value) -> (AppearanceSettings, ThemeLibrary) {
    let mut library = ThemeLibrary::default();
    if let Some(themes) = value.get("customThemes").and_then(Value::as_array) {
        for candidate in themes.iter().take(MAX_CUSTOM_THEMES) {
            if let Ok(theme) = theme_contract::parse_theme(candidate) {
                let _ = library.insert(theme);
            }
        }
    }
    let settings = AppearanceSettings {
        theme: library
            .find(
                value
                    .get("theme")
                    .and_then(Value::as_str)
                    .unwrap_or("paper"),
            )
            .unwrap_or(Appearance::Paper),
        mode: mode(value.get("mode").and_then(Value::as_str)).unwrap_or("light"),
        density: density(value.get("density").and_then(Value::as_str)).unwrap_or("standard"),
    };
    (settings, library)
}

#[cfg(not(target_arch = "wasm32"))]
fn preference_path() -> Option<std::path::PathBuf> {
    #[cfg(target_os = "windows")]
    let root = std::env::var_os("LOCALAPPDATA").map(std::path::PathBuf::from);
    #[cfg(target_os = "macos")]
    let root = std::env::var_os("HOME")
        .map(|p| std::path::PathBuf::from(p).join("Library/Application Support"));
    #[cfg(all(not(target_os = "windows"), not(target_os = "macos")))]
    let root = std::env::var_os("XDG_CONFIG_HOME")
        .map(std::path::PathBuf::from)
        .or_else(|| std::env::var_os("HOME").map(|p| std::path::PathBuf::from(p).join(".config")));
    root.map(|p| p.join("rpi-work").join("appearance.json"))
}
fn load() -> (AppearanceSettings, ThemeLibrary) {
    #[cfg(not(target_arch = "wasm32"))]
    if let Some(value) = preference_path()
        .and_then(|path| std::fs::read_to_string(path).ok())
        .and_then(|text| serde_json::from_str::<Value>(&text).ok())
    {
        return restored(&value);
    }
    (AppearanceSettings::default(), ThemeLibrary::default())
}
fn preference_value(settings: AppearanceSettings, library: &ThemeLibrary) -> Value {
    json!({"theme":library.resolve(settings.theme).id,"mode":settings.mode,"density":settings.density,"customThemes":library.0.iter().map(theme_contract::theme_value).collect::<Vec<_>>()})
}
#[cfg(not(target_arch = "wasm32"))]
fn persist_native(settings: AppearanceSettings, library: &ThemeLibrary) -> Result<(), String> {
    let path = preference_path().ok_or("无法定位本设备的偏好目录。")?;
    if let Some(parent) = path.parent() {
        std::fs::create_dir_all(parent).map_err(|_| "无法创建偏好目录。")?;
    }
    // Keep the previous saved file intact on a failed write.
    let temporary = path.with_extension("json.tmp");
    std::fs::write(&temporary, preference_value(settings, library).to_string())
        .map_err(|_| "无法保存外观偏好。")?;
    std::fs::rename(&temporary, &path).map_err(|_| "无法替换已保存的外观偏好。".into())
}
#[cfg(target_arch = "wasm32")]
fn persist_script(settings: AppearanceSettings, library: &ThemeLibrary) -> String {
    let literal = serde_json::to_string(&preference_value(settings, library).to_string()).unwrap();
    format!(
        "try {{ if(!document.getElementById('appearance')?.open){{dioxus.send(false);}}else{{localStorage.setItem('rpi-work-appearance-v1', {literal});dioxus.send(true);}} }} catch (_) {{ dioxus.send(false); }}"
    )
}

fn parse_portable_appearance(
    value: &Value,
    library: &ThemeLibrary,
) -> Result<AppearanceSettings, String> {
    let object = value.as_object().ok_or("导入文件必须是 JSON 对象。")?;
    let fields = ["schemaVersion", "kind", "preset", "mode", "density"];
    if object.len() != fields.len() || object.keys().any(|key| !fields.contains(&key.as_str())) {
        return Err("偏好 JSON 含有未知或缺失字段。".into());
    }
    if value["schemaVersion"].as_u64() != Some(1) || value["kind"].as_str() != Some(APPEARANCE_KIND)
    {
        return Err("偏好 JSON 版本或类型不受支持。".into());
    }
    Ok(AppearanceSettings {
        theme: library
            .find(value["preset"].as_str().unwrap_or(""))
            .ok_or("偏好引用的主题尚未导入，请先导入完整主题 JSON。")?,
        mode: mode(value["mode"].as_str())?,
        density: density(value["density"].as_str())?,
    })
}
fn import_document(
    text: &str,
    current: AppearanceSettings,
    library: &ThemeLibrary,
) -> Result<(AppearanceSettings, ThemeLibrary, &'static str), String> {
    if text.len() > MAX_THEME_JSON_BYTES {
        return Err("主题 JSON 不能超过 64 KiB。".into());
    }
    let value: Value = serde_json::from_str(text).map_err(|_| "无法解析主题 JSON。")?;
    match value["kind"].as_str() {
        Some(THEME_KIND) => {
            let (theme, preferred_mode, preferred_density) =
                theme_contract::parse_theme_document(&value)?;
            let mut next = library.clone();
            let theme = next.insert(theme)?;
            Ok((
                AppearanceSettings {
                    theme,
                    mode: if let Some(value) = preferred_mode {
                        mode(Some(value))?
                    } else {
                        current.mode
                    },
                    density: if let Some(value) = preferred_density {
                        density(Some(value))?
                    } else {
                        current.density
                    },
                },
                next,
                "已导入完整主题 JSON，正在预览；保存后保留。",
            ))
        }
        Some(APPEARANCE_KIND) => Ok((
            parse_portable_appearance(&value, library)?,
            library.clone(),
            "已导入外观偏好 JSON，正在预览；保存后保留。",
        )),
        _ => Err("无法识别文件类型：需要 rpi-work-theme 主题或 rpi-work-appearance 偏好。".into()),
    }
}
fn export_script(settings: AppearanceSettings, library: &ThemeLibrary, full: bool) -> String {
    let theme = library.resolve(settings.theme);
    let payload = if full {
        theme_contract::theme_document(&theme, settings.mode, settings.density)
    } else {
        json!({"schemaVersion":1,"kind":APPEARANCE_KIND,"preset":theme.id,"mode":settings.mode,"density":settings.density})
    };
    let filename = if full {
        format!("rpi-work-theme-{}.json", theme.id)
    } else {
        "rpi-work-appearance.json".into()
    };
    // Imported names/descriptions remain JSON data; they never become executable JS or CSS.
    let literal = serde_json::to_string(&payload.to_string()).unwrap();
    let filename = serde_json::to_string(&filename).unwrap();
    format!(
        "try{{const payload=JSON.parse({literal});const url=URL.createObjectURL(new Blob([JSON.stringify(payload,null,2)+'\\n'],{{type:'application/json'}}));try{{const a=document.createElement('a');a.href=url;a.download={filename};document.body.append(a);try{{a.click();}}finally{{a.remove();}}}}finally{{setTimeout(()=>URL.revokeObjectURL(url),1000);}}dioxus.send(true);}}catch(_){{dioxus.send(false);}}"
    )
}

fn apply_script(settings: AppearanceSettings, library: &ThemeLibrary) -> String {
    let theme = library.resolve(settings.theme);
    let light = theme_contract::build_tokens(&theme, false, settings.density == "compact");
    let dark = theme_contract::build_tokens(&theme, true, settings.density == "compact");
    let payload = json!({"id":theme.id,"mode":settings.mode,"density":settings.density,"custom":matches!(settings.theme,Appearance::Custom(_)),"light":light,"dark":dark});
    let literal = serde_json::to_string(&payload.to_string()).unwrap();
    format!(
        r#"
        window.__rpiAppearanceDispose?.();
        const config=JSON.parse({literal}), root=document.documentElement;
        const media=matchMedia('(prefers-color-scheme: dark)');
        const apply=()=>{{
            const dark=config.mode==='dark'||(config.mode==='system'&&media.matches), mode=dark?'dark':'light', tokens=config[mode];
            for(const key of Object.keys(tokens))root.style.removeProperty('--'+key);
            if(config.custom)for(const [key,value] of Object.entries(tokens))root.style.setProperty('--'+key,value);
            else for(const key of ['density-factor','font-message','message-line-height','control-height'])root.style.setProperty('--'+key,tokens[key]);
            root.dataset.theme=config.id;root.dataset.mode=mode;root.dataset.colorMode=mode;
            root.dataset.appearance=config.mode;root.dataset.density=config.density;root.style.colorScheme=mode;
            document.body?.classList.toggle('theme-dark',dark);
            document.querySelector('meta[name="theme-color"]')?.setAttribute('content',tokens.bg);
        }};
        apply();media.addEventListener('change',apply);
        window.__rpiAppearanceDispose=()=>media.removeEventListener('change',apply);
        dioxus.send(true);
    "#
    )
}

pub fn use_appearance() -> Signal<AppearanceSettings> {
    let initial = use_hook(load);
    let settings = use_signal(|| initial.0);
    let library = use_signal(|| initial.1);
    use_context_provider(|| library);
    let ready = use_signal(|| cfg!(not(target_arch = "wasm32")));
    #[cfg(target_arch = "wasm32")]
    {
        let (mut settings, mut library, mut ready) = (settings, library, ready);
        use_effect(move || {
            spawn(async move {
                if let Ok(value) = document::eval("try{dioxus.send(JSON.parse(localStorage.getItem('rpi-work-appearance-v1')||'null'));}catch(_){dioxus.send(null);}").recv::<Value>().await {
                    if !value.is_null() { let (value, themes) = restored(&value); library.set(themes); settings.set(value); }
                }
                ready.set(true);
            });
        });
    }
    use_effect(move || {
        let is_ready = *ready.read();
        let value = *settings.read();
        let themes = library.read().clone();
        if is_ready {
            spawn(async move {
                let _ = document::eval(&apply_script(value, &themes))
                    .recv::<bool>()
                    .await;
            });
        }
    });
    settings
}

fn preview_style(theme: &Theme) -> String {
    let mut style = String::new();
    for (mode, palette) in [("light", &theme.light), ("dark", &theme.dark)] {
        for (key, value) in [
            ("bg", &palette.bg),
            ("side", &palette.side),
            ("card", &palette.card),
            ("line", &palette.line),
            ("accent", &palette.accent),
        ] {
            let _ = write!(style, "--mini-{mode}-{key}:{value};");
        }
    }
    let radius = match theme.shape.as_str() {
        "crisp" => 5,
        "square" => 3,
        _ => 8,
    };
    let _ = write!(style, "--mini-radius:{radius}px");
    style
}

#[component]
pub fn AppearancePicker(
    mut settings: Signal<AppearanceSettings>,
    onclose: EventHandler<()>,
) -> Element {
    let mut library = use_context::<Signal<ThemeLibrary>>();
    let snapshot = use_hook(|| (*settings.read(), library.read().clone()));
    let original = use_signal(|| snapshot);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let saving = use_signal(|| false);
    let session = use_hook(|| Rc::new(Cell::new(true)));
    let saved = use_hook(|| Rc::new(Cell::new(false)));
    let epoch = use_hook(|| Rc::new(Cell::new(0u64)));
    {
        let (session, saved) = (session.clone(), saved.clone());
        use_drop(move || {
            session.set(false);
            if !saved.get() {
                let (value, themes) = original.peek().clone();
                settings.set(value);
                library.set(themes);
            }
        });
    }
    let mut cancel = {
        let (session, saved) = (session.clone(), saved.clone());
        move || {
            session.set(false);
            if !saved.get() {
                let (value, themes) = original.peek().clone();
                settings.set(value);
                library.set(themes);
            }
            onclose.call(());
        }
    };
    let save = {
        let (session, saved, epoch) = (session.clone(), saved.clone(), epoch.clone());
        move || {
            let mut error = error;
            if *saving.peek() || !session.get() {
                return;
            }
            epoch.set(epoch.get() + 1);
            let value = *settings.peek();
            let themes = library.peek().clone();
            #[cfg(not(target_arch = "wasm32"))]
            match persist_native(value, &themes) {
                Ok(()) => {
                    saved.set(true);
                    session.set(false);
                    onclose.call(());
                }
                Err(message) => error.set(message),
            }
            #[cfg(target_arch = "wasm32")]
            {
                let mut saving = saving;
                let (session, saved) = (session.clone(), saved.clone());
                saving.set(true);
                spawn(async move {
                    let result = document::eval(&persist_script(value, &themes))
                        .recv::<bool>()
                        .await;
                    if !session.get() {
                        return;
                    }
                    saving.set(false);
                    match result {
                        Ok(true) => {
                            saved.set(true);
                            session.set(false);
                            onclose.call(());
                        }
                        _ => error.set("无法保存外观偏好，请检查浏览器存储权限。".into()),
                    }
                });
            }
        }
    };
    let import = {
        let (session, epoch) = (session.clone(), epoch.clone());
        move |_| {
            if *saving.peek() {
                return;
            }
            epoch.set(epoch.get() + 1);
            let generation = epoch.get();
            let (session, epoch) = (session.clone(), epoch.clone());
            spawn(async move {
                let script = r"try{const dialog=document.getElementById('appearance'),input=document.getElementById('appearance-import-file'),file=input?.files?.[0];let text='';if(file)text=file.size>65536?'__too_large__':await file.text();if(input)input.value='';dioxus.send(dialog?.open&&dialog.isConnected?text:'__closed__');}catch(_){dioxus.send('__read_error__');}";
                let text = document::eval(script).recv::<String>().await;
                if !session.get() || epoch.get() != generation {
                    return;
                }
                let result = match text {
                    Ok(text) if text == "__closed__" => return,
                    Ok(text) if text == "__too_large__" => {
                        Err("主题 JSON 不能超过 64 KiB。".into())
                    }
                    Ok(text) if text == "__read_error__" => Err("无法读取主题文件。".into()),
                    Ok(text) if text.is_empty() => Err("请选择一个主题或偏好 JSON 文件。".into()),
                    Ok(text) => import_document(&text, *settings.peek(), &library.peek()),
                    Err(_) => Err("无法读取主题文件。".into()),
                };
                match result {
                    Ok((value, themes, message)) => {
                        library.set(themes);
                        settings.set(value);
                        error.set(String::new());
                        notice.set(message.into());
                    }
                    Err(message) => {
                        notice.set(String::new());
                        error.set(message);
                    }
                }
            });
        }
    };
    let cards: Vec<(Appearance, Theme)> = Appearance::all()
        .into_iter()
        .map(|appearance| (appearance, library.read().resolve(appearance)))
        .chain(library.read().0.iter().enumerate().map(|(slot, theme)| {
            (
                Appearance::Custom(u8::try_from(slot).expect("theme slots are limited to 12")),
                theme.clone(),
            )
        }))
        .collect();
    let mut cancel_header = cancel.clone();
    rsx! {
        style { {include_str!("settings.css")} }
        crate::components::Modal {
            id: "appearance", title: "外观与主题", class: "appearance-modal",
            onclose: move |()| cancel_header(),
            div { class: "modal-body",
                p { class: "theme-settings-intro", "主题统一控制整个工作台。立即预览主题、明暗模式与密度，保存后保留到本设备。支持完整主题 JSON 与外观偏好 JSON。" }
                div { class: "theme-preset-grid",
                    for (appearance, theme) in cards {
                        button {
                            key: "{theme.id}", class: if settings.read().theme == appearance { "theme-preset selected" } else { "theme-preset" },
                            aria_pressed: settings.read().theme == appearance, disabled: *saving.read(),
                            onclick: move |_| { settings.write().theme = appearance; error.set(String::new()); notice.set(String::new()); },
                            span { class: "theme-mini", style: preview_style(&theme), "data-mode": settings.read().mode,
                                i { class: "theme-mini-sidebar", b {} b {} b {} }
                                i { class: "theme-mini-content", b {} em {} em {} span {} }
                            }
                            span { class: "theme-preset-label", strong { "{theme.name}" } if settings.read().theme == appearance { crate::components::Icon { name: "check" } } }
                            small { "{theme.description}" }
                        }
                    }
                }
                div { class: "theme-setting-fields",
                    div { class: "form-field", label { r#for: "color-mode", "明暗模式" }
                        select { id: "color-mode", value: settings.read().mode, disabled: *saving.read(), onchange: move |event| { settings.write().mode = mode(Some(&event.value())).unwrap_or("light"); error.set(String::new()); },
                            option { value: "light", "浅色" } option { value: "dark", "深色" } option { value: "system", "跟随系统" }
                        }
                    }
                    div { class: "form-field", label { r#for: "density", "界面密度" }
                        select { id: "density", value: settings.read().density, disabled: *saving.read(), onchange: move |event| { settings.write().density = density(Some(&event.value())).unwrap_or("standard"); error.set(String::new()); },
                            option { value: "standard", "标准 · 舒展阅读" } option { value: "compact", "紧凑 · 展示更多内容" }
                        }
                    }
                }
                div { class: "theme-contract-preview", span { "品牌强调" } span { "成功 / 新增" } span { "失败 / 删除" } span { "运行状态" } }
                div { class: "theme-portable",
                    for (full, label) in [(true,"导出主题 JSON"),(false,"导出偏好 JSON")] {
                        button { class: "button", onclick: move |_| {
                            let script = export_script(*settings.peek(), &library.peek(), full);
                            spawn(async move { if !matches!(document::eval(&script).recv::<bool>().await, Ok(true)) { error.set("无法导出 JSON 文件。".into()); } });
                        }, "{label}" }
                    }
                    button { class: "button", disabled: *saving.read(), onclick: move |_| { spawn(async move { let _ = document::eval("document.getElementById('appearance-import-file')?.click();dioxus.send(true);").recv::<bool>().await; }); }, "导入主题" }
                    input { id: "appearance-import-file", r#type: "file", accept: ".json,application/json", hidden: true, onchange: import }
                    span { "主题含完整配色 · 偏好引用主题 ID" }
                }
                p { class: "theme-import-notice", role: "status", "{notice}" }
                p { class: "theme-error", role: "alert", "{error}" }
                div { class: "modal-actions",
                    button { class: "secondary", onclick: move |_| cancel(), "取消" }
                    button { class: "button primary", disabled: *saving.read(), onclick: move |_| save(), if *saving.read() { "正在保存…" } else { "保存" } }
                }
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;
    #[test]
    fn legacy_preferences_still_restore() {
        let (settings, library) =
            restored(&json!({"theme":"ocean","mode":"dark","density":"compact"}));
        assert_eq!(
            settings,
            AppearanceSettings {
                theme: Appearance::Ocean,
                mode: "dark",
                density: "compact"
            }
        );
        assert_eq!(library.0.len(), 0);
    }

    #[test]
    fn custom_theme_round_trips_and_conflicts_are_atomic() {
        let mut theme = theme_contract::presets().remove(0);
        theme.id = "review-lime".into();
        theme.brand = Some("#135724".into());
        let document = theme_contract::theme_document(&theme, "system", "compact");
        let (settings, library, _) = import_document(
            &document.to_string(),
            AppearanceSettings::default(),
            &ThemeLibrary::default(),
        )
        .unwrap();
        assert_eq!(settings.mode, "system");
        assert_eq!(settings.density, "compact");
        assert_eq!(library.0[0], theme);
        let (_, same_library, _) =
            import_document(&document.to_string(), settings, &library).unwrap();
        assert_eq!(same_library.0.len(), 1);
        let mut conflicting = theme.clone();
        conflicting.name = "另一名称".into();
        let conflict = theme_contract::theme_document(&conflicting, "light", "standard");
        assert!(import_document(&conflict.to_string(), settings, &library).is_err());
        assert_eq!(library.0[0], theme);
        let (reloaded, reloaded_library) = restored(&preference_value(settings, &library));
        assert_eq!(reloaded, settings);
        assert_eq!(reloaded_library, library);
        let preference = json!({"schemaVersion":1,"kind":APPEARANCE_KIND,"preset":theme.id,"mode":"dark","density":"standard"});
        assert_eq!(
            parse_portable_appearance(&preference, &library)
                .unwrap()
                .theme,
            settings.theme
        );
    }

    #[test]
    fn capacity_builtin_ids_and_size_limits_are_enforced() {
        let mut library = ThemeLibrary::default();
        let base = theme_contract::presets().remove(0);
        for index in 0..12 {
            let mut theme = base.clone();
            theme.id = format!("review-{index}");
            assert_eq!(library.insert(theme).unwrap(), Appearance::Custom(index));
        }
        let mut overflow = base.clone();
        overflow.id = "review-overflow".into();
        let snapshot = library.clone();
        assert!(library.insert(overflow).is_err());
        assert_eq!(library, snapshot);
        assert_eq!(
            library.insert(library.0[0].clone()).unwrap(),
            Appearance::Custom(0)
        );
        let mut overwrite = base.clone();
        overwrite.name = "替代内置".into();
        assert!(library.insert(overwrite).is_err());
        assert_eq!(library.insert(base).unwrap(), Appearance::Paper);
        assert!(
            import_document(
                &" ".repeat(MAX_THEME_JSON_BYTES + 1),
                AppearanceSettings::default(),
                &library
            )
            .is_err()
        );
    }
}
