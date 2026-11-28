use std::ops::Range;

use gpui::{AnyElement, Context, IntoElement, ScrollHandle, Window, div, prelude::*, px};

use super::{DraftKey, Shell};
use crate::icons::icon;

#[derive(Clone, Copy, PartialEq, Eq)]
enum Command {
    New,
    Status,
    Review,
    Template(usize),
}

type CommandOption<'a> = (Command, &'a str, &'a str, &'static str);

impl Command {
    const OPTIONS: [CommandOption<'static>; 3] = [
        (Self::New, "/new", "新对话", "compose"),
        (Self::Status, "/status", "查看会话上下文", "message-circle"),
        (Self::Review, "/review", "审查当前改动", "file-diff"),
    ];
}

pub(super) fn is_builtin(token: &str) -> bool {
    Command::OPTIONS.iter().any(|option| option.1 == token)
}

#[derive(Default)]
pub(super) struct Menu {
    selected: usize,
    scroll: ScrollHandle,
    draft: Option<(DraftKey, u64)>,
    dismissed: bool,
    pub(super) add: Option<(DraftKey, u64)>,
}

#[derive(Clone, Copy)]
enum Add {
    Attachment,
    FileReference,
}

impl Add {
    const OPTIONS: [(Self, &str, &str); 2] = [
        (Self::Attachment, "添加附件", "paperclip"),
        (Self::FileReference, "引用项目文件", "folder"),
    ];
}

fn command_query(text: &str, head: usize) -> Option<(Range<usize>, &str)> {
    let prefix = text.get(..head)?;
    let token = prefix.trim_start();
    let name = token.strip_prefix('/')?;
    if !name
        .bytes()
        .all(|byte| byte.is_ascii_alphanumeric() || byte == b'-')
        || !text[head..].trim().is_empty()
    {
        return None;
    }
    Some((head - token.len()..head, name))
}

impl Shell {
    fn command_draft(&self) -> Option<(DraftKey, u64, Range<usize>, &str)> {
        if !self.navigation_keyboard_enabled() || self.overlay.is_some() {
            return None;
        }
        let key = self.draft_key();
        let input = &self.drafts.get(&key)?.input;
        let (text, head) = input.completion_text()?;
        let (range, name) = command_query(text, head)?;
        Some((key, input.revision, range, name))
    }

    pub(super) fn sync_composer_commands(&mut self, window: &Window) {
        let draft = self
            .focus
            .is_focused(window)
            .then(|| self.command_draft())
            .flatten()
            .map(|(key, rev, _, _)| (key, rev));
        if draft != self.composer_commands.draft {
            self.composer_commands.draft = draft;
            self.composer_commands.selected = 0;
            self.composer_commands.scroll = ScrollHandle::new();
            self.composer_commands.dismissed = false;
        }
        if !self.focus.is_focused(window)
            || self.composer_commands.add != Some(self.composer_revision())
        {
            self.composer_commands.add = None;
        }
    }

    fn composer_revision(&self) -> (DraftKey, u64) {
        let key = self.draft_key();
        (
            key,
            self.drafts
                .get(&key)
                .map_or(0, |draft| draft.input.revision),
        )
    }

    fn add_options(&self) -> Vec<(Add, &'static str, &'static str)> {
        if !self.navigation_keyboard_enabled() || self.composer_is_marked() {
            return Vec::new();
        }
        Add::OPTIONS
            .into_iter()
            .filter(|(action, _, _)| match action {
                Add::Attachment => {
                    self.supports_attachments()
                        && !self.selecting_attachments.contains(&self.draft_key())
                }
                Add::FileReference => self.node.is_some() && self.project.is_some(),
            })
            .collect()
    }

    pub(super) fn composer_add_button(&self, cx: &Context<Self>) -> impl IntoElement {
        let available = !self.add_options().is_empty();
        self.navigation_tooltip(
            self.navigation_control(
                self.icon_button("composer-add", "plus", 28.0),
                available && self.overlay.is_none(),
            )
            .when(!available, |button| button.opacity(0.4).cursor_default())
            .when(available, |button| {
                button.on_click(cx.listener(|shell, _, window, cx| {
                    shell.composer_config.close_menu();
                    shell.focus_composer(window);
                    shell.composer_commands.add = shell
                        .composer_commands
                        .add
                        .is_none()
                        .then(|| shell.composer_revision());
                    shell.composer_commands.selected = 0;
                    cx.notify();
                }))
            }),
            "添加到对话",
        )
    }

    fn run_add_action(&mut self, action: Add, window: &mut Window, cx: &mut Context<Self>) {
        self.composer_commands.add = None;
        match action {
            Add::Attachment => self.pick_attachments(cx),
            Add::FileReference => {
                let key = self.draft_key();
                let input = &mut self.drafts.entry(key).or_default().input;
                let end = input.text.len();
                let token = if end == 0 || input.text.ends_with(char::is_whitespace) {
                    "@"
                } else {
                    " @"
                };
                input.replace_completion(end..end, token);
                self.focus_composer(window);
            }
        }
        cx.notify();
    }

    fn command_options(&self) -> Option<(Range<usize>, Vec<CommandOption<'_>>)> {
        let (key, rev, range, name) = self.command_draft()?;
        if self.composer_commands.add.is_some()
            || self.composer_commands.draft != Some((key, rev))
            || self.composer_commands.dismissed
        {
            return None;
        }
        let name = name.to_ascii_lowercase();
        let templates = self
            .desktop_plugins
            .prompts()
            .enumerate()
            .map(|(index, prompt)| {
                (
                    Command::Template(index),
                    prompt.token.as_str(),
                    prompt.label.as_str(),
                    "blocks",
                )
            });
        let options = Command::OPTIONS
            .into_iter()
            .chain(templates)
            .filter(|(command, token, _, _)| {
                token[1..].starts_with(&name)
                    && match command {
                        Command::New => self.project.is_some(),
                        Command::Status => self.session.is_some(),
                        Command::Review => self.can_run_review(),
                        Command::Template(_) => true,
                    }
            })
            .collect::<Vec<_>>();
        (!options.is_empty()).then_some((range, options))
    }

