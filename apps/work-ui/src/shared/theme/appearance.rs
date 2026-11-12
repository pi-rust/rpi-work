use crate::shared::app_settings::{self, AppSettingsState};
use dioxus::prelude::*;
use serde_json::{Value, json};
use std::{cell::Cell, fmt::Write as _, rc::Rc};

#[path = "theme_contract.rs"]
mod theme_contract;
use theme_contract::{MAX_CUSTOM_THEMES, MAX_THEME_JSON_BYTES, THEME_KIND, Theme, ToolRendering};

const APPEARANCE_KIND: &str = "rpi-work-appearance";
pub const STYLESHEETS: &str = concat!(
    include_str!("themes.css"),
    "\n",
    include_str!("codex.css"),
    "\n",
    include_str!("codex-components.css"),
    "\n",
    include_str!("theme-design.css"),
    "\n",
    include_str!("tool-presentation.css")
);

#[derive(Clone, Copy, Debug, PartialEq, Eq)]
pub enum Appearance {
    Codex,
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
            Self::Codex => "codex",
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
    pub const fn all() -> [Self; 5] {
        [
            Self::Codex,
            Self::Paper,
            Self::Ocean,
            Self::Graphite,
            Self::Reddit,
        ]
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
    pub tool_rendering: Option<ToolRendering>,
}
impl Default for AppearanceSettings {
    fn default() -> Self {
        Self {
            theme: Appearance::Codex,
            mode: "light",
            density: "standard",
            tool_rendering: None,
        }
    }
}

fn effective_tool_rendering(settings: AppearanceSettings, theme: &Theme) -> ToolRendering {
    settings
        .tool_rendering
        .unwrap_or_else(|| theme.effective_design().tool_rendering)
}

fn with_tool_rendering(mut value: Value, key: &str, mode: Option<ToolRendering>) -> Value {
    if let Some(mode) = mode {
        value[key] = json!(mode);
    }
    value
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
                    .unwrap_or("codex"),
            )
            .unwrap_or(Appearance::Codex),
        mode: mode(value.get("mode").and_then(Value::as_str)).unwrap_or("light"),
        density: density(value.get("density").and_then(Value::as_str)).unwrap_or("standard"),
        tool_rendering: ToolRendering::parse_optional(value.get("toolRendering"))
            .unwrap_or_default(),
    };
    (settings, library)
}

