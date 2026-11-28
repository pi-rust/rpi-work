//! Local profiles consumed by the rpi-im-message extension.

use std::{env, fs, path::PathBuf};

use gpui::{ClipboardItem, Context, Div, FontWeight, Window, div, prelude::*, px};
use serde_json::{Map, Value, json};

use super::{Shell, session_storage};

pub(super) const PACKAGE: &str = "rpi-im-message";
const GUIDE: &str = "https://github.com/pi-rust/rpi-extension/tree/main/packages/rpi-im-message";

type Object = Map<String, Value>;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Platform {
    Feishu,
    Wecom,
}

impl Platform {
    const ALL: [Self; 2] = [Self::Feishu, Self::Wecom];

    fn title(self) -> &'static str {
        match self {
            Self::Feishu => "飞书",
            Self::Wecom => "企业微信",
        }
    }

    fn key(self) -> &'static str {
        match self {
            Self::Feishu => "feishu",
            Self::Wecom => "wecom",
        }
    }

    fn id_key(self) -> &'static str {
        match self {
            Self::Feishu => "appId",
            Self::Wecom => "botId",
        }
    }

    fn secret_key(self) -> &'static str {
        match self {
            Self::Feishu => "appSecret",
            Self::Wecom => "botSecret",
        }
    }

    fn instructions(self) -> &'static str {
        match self {
            Self::Feishu => {
                "在飞书开放平台启用应用机器人，使用长连接订阅 im.message.receive_v1，并开通消息权限。"
            }
            Self::Wecom => "在企业微信中创建智能机器人，选择长连接方式，获取 Bot ID 和 Secret。",
        }
    }
}

#[derive(Clone, Copy, PartialEq, Eq)]
enum Field {
    Id,
    Secret,
    Chats,
}

impl Field {
    const ALL: [Self; 3] = [Self::Id, Self::Secret, Self::Chats];

    fn label(self, platform: Platform) -> &'static str {
        match (self, platform) {
            (Self::Id, Platform::Feishu) => "App ID",
            (Self::Id, Platform::Wecom) => "Bot ID",
            (Self::Secret, Platform::Feishu) => "App Secret",
            (Self::Secret, Platform::Wecom) => "Bot Secret",
            (Self::Chats, _) => "允许的会话 ID",
        }
    }
}

struct Draft {
    platform: Platform,
    name: String,
    original: Option<Value>,
    profile: Object,
    active: Option<Field>,
}

impl Draft {
    fn has_credential(&self, key: &str) -> bool {
        [key, &format!("{key}Env")].iter().any(|key| {
            self.profile
                .get(*key)
                .and_then(Value::as_str)
                .is_some_and(|s| !s.trim().is_empty())
        })
    }

    fn value(&self, field: Field) -> String {
        match field {
            Field::Id => self
                .profile
                .get(self.platform.id_key())
                .and_then(Value::as_str)
                .unwrap_or_default()
                .into(),
            Field::Secret => String::new(),
            Field::Chats => self
                .profile
                .get("allowChats")
                .and_then(Value::as_array)
                .into_iter()
                .flatten()
                .filter_map(Value::as_str)
                .collect::<Vec<_>>()
                .join(", "),
        }
    }

    fn set(&mut self, field: Field, input: &str) -> Result<(), &'static str> {
        if input.chars().any(char::is_control) || input.len() > 16 * 1024 {
            return Err("输入过长或包含控制字符。");
        }
        let input = input.trim();
        if field == Field::Chats {
            let mut chats: Vec<&str> = Vec::new();
            for id in input.split([',', '，', ' ']).filter(|id| !id.is_empty()) {
                if !chats.contains(&id) {
                    chats.push(id);
                }
            }
            if chats.len() > 256 {
                return Err("最多可填写 256 个会话 ID。");
            }
            self.profile.insert("allowChats".into(), json!(chats));
            return Ok(());
        }
        let key = match field {
            Field::Id => self.platform.id_key(),
            Field::Secret => self.platform.secret_key(),
            Field::Chats => unreachable!(),
        };
        if !input.is_empty() {
            self.profile.insert(key.into(), json!(input));
            self.profile.remove(&format!("{key}Env"));
        }
        Ok(())
    }
}

