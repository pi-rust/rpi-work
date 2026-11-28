//! Device-local GPUI plugins. Contributions are data consumed by the composer.

use std::{
    collections::HashSet,
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde_json::Value;

use super::{composer_commands::is_builtin, session_storage};

const MAX_FILES: usize = 32;
const MAX_BYTES: u64 = 64 * 1024;
const EXAMPLE_FILE: &str = "workbench-starter.json";
const EXAMPLE: &str = include_str!("../examples/workbench-starter.json");

#[derive(Default)]
pub(super) struct Catalog {
    pub directory: Option<PathBuf>,
    pub plugins: Vec<Plugin>,
    pub problems: Vec<String>,
}

pub(super) struct Plugin {
    id: String,
    pub name: String,
    pub prompts: Vec<Prompt>,
}

pub(super) struct Prompt {
    pub token: String,
    pub label: String,
    pub text: String,
}

impl Catalog {
    pub fn load_local() -> Self {
        session_storage::directory("gpui-plugins").map_or_else(Self::default, Self::load)
    }

    fn load(directory: PathBuf) -> Self {
        let mut catalog = Self::default();
        match manifests(&directory) {
            Ok(paths) => catalog.read_plugins(paths),
            Err(error) => catalog.problems.push(format!("无法读取插件目录：{error}")),
        }
        catalog.directory = Some(directory);
        catalog
    }

    fn read_plugins(&mut self, paths: Vec<PathBuf>) {
        let mut ids = HashSet::new();
        let mut tokens = HashSet::new();
        for path in paths {
            let result = read_plugin(&path).and_then(|plugin| {
                if ids.contains(&plugin.id) {
                    return Err(format!("插件 ID {} 已存在。", plugin.id));
                }
                if let Some(prompt) = plugin
                    .prompts
                    .iter()
                    .find(|prompt| tokens.contains(&prompt.token))
                {
                    return Err(format!("命令 {} 已由其他插件提供。", prompt.token));
                }
                Ok(plugin)
            });
            match result {
                Ok(plugin) => {
                    ids.insert(plugin.id.clone());
                    tokens.extend(plugin.prompts.iter().map(|prompt| prompt.token.clone()));
                    self.plugins.push(plugin);
                }
                Err(error) => self.problems.push(format!(
                    "{}：{error}",
                    path.file_name().unwrap_or_default().to_string_lossy()
                )),
            }
        }
    }

    pub fn prompts(&self) -> impl Iterator<Item = &Prompt> {
        self.plugins.iter().flat_map(|plugin| &plugin.prompts)
    }
}

pub(super) fn install_example(directory: &Path) -> io::Result<()> {
    fs::create_dir_all(directory)?;
    let mut file = tempfile::NamedTempFile::new_in(directory)?;
    file.write_all(EXAMPLE.as_bytes())?;
    match file.persist_noclobber(directory.join(EXAMPLE_FILE)) {
        Ok(_) => Ok(()),
        Err(error) if error.error.kind() == io::ErrorKind::AlreadyExists => Ok(()),
        Err(error) => Err(error.error),
    }
}

fn manifests(directory: &Path) -> io::Result<Vec<PathBuf>> {
    let entries = match fs::read_dir(directory) {
        Ok(entries) => entries,
        Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(Vec::new()),
        Err(error) => return Err(error),
    };
    let mut paths = Vec::new();
    for entry in entries {
        let entry = entry?;
        let path = entry.path();
        if path
            .extension()
            .is_some_and(|extension| extension == "json")
            && entry.file_type()?.is_file()
        {
            paths.push(path);
            if paths.len() > MAX_FILES {
                return Err(io::Error::other("最多可加载 32 份插件清单。"));
            }
        }
    }
    paths.sort_unstable();
    Ok(paths)
}

fn read_plugin(path: &Path) -> Result<Plugin, String> {
    let read = || -> io::Result<Vec<u8>> {
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        Ok(bytes)
    };
    let bytes = read().map_err(|error| error.to_string())?;
    if bytes.len() as u64 > MAX_BYTES {
        return Err("插件清单超过 64 KiB。".into());
    }
    let value = serde_json::from_slice(&bytes).map_err(|error| error.to_string())?;
    parse(&value)
}

fn fields(value: &Value, allowed: &[&str]) -> Result<(), String> {
    let object = value.as_object().ok_or("清单或贡献项必须是 JSON 对象。")?;
    if let Some(field) = object.keys().find(|key| !allowed.contains(&key.as_str())) {
        return Err(format!("不支持的字段 {field}。"));
    }
    Ok(())
}

fn text<'a>(value: &'a Value, field: &str, limit: usize) -> Result<&'a str, String> {
    value[field]
        .as_str()
        .filter(|text| !text.trim().is_empty() && text.len() <= limit)
        .ok_or_else(|| format!("{field} 必须是非空文本，最长 {limit} 字节。"))
}