    fn run_composer_command(
        &mut self,
        command: Command,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) {
        let Some((range, options)) = self.command_options() else {
            return;
        };
        if !options.iter().any(|option| option.0 == command) {
            return;
        }
        let replacement = match command {
            Command::Template(index) => {
                let Some(prompt) = self.desktop_plugins.prompts().nth(index) else {
                    return;
                };
                prompt.text.as_str()
            }
            _ => "",
        };
        let key = self.draft_key();
        let input = &mut self.drafts.entry(key).or_default().input;
        input.replace_completion(range, replacement);
        self.composer_commands = Menu::default();
        match command {
            Command::New => self.open_new_conversation(window, cx),
            Command::Status => self.open_session_context(window, cx),
            Command::Review => self.run_review(cx),
            Command::Template(_) => {
                self.composer_commands.draft = Some((key, input.revision));
                self.composer_commands.dismissed = true;
                self.focus_composer(window);
            }
        }
        cx.notify();
    }

    pub(super) fn composer_command_key(
        &mut self,
        key: &str,
        window: &mut Window,
        cx: &mut Context<Self>,
    ) -> bool {
        if self.composer_commands.add.is_some() {
            let options = self.add_options();
            let len = options.len();
            let index = self.composer_commands.selected.min(len.saturating_sub(1));
            match key {
                "up" if len > 0 => self.composer_commands.selected = (index + len - 1) % len,
                "down" if len > 0 => self.composer_commands.selected = (index + 1) % len,
                "tab" | "enter" if len > 0 => self.run_add_action(options[index].0, window, cx),
                "escape" => self.composer_commands.add = None,
                _ => return false,
            }
            cx.notify();
            return true;
        }
        let Some((_, options)) = self.command_options() else {
            return false;
        };
        let len = options.len();
        let index = self.composer_commands.selected.min(len - 1);
        match key {
            "up" | "down" => {
                let step = if key == "up" { len - 1 } else { 1 };
                let selected = (index + step) % len;
                self.composer_commands.selected = selected;
                self.composer_commands.scroll.scroll_to_item(selected);
            }
            "tab" | "enter" => self.run_composer_command(options[index].0, window, cx),
            "escape" => self.composer_commands.dismissed = true,
            _ => return false,
        }
        cx.notify();
        true
    }

    pub(super) fn composer_command_menu(&self, cx: &Context<Self>) -> Option<AnyElement> {
        let palette = self.palette;
        if self.composer_commands.add.is_some() {
            let options = self.add_options();
            if options.is_empty() {
                return None;
            }
            let selected = self.composer_commands.selected.min(options.len() - 1);
            return Some(
                div()
                    .flex()
                    .flex_col()
                    .my_1()
                    .rounded(px(8.0))
                    .border_1()
                    .border_color(palette.line)
                    .p_1()
                    .children(options.into_iter().enumerate().map(
                        |(index, (action, label, shape))| {
                            self.button(("composer-add-option", index), "")
                                .justify_start()
                                .gap_2()
                                .when(index == selected, |button| button.bg(palette.subtle))
                                .child(icon(shape, 14.0, palette.muted))
                                .child(self.text(label, 12.0, palette.text))
                                .on_click(cx.listener(move |shell, _, window, cx| {
                                    shell.run_add_action(action, window, cx)
                                }))
                        },
                    ))
                    .into_any_element(),
            );
        }
        let (_, options) = self.command_options()?;
        let selected = self.composer_commands.selected.min(options.len() - 1);
        Some(
            div()
                .id("composer-command-options")
                .flex()
                .flex_col()
                .flex_none()
                .max_h(px(152.0))
                .overflow_y_scroll()
                .track_scroll(&self.composer_commands.scroll)
                .my_1()
                .rounded(px(8.0))
                .border_1()
                .border_color(palette.line)
                .p_1()
                .children(options.into_iter().enumerate().map(
                    |(index, (command, token, label, shape))| {
                        self.button(("composer-command", index), "")
                            .justify_start()
                            .gap_2()
                            .when(index == selected, |button| button.bg(palette.subtle))
                            .flex_none()
                            .child(icon(shape, 14.0, palette.muted).flex_none())
                            .child(self.text(token.to_owned(), 12.0, palette.text).flex_none())
                            .child(
                                self.text(label.to_owned(), 12.0, palette.muted)
                                    .min_w_0()
                                    .truncate(),
                            )
                            .on_click(cx.listener(move |shell, _, window, cx| {
                                shell.run_composer_command(command, window, cx)
                            }))
                    },
                ))
                .into_any_element(),
        )
    }
}

#[cfg(test)]
mod tests {
    use super::command_query;

    #[test]
    fn commands_only_suggest_for_a_standalone_token() {
        assert_eq!(command_query("  /stat", 7), Some((2..7, "stat")));
        assert_eq!(command_query("/test-2", 7), Some((0..7, "test-2")));
        assert_eq!(command_query("hello /stat", 11), None);
        assert_eq!(command_query("/status please", 7), None);
        assert_eq!(command_query("/tmp/main.rs", 12), None);
    }
}