#[derive(Default)]
pub(super) struct Panel {
    config: Option<Object>,
    editor: Option<Draft>,
    notice: Option<(&'static str, bool)>,
}

impl Panel {
    pub(super) fn editing(&self) -> bool {
        self.editor.is_some()
    }

    pub(super) fn masking_credential(&self) -> bool {
        self.editor
            .as_ref()
            .is_some_and(|draft| draft.active == Some(Field::Secret))
    }

    pub(super) fn field_label(&self) -> Option<&'static str> {
        let editor = self.editor.as_ref()?;
        Some(editor.active?.label(editor.platform))
    }
}

fn config_path() -> Result<PathBuf, &'static str> {
    // Match the extension's fallback path. Project files and overrides may be shared.
    env::var_os("USERPROFILE")
        .or_else(|| env::var_os("HOME"))
        .map(PathBuf::from)
        .filter(|home| home.is_absolute())
        .map(|home| home.join(".rpi").join("agent").join("im.json"))
        .ok_or("找不到本机用户配置目录。")
}

fn load(path: &std::path::Path) -> Result<Object, &'static str> {
    match session_storage::read(path) {
        Ok(None) => Ok(Object::from_iter([("profiles".into(), json!({}))])),
        Ok(Some(Value::Object(config))) if config.get("profiles").is_some_and(Value::is_object) => {
            Ok(config)
        }
        _ => Err("无法读取机器人配置。请检查 im.json 的格式和访问权限。"),
    }
}

fn save(path: &std::path::Path, draft: &Draft) -> Result<Object, &'static str> {
    if fs::symlink_metadata(path).is_ok_and(|meta| meta.file_type().is_symlink()) {
        return Err("im.json 是符号链接。请在原配置文件中编辑。");
    }
    let mut config = load(path)?;
    let profiles = config
        .get_mut("profiles")
        .and_then(Value::as_object_mut)
        .expect("validated profiles object");
    if profiles.get(&draft.name) != draft.original.as_ref() {
        return Err("这份机器人配置已在别处修改。取消编辑并刷新后再试。");
    }
    profiles.insert(draft.name.clone(), Value::Object(draft.profile.clone()));
    if !config.contains_key("defaultProfile") {
        config.insert("defaultProfile".into(), json!(draft.name));
    }
    session_storage::write(path, &Value::Object(config.clone()))
        .map_err(|_| "无法保存 im.json。请检查目录访问权限。")?;
    Ok(config)
}

impl Shell {
    pub(super) fn read_chat_integrations(&mut self, cx: &mut Context<Self>) {
        self.read_resource_catalog(cx);
        if !self.node.as_ref().is_some_and(crate::data::Node::is_local) {
            return;
        }
        let panel = &mut self.chat_integrations;
        match config_path().and_then(|path| load(&path)) {
            Ok(config) => {
                panel.config = Some(config);
                panel.notice = None;
            }
            Err(error) => {
                panel.config = None;
                panel.notice = Some((error, true));
            }
        }
        cx.notify();
    }

    pub(super) fn cancel_chat_edit(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        self.chat_integrations.editor = None;
        self.chat_integrations.notice = None;
        self.search.clear();
        self.focus_settings_title(window);
        cx.notify();
    }