fn parse(value: &Value) -> Result<Plugin, String> {
    fields(value, &["schemaVersion", "id", "name", "contributions"])?;
    if value["schemaVersion"].as_u64() != Some(1) {
        return Err("仅支持 schemaVersion 1。".into());
    }
    let id = text(value, "id", 64)?;
    if !id.starts_with(|ch: char| ch.is_ascii_lowercase())
        || !id.bytes().all(|byte| {
            byte.is_ascii_lowercase() || byte.is_ascii_digit() || matches!(byte, b'-' | b'.')
        })
    {
        return Err("插件 ID 须以小写字母开头，可含小写字母、数字、连字符和点。".into());
    }
    let name = text(value, "name", 120)?;
    if name.chars().any(char::is_control) {
        return Err("插件名称须为单行文本。".into());
    }
    let contributions = value["contributions"]
        .as_array()
        .filter(|items| (1..=8).contains(&items.len()))
        .ok_or("每个插件须提供 1 到 8 个贡献项。")?;
    let mut tokens = HashSet::new();
    let mut prompts = Vec::new();
    for item in contributions {
        fields(item, &["type", "command", "label", "text"])?;
        if item["type"].as_str() != Some("promptTemplate") {
            return Err("仅支持 promptTemplate 贡献项。".into());
        }
        let command = text(item, "command", 24)?;
        if !command.starts_with(|ch: char| ch.is_ascii_lowercase())
            || !command
                .bytes()
                .all(|byte| byte.is_ascii_lowercase() || byte.is_ascii_digit() || byte == b'-')
        {
            return Err("命令须以小写字母开头，可含小写字母、数字和连字符。".into());
        }
        let token = format!("/{command}");
        if is_builtin(&token) || !tokens.insert(command) {
            return Err(format!("命令 {token} 与内置命令冲突或在同一插件中重复。"));
        }
        let label = text(item, "label", 120)?;
        if label.chars().any(char::is_control) {
            return Err("命令名称须为单行文本。".into());
        }
        let body = text(item, "text", 4096)?;
        if body
            .chars()
            .any(|ch| ch.is_control() && !matches!(ch, '\n' | '\r' | '\t'))
        {
            return Err("模板正文含有不支持的控制字符。".into());
        }
        prompts.push(Prompt {
            token,
            label: label.into(),
            text: body.replace("\r\n", "\n").replace('\r', "\n"),
        });
    }
    Ok(Plugin {
        id: id.into(),
        name: name.into(),
        prompts,
    })
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn loads_example_without_overwriting_local_edits() {
        let directory = tempfile::tempdir().unwrap();
        install_example(directory.path()).unwrap();
        let catalog = Catalog::load(directory.path().into());
        assert!(catalog.problems.is_empty());
        assert_eq!(
            catalog
                .prompts()
                .map(|prompt| prompt.token.as_str())
                .collect::<Vec<_>>(),
            ["/summarize", "/test-plan"]
        );
        let path = directory.path().join(EXAMPLE_FILE);
        fs::write(&path, "local edits").unwrap();
        install_example(directory.path()).unwrap();
        assert_eq!(fs::read_to_string(path).unwrap(), "local edits");
    }

    #[test]
    fn rejects_an_invalid_or_conflicting_plugin_as_a_whole() {
        let directory = tempfile::tempdir().unwrap();
        install_example(directory.path()).unwrap();
        let mut manifest: Value = serde_json::from_str(EXAMPLE).unwrap();
        manifest["id"] = "workbench.conflicting".into();
        manifest["contributions"][1]["command"] = "unique".into();
        fs::write(
            directory.path().join("x-conflict.json"),
            manifest.to_string(),
        )
        .unwrap();
        manifest["contributions"][0]["command"] = "review".into();
        fs::write(
            directory.path().join("y-reserved.json"),
            manifest.to_string(),
        )
        .unwrap();
        manifest["contributions"][0]["command"] = "unique-two".into();
        manifest["contributions"][1]["type"] = "futureType".into();
        fs::write(directory.path().join("z-future.json"), manifest.to_string()).unwrap();
        let catalog = Catalog::load(directory.path().into());
        assert_eq!(catalog.plugins.len(), 1);
        assert_eq!(catalog.prompts().count(), 2);
        assert_eq!(catalog.problems.len(), 3);
        assert!(catalog.problems[0].contains("其他插件"));
        assert!(catalog.problems[1].contains("内置命令"));
        assert!(catalog.problems[2].contains("promptTemplate"));
    }
}