fn preference_value(settings: AppearanceSettings, library: &ThemeLibrary) -> Value {
    with_tool_rendering(
        json!({"theme":library.resolve(settings.theme).id,"mode":settings.mode,"density":settings.density,"customThemes":library.0.iter().map(theme_contract::theme_value).collect::<Vec<_>>()}),
        "toolRendering",
        settings.tool_rendering,
    )
}
fn parse_portable_appearance(
    value: &Value,
    library: &ThemeLibrary,
) -> Result<AppearanceSettings, String> {
    let object = value.as_object().ok_or("导入文件必须是 JSON 对象。")?;
    let fields = ["schemaVersion", "kind", "preset", "mode", "density"];
    if fields.iter().any(|key| !object.contains_key(*key))
        || object
            .keys()
            .any(|key| !fields.contains(&key.as_str()) && key != "toolRendering")
    {
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
        tool_rendering: ToolRendering::parse_optional(object.get("toolRendering"))?,
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
                    tool_rendering: ToolRendering::parse_optional(
                        value.get("preferredToolRendering"),
                    )?,
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
        with_tool_rendering(
            theme_contract::theme_document(&theme, settings.mode, settings.density),
            "preferredToolRendering",
            settings.tool_rendering,
        )
    } else {
        with_tool_rendering(
            json!({"schemaVersion":1,"kind":APPEARANCE_KIND,"preset":theme.id,"mode":settings.mode,"density":settings.density}),
            "toolRendering",
            settings.tool_rendering,
        )
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
    let layout_keys: Vec<_> = theme
        .effective_design()
        .tokens(settings.density == "compact")
        .into_keys()
        .collect();
    let payload = json!({"id":theme.id,"mode":settings.mode,"density":settings.density,"toolRendering":effective_tool_rendering(settings, &theme),"custom":matches!(settings.theme,Appearance::Custom(_)),"layoutKeys":layout_keys,"light":light,"dark":dark});
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
            else for(const key of config.layoutKeys)root.style.setProperty('--'+key,tokens[key]);
            root.dataset.theme=config.id;root.dataset.mode=mode;root.dataset.colorMode=mode;
            root.dataset.appearance=config.mode;root.dataset.density=config.density;root.style.colorScheme=mode;
            root.dataset.toolRendering=config.toolRendering;
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
    let app_settings = try_use_context::<Signal<AppSettingsState>>();
    let initial = use_hook(move || {
        app_settings
            .and_then(|store| store.peek().value.get("appearance").cloned())
            .filter(Value::is_object)
            .map_or_else(
                || (AppearanceSettings::default(), ThemeLibrary::default()),
                |value| restored(&value),
            )
    });
    let mut settings = use_signal(|| initial.0);
    let mut library = use_signal(|| initial.1);
    use_context_provider(|| library);
    let ready = use_signal(|| true);
    let mut unified_synced = use_signal(|| false);
    use_effect(move || {
        let is_ready = *ready.read();
        let Some(app_settings) = app_settings else {
            return;
        };
        if is_ready && app_settings.read().ready && !*unified_synced.read() {
            let value = app_settings.read().value.get("appearance").cloned();
            if let Some(value) = value.filter(Value::is_object) {
                let (next, themes) = restored(&value);
                library.set(themes);
                settings.set(next);
            } else {
                let value = preference_value(*settings.peek(), &library.peek());
                if app_settings::update_namespace(app_settings, "appearance", value).is_some() {
                    app_settings::request_write(app_settings);
                }
            }
            unified_synced.set(true);
        }
    });
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
    let radius = theme.effective_design().geometry.radii[2];
    let _ = write!(style, "--mini-radius:{radius}px");
    style
}

#[component]
pub fn AppearancePicker(
    mut settings: Signal<AppearanceSettings>,
    onclose: EventHandler<()>,
    #[props(default)] embedded: bool,
) -> Element {
    let mut library = use_context::<Signal<ThemeLibrary>>();
    let snapshot = use_hook(|| (*settings.read(), library.read().clone()));
    let original = use_signal(|| snapshot);
    let mut error = use_signal(String::new);
    let mut notice = use_signal(String::new);
    let saving = use_signal(|| false);
    let app_settings = try_use_context::<Signal<AppSettingsState>>();
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
            let Some(app_settings) = app_settings else {
                error.set("全局设置尚未加载，无法保存外观偏好。".into());
                return;
            };
            if !app_settings.peek().ready {
                error.set("全局设置尚未加载，无法保存外观偏好。".into());
                return;
            }
            epoch.set(epoch.get() + 1);
            let value = *settings.peek();
            let themes = library.peek().clone();
            if app_settings::update_namespace(
                app_settings,
                "appearance",
                preference_value(value, &themes),
            )
            .is_none()
            {
                error.set("无法保存外观偏好；请先修复全局设置文件。".into());
                return;
            }
            app_settings::request_write(app_settings);
            saved.set(true);
            session.set(false);
            onclose.call(());
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
                let script = r"try{const dialog=document.getElementById('appearance'),input=document.getElementById('appearance-import-file'),file=input?.files?.[0];let text='';if(file)text=file.size>65536?'__too_large__':await file.text();if(input)input.value='';dioxus.send(dialog?.isConnected&&(dialog.tagName!=='DIALOG'||dialog.open)?text:'__closed__');}catch(_){dioxus.send('__read_error__');}";
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
    let visual = library
        .read()
        .resolve(settings.read().theme)
        .effective_design();
    let body_font = visual.typography.body_font.stack();
    let heading_font = visual.typography.heading_font.stack();
    let code_font = visual.typography.code_font.stack();
    let radii = format!("{:?}", visual.geometry.radii);
    let mut cancel_header = cancel.clone();
    rsx! {
        style { {include_str!("../../features/settings/settings.css")} }
        AppearanceSurface {
            embedded, onclose: move |()| cancel_header(),
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
                            span { class: "theme-preset-label", strong { "{theme.name}" } if settings.read().theme == appearance { crate::shared::components::Icon { name: "check" } } }
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
                    div { class: "form-field tool-rendering-field",
                        label { r#for: "tool-rendering", "工具展示" }
                        select {
                            id: "tool-rendering", value: settings.read().tool_rendering.map_or("theme", ToolRendering::as_str), disabled: *saving.read(),
                            onchange: move |event| {
                                settings.write().tool_rendering = match event.value().as_str() {
                                    "simple" => Some(ToolRendering::Simple),
                                    "detailed" => Some(ToolRendering::Detailed),
                                    _ => None,
                                };
                                error.set(String::new());
                            },
                            option { value: "theme", if visual.tool_rendering.is_simple() { "跟随主题 · 简单" } else { "跟随主题 · 详情" } }
                            option { value: "simple", "简单 · Codex 紧凑摘要" }
                            option { value: "detailed", "详情 · 当前完整卡片" }
                        }
                        small { "仅改变展示；简单模式也可展开完整参数与结果，不影响执行、确认和状态。" }
                    }
                }
                section { class: "theme-design-summary", aria_label: "主题视觉规格",
                    h3 { "视觉规格 · 不只是配色" }
                    dl {
                        div { dt { "正文字体" } dd { "{body_font}" } }
                        div { dt { "标题字体" } dd { "{heading_font}" } }
                        div { dt { "代码字体" } dd { "{code_font}" } }
                        div { dt { "字号 / 行高" } dd { "{visual.typography.scale}× · 正文 {visual.typography.body_size}px · 消息 {visual.typography.message_size}px / {visual.typography.message_line_height}" } }
                        div { dt { "间距 / 圆角" } dd { "{visual.geometry.spacing_scale}× · {radii}px" } }
                        div { dt { "控件 / 触控" } dd { "{visual.geometry.control_height}px / {visual.geometry.hit_size}px" } }
                        div { dt { "动效" } dd { "{visual.motion.fast} / {visual.motion.normal}ms" } }
                    }
                    p { "完整视觉规格使用主题 JSON v2；旧版主题仍可直接使用。字体仅使用本机安全字体栈，不下载外部资源。" }
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
            }
            div { class: "modal-actions",
                button { class: "secondary", onclick: move |_| cancel(), "取消" }
                button { class: "button primary", disabled: *saving.read(), onclick: move |_| save(), if *saving.read() { "正在保存…" } else { "保存" } }
            }
        }
    }
}

// Reuse the same preview/save/rollback lifecycle in the settings page and dialogs.
#[component]
fn AppearanceSurface(embedded: bool, onclose: EventHandler<()>, children: Element) -> Element {
    if embedded {
        rsx! {
            section { id: "appearance", class: "appearance-modal appearance-inline", aria_label: "外观与主题",
                div { class: "page-heading", h1 { "外观与主题" } }
                {children}
            }
        }
    } else {
        rsx! {
            crate::shared::components::Modal {
                id: "appearance", title: "外观与主题", class: "appearance-modal", onclose,
                {children}
            }
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn tool_rendering_defaults_follow_the_theme_and_overrides_round_trip() {
        let initial = AppearanceSettings::default();
        let library = ThemeLibrary::default();
        for preset in Appearance::all() {
            let (settings, _) = restored(&json!({"theme":preset.id()}));
            assert_eq!(settings.tool_rendering, None);
            assert_eq!(
                effective_tool_rendering(settings, &library.resolve(preset)),
                ToolRendering::Simple
            );
        }
        let mut theme = library.resolve(Appearance::Codex);
        theme.id = "tool-details".into();
        let mut design = theme.effective_design();
        design.tool_rendering = ToolRendering::Detailed;
        theme.design = Some(design);
        let document = theme_contract::theme_document(&theme, "light", "standard");
        let overridden = AppearanceSettings {
            tool_rendering: Some(ToolRendering::Simple),
            ..initial
        };
        let (settings, themes, _) =
            import_document(&document.to_string(), overridden, &library).unwrap();
        assert_eq!(settings.tool_rendering, None);
        assert_eq!(
            effective_tool_rendering(settings, &themes.resolve(settings.theme)),
            ToolRendering::Detailed
        );
        let preferred = with_tool_rendering(
            document.clone(),
            "preferredToolRendering",
            Some(ToolRendering::Simple),
        );
        let (settings, themes, _) =
            import_document(&preferred.to_string(), settings, &themes).unwrap();
        assert_eq!(
            effective_tool_rendering(settings, &themes.resolve(settings.theme)),
            ToolRendering::Simple
        );
        assert_eq!(
            restored(&preference_value(settings, &themes)),
            (settings, themes.clone())
        );
        let preference = with_tool_rendering(
            json!({"schemaVersion":1,"kind":APPEARANCE_KIND,"preset":theme.id,"mode":"dark","density":"compact"}),
            "toolRendering",
            Some(ToolRendering::Detailed),
        );
        let (settings, _, _) = import_document(&preference.to_string(), settings, &themes).unwrap();
        assert_eq!(settings.tool_rendering, Some(ToolRendering::Detailed));
        assert_eq!(settings.mode, "dark");
        let (settings, _, _) = import_document(&document.to_string(), settings, &themes).unwrap();
        assert_eq!(settings.tool_rendering, None);
        assert!(
            theme_contract::theme_document(
                &library.resolve(Appearance::Codex),
                "light",
                "standard"
            )
            .get("preferredToolRendering")
            .is_none()
        );
    }

    #[test]
    fn invalid_tool_rendering_imports_are_rejected_without_mutation() {
        let initial = AppearanceSettings::default();
        let library = ThemeLibrary::default();
        let theme = library.resolve(Appearance::Codex);
        for bad in [
            json!(null),
            json!(true),
            json!(1),
            json!("compact"),
            json!("simple;display:none"),
            json!({"mode":"simple"}),
        ] {
            let mut document = theme_contract::theme_document(&theme, "light", "standard");
            document["preferredToolRendering"] = bad.clone();
            assert!(import_document(&document.to_string(), initial, &library).is_err());
            let preference = json!({"schemaVersion":1,"kind":APPEARANCE_KIND,"preset":"codex","mode":"light","density":"standard","toolRendering":bad});
            assert!(import_document(&preference.to_string(), initial, &library).is_err());
            assert_eq!(
                restored(&json!({"toolRendering":bad})).0.tool_rendering,
                None
            );
        }
        assert_eq!(library, ThemeLibrary::default());
    }

    #[test]
    fn codex_is_default_but_existing_theme_choices_are_preserved() {
        assert_eq!(AppearanceSettings::default().theme, Appearance::Codex);
        assert_eq!(Appearance::all()[0], Appearance::Codex);
        for value in [Value::Null, json!({}), json!({"theme":"missing"})] {
            assert_eq!(restored(&value).0.theme, Appearance::Codex);
        }
        for theme in Appearance::all() {
            let (settings, _) =
                restored(&json!({"theme":theme.id(),"mode":"dark","density":"compact"}));
            assert_eq!(settings.theme, theme);
            assert_eq!(settings.mode, "dark");
            assert_eq!(settings.density, "compact");
        }
    }

    #[test]
    fn codex_portable_colors_and_builtin_css_match_in_both_modes() {
        let library = ThemeLibrary::default();
        let theme = library.resolve(Appearance::Codex);
        let portable = theme_contract::theme_document(&theme, "light", "standard");
        let (settings, restored_library, _) = import_document(
            &portable.to_string(),
            AppearanceSettings::default(),
            &library,
        )
        .unwrap();
        assert_eq!(settings.theme, Appearance::Codex);
        assert_eq!(restored_library, library);
        assert_eq!(
            include_str!("codex.css"),
            theme_contract::codex_stylesheet()
        );
        for dark in [false, true] {
            let tokens = theme_contract::build_tokens(&theme, dark, false);
            for surface in ["bg", "side", "panel", "card", "code", "selected-bg"] {
                for text in ["text", "muted", "body-text"] {
                    assert!(
                        theme_contract::contrast(&tokens[text], &tokens[surface]) >= 4.5,
                        "{text} on {surface} (dark={dark})"
                    );
                }
            }
            assert!(theme_contract::contrast(&tokens["accent"], &tokens["on-accent"]) >= 4.5);
        }
    }

    #[test]
    fn legacy_preferences_still_restore() {
        let (settings, library) =
            restored(&json!({"theme":"ocean","mode":"dark","density":"compact"}));
        assert_eq!(
            settings,
            AppearanceSettings {
                theme: Appearance::Ocean,
                mode: "dark",
                density: "compact",
                tool_rendering: None,
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