    fn open_chat_edit(
        &mut self,
        platform: Platform,
        selected: Option<String>,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some(profiles) = self
            .chat_integrations
            .config
            .as_ref()
            .and_then(|config| config.get("profiles"))
            .and_then(Value::as_object)
        else {
            return;
        };
        let name = selected.unwrap_or_else(|| {
            (1..=profiles.len() + 1)
                .map(|index| {
                    if index == 1 {
                        format!("{}-main", platform.key())
                    } else {
                        format!("{}-{index}", platform.key())
                    }
                })
                .find(|name| !profiles.contains_key(name))
                .expect("available profile name")
        });
        let original = profiles.get(&name).cloned();
        let profile = original
            .as_ref()
            .and_then(Value::as_object)
            .cloned()
            .unwrap_or_else(|| {
                Object::from_iter([
                    ("provider".into(), json!(platform.key())),
                    ("transport".into(), json!("long_connection")),
                ])
            });
        self.chat_integrations.editor = Some(Draft {
            platform,
            name,
            original,
            profile,
            active: None,
        });
        self.chat_integrations.notice = None;
        self.search.clear();
        self.show_settings_form();
        self.focus_settings_title(window);
        cx.notify();
    }

    fn commit_chat_field(&mut self) -> bool {
        let Some(draft) = self.chat_integrations.editor.as_mut() else {
            return false;
        };
        let Some(field) = draft.active else {
            return true;
        };
        if !self.search.buffer.is_committed() {
            return false;
        }
        if let Err(error) = draft.set(field, &self.search.buffer.text) {
            self.chat_integrations.notice = Some((error, true));
            return false;
        }
        draft.active = None;
        self.search.clear();
        self.chat_integrations.notice = None;
        true
    }

    pub(super) fn finish_chat_field(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_chat_field() {
            self.focus_settings_title(window);
        }
        cx.notify();
    }

    fn edit_chat_field(&mut self, field: Field, window: &mut Window, cx: &mut Context<Self>) {
        if self.commit_chat_field()
            && let Some(draft) = self.chat_integrations.editor.as_mut()
        {
            draft.active = Some(field);
            self.search.buffer.replace(None, &draft.value(field));
            self.search.buffer.select_all();
            self.search.focus.focus(window);
        }
        cx.notify();
    }

    fn save_chat_profile(&mut self, window: &mut Window, cx: &mut Context<Self>) {
        if !self.commit_chat_field() {
            cx.notify();
            return;
        }
        let panel = &mut self.chat_integrations;
        let Some(draft) = panel.editor.as_ref() else {
            return;
        };
        let result = if draft.has_credential(draft.platform.id_key())
            && draft.has_credential(draft.platform.secret_key())
        {
            config_path().and_then(|path| save(&path, draft))
        } else {
            Err("请填写机器人的 ID 和密钥。")
        };
        match result {
            Ok(config) => {
                panel.config = Some(config);
                panel.editor = None;
                panel.notice = Some(("配置已保存。请用对应方案的启动命令连接机器人。", false));
                self.focus_settings_title(window);
            }
            Err(error) => panel.notice = Some((error, true)),
        }
        cx.notify();
    }

    pub(super) fn chat_integration_settings(&self, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let panel = &self.chat_integrations;
        let mut body =
            div()
                .flex()
                .flex_col()
                .gap_3()
                .when_some(panel.notice, |body, (notice, error)| {
                    body.child(self.text(
                        notice,
                        13.0,
                        if error { palette.danger } else { palette.text },
                    ))
                });
        if let Some(draft) = &panel.editor {
            body = body
                .child(
                    self.text(
                        format!("{} · {}", draft.platform.title(), draft.name),
                        16.0,
                        palette.text,
                    )
                    .font_weight(FontWeight::MEDIUM),
                )
                .child(self.text(draft.platform.instructions(), 13.0, palette.muted))
                .children(Field::ALL.map(|field| self.chat_field(draft, field, cx)))
                .child(self.text(
                    "会话 ID 用逗号分隔，留空接受所有可向机器人发消息的会话。",
                    12.0,
                    palette.muted,
                ))
                .child(
                    div()
                        .flex()
                        .items_center()
                        .justify_between()
                        .gap_2()
                        .child(self.text("自动回复", 14.0, palette.text))
                        .child(
                            self.setting_switch(
                                "chat-auto-reply",
                                draft
                                    .profile
                                    .get("autoReply")
                                    .and_then(Value::as_bool)
                                    .unwrap_or(false),
                            )
                            .on_click(cx.listener(
                                |shell, _, _, cx| {
                                    if let Some(draft) = shell.chat_integrations.editor.as_mut() {
                                        let enabled = draft
                                            .profile
                                            .get("autoReply")
                                            .and_then(Value::as_bool)
                                            .unwrap_or(false);
                                        draft.profile.insert("autoReply".into(), json!(!enabled));
                                        cx.notify();
                                    }
                                },
                            )),
                        ),
                )
                .child(
                    div()
                        .flex()
                        .flex_wrap()
                        .gap_2()
                        .child(self.setting_choice("chat-save", "保存配置", true).on_click(
                            cx.listener(|shell, _, window, cx| shell.save_chat_profile(window, cx)),
                        ))
                        .child(self.setting_choice("chat-cancel", "取消", false).on_click(
                            cx.listener(|shell, _, window, cx| shell.cancel_chat_edit(window, cx)),
                        )),
                );
            return body;
        }
        let local = self.node.as_ref().is_some_and(crate::data::Node::is_local);
        body = body
            .child(self.text(
                format!("{PACKAGE} · {}", self.extension_status(PACKAGE)),
                13.0,
                palette.text,
            ))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .when(!self.confirmed_extension(PACKAGE), |row| {
                        row.child(
                            self.setting_choice("chat-copy-install", "复制安装命令", false)
                                .on_click(|_, _, cx| {
                                    cx.write_to_clipboard(ClipboardItem::new_string(
                                        "rpi install rpi-im-message".into(),
                                    ))
                                }),
                        )
                    })
                    .child(
                        self.setting_choice("chat-guide", "接入说明", false)
                            .on_click(|_, _, cx| cx.open_url(GUIDE)),
                    )
                    .child(
                        self.setting_choice("chat-refresh", "刷新状态", false)
                            .on_click(
                                cx.listener(|shell, _, _, cx| shell.read_chat_integrations(cx)),
                            ),
                    ),
            );
        if !local {
            return body.child(self.text(
                "请在运行机器人的电脑上打开本机工作台填写配置。",
                13.0,
                palette.muted,
            ));
        }
        let profiles = panel
            .config
            .as_ref()
            .and_then(|config| config.get("profiles"))
            .and_then(Value::as_object);
        body.child(self.text(
            "配置和密钥写入本机 ~/.rpi/agent/im.json。启动时，RPI_IM_CONFIG 和项目 .rpi/im.json 优先。",
            13.0,
            palette.muted,
        ))
        .children(Platform::ALL.map(|platform| {
            let matching = profiles.into_iter().flatten().filter(|(_, profile)| {
                profile.is_object()
                    && match profile.get("provider") {
                        Some(provider) => provider.as_str() == Some(platform.key()),
                        None => platform == Platform::Feishu,
                    }
            });
            let add = self
                .setting_choice(("chat-add", platform as usize), "添加机器人", false)
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.open_chat_edit(platform, None, window, cx);
                }));
            div()
                .id(platform.key())
                .flex()
                .flex_col()
                .gap_2()
                .border_t_1()
                .border_color(palette.line)
                .pt_3()
                .child(
                    self.text(platform.title(), 15.0, palette.text)
                        .font_weight(FontWeight::MEDIUM),
                )
                .child(self.text(platform.instructions(), 13.0, palette.muted))
                .children(matching.enumerate().map(|(index, (name, _))| {
                    self.chat_profile_row(platform, index, name, cx)
                }))
                .when(profiles.is_some(), |card| card.child(div().child(add)))
        }))
    }

    fn chat_profile_row(
        &self,
        platform: Platform,
        index: usize,
        name: &str,
        cx: &Context<Self>,
    ) -> Div {
        let palette = self.palette;
        let profile = name.to_owned();
        let safe_name = !name.is_empty()
            && !name.starts_with('-')
            && name
                .bytes()
                .all(|byte| byte.is_ascii_alphanumeric() || b"-_".contains(&byte));
        let command = format!("rpi --im-message-server --im-profile {name}");
        div()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.text(profile.clone(), 13.0, palette.text))
            .child(
                div()
                    .flex()
                    .flex_wrap()
                    .gap_2()
                    .child(
                        self.setting_choice(("chat-edit", index), "编辑", false)
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.open_chat_edit(platform, Some(profile.clone()), window, cx);
                            })),
                    )
                    .when(safe_name, |row| {
                        row.child(
                            self.setting_choice(
                                ("chat-start-command", index),
                                "复制启动命令",
                                false,
                            )
                            .on_click(move |_, _, cx| {
                                cx.write_to_clipboard(ClipboardItem::new_string(command.clone()))
                            }),
                        )
                    }),
            )
    }

    fn chat_field(&self, draft: &Draft, field: Field, cx: &mut Context<Self>) -> Div {
        let palette = self.palette;
        let label = field.label(draft.platform);
        let row = div()
            .flex()
            .flex_col()
            .gap_1()
            .child(self.text(label, 13.0, palette.text));
        if draft.active == Some(field) {
            return row.child(
                div()
                    .flex()
                    .items_center()
                    .gap_2()
                    .child(
                        div()
                            .flex()
                            .items_center()
                            .flex_1()
                            .min_w_0()
                            .h(px(44.0))
                            .px_2()
                            .rounded(px(6.0))
                            .border_1()
                            .border_color(palette.strong_line)
                            .child(self.search_input_element(cx)),
                    )
                    .child(
                        self.setting_choice("chat-field-done", "完成", false)
                            .on_click(cx.listener(|shell, _, window, cx| {
                                shell.finish_chat_field(window, cx)
                            })),
                    ),
            );
        }
        let value = draft.value(field);
        let summary = match field {
            Field::Secret if draft.has_credential(draft.platform.secret_key()) => {
                "已配置 · 点击替换"
            }
            Field::Id if value.is_empty() && draft.has_credential(draft.platform.id_key()) => {
                "已使用环境变量 · 点击替换"
            }
            _ if value.is_empty() => "点击填写",
            _ => &value,
        };
        row.child(
            self.button(label, "")
                .h(px(44.0))
                .justify_start()
                .min_w_0()
                .tab_index(0)
                .border_1()
                .border_color(palette.line)
                .focus(|button| button.border_color(palette.strong_line))
                .child(self.text(summary.to_owned(), 13.0, palette.text).truncate())
                .on_click(cx.listener(move |shell, _, window, cx| {
                    shell.edit_chat_field(field, window, cx)
                })),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn profile_save_merges_unrelated_changes_and_rejects_stale_edits() {
        let directory = tempfile::tempdir().unwrap();
        let path = directory.path().join("im.json");
        let original =
            json!({"appIdEnv": "APP_ID", "appSecretEnv": "APP_SECRET", "domain": "lark"});
        let mut draft = Draft {
            platform: Platform::Feishu,
            name: "existing".into(),
            original: Some(original.clone()),
            profile: original.as_object().unwrap().clone(),
            active: None,
        };
        let initial = json!({"defaultProfile": "other", "profiles": {
            "existing": original,
            "other": {"provider": "wecom", "botIdEnv": "BOT_ID"}
        }});
        session_storage::write(&path, &initial).unwrap();
        draft.set(Field::Chats, "oc_one, oc_two, oc_one").unwrap();
        let saved = save(&path, &draft).unwrap();
        assert_eq!(saved["profiles"]["other"], initial["profiles"]["other"]);
        assert_eq!(saved["defaultProfile"], "other");
        let profile = &saved["profiles"]["existing"];
        assert_eq!(profile["domain"], "lark");
        assert_eq!(profile["appSecretEnv"], "APP_SECRET");
        assert_eq!(profile["allowChats"], json!(["oc_one", "oc_two"]));
        assert!(save(&path, &draft).is_err());
        assert_eq!(load(&path).unwrap(), saved);
    }
}
