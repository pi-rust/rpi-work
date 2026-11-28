use std::{
    fs,
    io::{self, Read, Write},
    path::{Path, PathBuf},
};

use serde_json::{Value, json};

use super::Preferences;
use crate::theme::Palette;

const MAX_BYTES: u64 = 1024 * 1024;

pub(super) struct SettingsFile {
    pub path: Option<PathBuf>,
    pub revision: u64,
    pub problem: Option<String>,
    contents: Option<Vec<u8>>,
    baseline: Value,
}

impl SettingsFile {
    pub fn load(path: Option<PathBuf>) -> (Self, Preferences) {
        let mut preferences = Preferences::default();
        let mut file = Self {
            path,
            revision: 0,
            problem: None,
            contents: None,
            baseline: preferences.value(),
        };
        if let Some(path) = &file.path {
            file.reload(Self::read(path), &mut preferences);
        }
        (file, preferences)
    }

    pub fn read(path: &Path) -> io::Result<Option<Vec<u8>>> {
        let metadata = match fs::metadata(path) {
            Ok(metadata) => metadata,
            Err(error) if error.kind() == io::ErrorKind::NotFound => return Ok(None),
            Err(error) => return Err(error),
        };
        if !metadata.is_file() {
            return Err(invalid("设置路径必须指向普通文件。"));
        }
        let mut bytes = Vec::new();
        fs::File::open(path)?
            .take(MAX_BYTES + 1)
            .read_to_end(&mut bytes)?;
        if bytes.len() as u64 > MAX_BYTES {
            return Err(invalid("设置文件超过 1 MiB。"));
        }
        Ok(Some(bytes))
    }

    pub fn reload(
        &mut self,
        read: io::Result<Option<Vec<u8>>>,
        preferences: &mut Preferences,
    ) -> bool {
        if self.problem.is_none() && read.as_ref().is_ok_and(|bytes| *bytes == self.contents) {
            return false;
        }
        let document = read.and_then(|bytes| parse(bytes.as_deref()).map(|value| (bytes, value)));
        let (bytes, value) = match document {
            Ok(document) => document,
            Err(error) => {
                let problem = error.to_string();
                let changed = self.problem.as_ref() != Some(&problem);
                self.problem = Some(problem);
                return changed;
            }
        };
        let baseline = Preferences::from_value(&value).value();
        let mut effective = baseline.clone();
        merge_changes(&mut effective, &self.baseline, &preferences.value());
        *preferences = Preferences::from_value(&effective);
        self.baseline = baseline;
        self.contents = bytes;
        self.problem = None;
        true
    }

    pub fn has_pending_changes(&self, preferences: &Preferences) -> bool {
        preferences.value() != self.baseline
    }

    pub fn open(&mut self, preferences: &Preferences) -> io::Result<PathBuf> {
        let path = self.path.clone().ok_or(io::ErrorKind::NotFound)?;
        match fs::metadata(&path) {
            Ok(metadata) if metadata.is_file() => {}
            Ok(_) => return Err(invalid("设置路径必须指向普通文件。")),
            Err(error) if error.kind() == io::ErrorKind::NotFound => {
                let value = preferences.value();
                let bytes = encode(&value)?;
                match temporary(&path, &bytes)?.persist_noclobber(&path) {
                    Ok(_) => self.record_write(bytes, value),
                    Err(error)
                        if error.error.kind() == io::ErrorKind::AlreadyExists
                            && fs::metadata(&path).is_ok_and(|metadata| metadata.is_file()) => {}
                    Err(error) => return Err(error.error),
                }
            }
            Err(error) => return Err(error),
        }
        Ok(path)
    }

    pub fn save(&mut self, preferences: &mut Preferences) -> io::Result<()> {
        let path = self.path.as_deref().ok_or(io::ErrorKind::NotFound)?;
        let original = Self::read(path)?;
        let current = parse(original.as_deref())?;
        let mut document = if original.is_some() {
            current.clone()
        } else {
            Preferences::default().value()
        };
        merge_changes(&mut document, &self.baseline, &preferences.value());
        let bytes = if let Some(bytes) = original.as_ref().filter(|_| current == document) {
            bytes.clone()
        } else {
            let bytes = encode(&document)?;
            if original.is_some() {
                // Keep a settings symlink intact when it points to a dotfiles checkout.
                let target = fs::canonicalize(path)?;
                let temp = temporary(&target, &bytes)?;
                if Self::read(path)? != original {
                    return Err(io::Error::other("保存时检测到设置文件已变更。"));
                }
                temp.persist(target).map_err(|error| error.error)?;
            } else {
                temporary(path, &bytes)?
                    .persist_noclobber(path)
                    .map_err(|error| error.error)?;
            }
            bytes
        };
        *preferences = Preferences::from_value(&document);
        self.record_write(bytes, preferences.value());
        Ok(())
    }

    fn record_write(&mut self, contents: Vec<u8>, baseline: Value) {
        self.contents = Some(contents);
        self.baseline = baseline;
        self.problem = None;
        self.revision += 1;
    }
}

fn parse(bytes: Option<&[u8]>) -> io::Result<Value> {
    let Some(bytes) = bytes else {
        return Ok(json!({}));
    };
    let bytes = bytes.strip_prefix(b"\xef\xbb\xbf").unwrap_or(bytes);
    let value: Value = serde_json::from_slice(bytes).map_err(|error| {
        invalid(format!(
            "JSON 格式错误（第 {} 行，第 {} 列）。",
            error.line(),
            error.column()
        ))
    })?;
    let object = value
        .as_object()
        .ok_or_else(|| invalid("设置文件必须是 JSON 对象。"))?;
    for key in [
        "classic",
        "largeText",
        "compact",
        "codeNumbers",
        "showContext",
        "automaticUpdates",
        "detachedReview",
        "notifyFinish",
    ] {
        if object.get(key).is_some_and(|value| !value.is_boolean()) {
            return Err(invalid(format!("{key} 必须是 true 或 false。")));
        }
    }
    one_of(&value, "color", &["system", "light", "dark"])?;
    one_of(&value, "followUp", &["followUp", "steer"])?;
    if let Some(theme) = value.get("theme")
        && !Palette::presets()
            .iter()
            .any(|(id, _)| theme.as_str() == Some(id))
    {
        return Err(invalid("theme 必须是有效的主题 ID。"));
    }
    if let Some(navigation) = value.get("navigation") {
        if !navigation.is_object() {
            return Err(invalid("navigation 必须是 JSON 对象。"));
        }
        one_of(navigation, "layout", &["node", "flat"])?;
        for key in ["collapsedSections", "collapsedNodes"] {
            if navigation.get(key).is_some_and(|value| !string_list(value)) {
                return Err(invalid(format!("navigation.{key} 必须是字符串数组。")));
            }
        }
        for key in ["pinnedProjects", "archivedProjects"] {
            if navigation.get(key).is_some_and(|value| {
                !value
                    .as_object()
                    .is_some_and(|projects| projects.values().all(string_list))
            }) {
                return Err(invalid(format!(
                    "navigation.{key} 的每个节点必须对应字符串数组。"
                )));
            }
        }
    }
    Ok(value)
}

fn one_of(object: &Value, key: &str, options: &[&str]) -> io::Result<()> {
    if let Some(value) = object.get(key)
        && !value.as_str().is_some_and(|value| options.contains(&value))
    {
        return Err(invalid(format!(
            "{key} 的可选值为 {}。",
            options.join("、")
        )));
    }
    Ok(())
}

fn string_list(value: &Value) -> bool {
    value
        .as_array()
        .is_some_and(|items| items.iter().all(Value::is_string))
}

fn invalid(message: impl Into<String>) -> io::Error {
    io::Error::new(io::ErrorKind::InvalidData, message.into())
}

fn encode(value: &Value) -> io::Result<Vec<u8>> {
    let mut bytes = serde_json::to_vec_pretty(value)?;
    bytes.push(b'\n');
    if bytes.len() as u64 > MAX_BYTES {
        return Err(invalid("设置文件超过 1 MiB。"));
    }
    Ok(bytes)
}

fn temporary(path: &Path, bytes: &[u8]) -> io::Result<tempfile::NamedTempFile> {
    let parent = path.parent().ok_or(io::ErrorKind::NotFound)?;
    fs::create_dir_all(parent)?;
    let mut temp = tempfile::NamedTempFile::new_in(parent)?;
    temp.write_all(bytes)?;
    Ok(temp)
}

// Apply only the choices changed in this window to the latest document on disk.
fn merge_changes(document: &mut Value, baseline: &Value, edited: &Value) {
    if baseline == edited {
        return;
    }
    // The arrays written by the UI are sets of navigation IDs.
    if let Some(edited) = edited.as_array() {
        let baseline = baseline.as_array().map_or(&[][..], Vec::as_slice);
        let mut values = document.as_array().cloned().unwrap_or_default();
        values.retain(|value| !baseline.contains(value) || edited.contains(value));
        for value in edited.iter().filter(|value| !baseline.contains(value)) {
            if !values.contains(value) {
                values.push(value.clone());
            }
        }
        *document = Value::Array(values);
        return;
    }
    let (Some(baseline), Some(edited)) = (baseline.as_object(), edited.as_object()) else {
        *document = edited.clone();
        return;
    };
    if !document.is_object() {
        *document = json!({});
    }
    let document = document.as_object_mut().expect("settings object");
    for key in baseline.keys().filter(|key| !edited.contains_key(*key)) {
        document.remove(key);
    }
    for (key, edited) in edited {
        let baseline = baseline.get(key).unwrap_or(&Value::Null);
        if baseline != edited {
            merge_changes(
                document.entry(key.clone()).or_insert(Value::Null),
                baseline,
                edited,
            );
        }
    }
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn edit_creates_a_readable_default_file_and_leaves_existing_bytes_alone() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("config/gpui-settings.json");
        let (mut file, preferences) = SettingsFile::load(Some(path.clone()));
        assert_eq!(file.open(&preferences).unwrap(), path);
        let text = fs::read_to_string(&path).unwrap();
        assert!(text.contains("\n  \"theme\": \"codex\""));
        assert!(text.ends_with('\n'));

        let authored = b"{\"color\": \"dark\", \"future\": true}\n";
        fs::write(&path, authored).unwrap();
        file.open(&preferences).unwrap();
        assert_eq!(fs::read(path).unwrap(), authored);
    }

    #[test]
    fn ui_save_merges_external_edits_and_navigation_ids() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("gpui-settings.json");
        fs::write(&path, br#"{"navigation":{"collapsedNodes":["old"]}}"#).unwrap();
        let (mut file, mut preferences) = SettingsFile::load(Some(path.clone()));
        let mut edited = preferences.value();
        edited["navigation"]["collapsedNodes"] = json!(["ui"]);
        edited["showContext"] = json!(false);
        preferences = Preferences::from_value(&edited);

        fs::write(&path, br#"{"color":"dark","future":{"value":42},"navigation":{"collapsedNodes":["old","external"],"custom":true}}"#).unwrap();
        file.save(&mut preferences).unwrap();
        let saved = parse(SettingsFile::read(&path).unwrap().as_deref()).unwrap();
        assert_eq!(saved["color"], "dark");
        assert_eq!(saved["future"]["value"], 42);
        assert_eq!(saved["navigation"]["custom"], true);
        assert_eq!(
            saved["navigation"]["collapsedNodes"],
            json!(["external", "ui"])
        );
        assert_eq!(saved["showContext"], false);
        assert_eq!(preferences.value()["color"], "dark");
        assert!(!file.reload(SettingsFile::read(&path), &mut preferences));
    }

    #[test]
    fn malformed_edits_keep_previous_settings_and_can_be_fixed_before_retry() {
        let root = tempfile::tempdir().unwrap();
        let path = root.path().join("gpui-settings.json");
        fs::write(&path, br#"{"theme":"ocean","color":"dark"}"#).unwrap();
        let (mut file, mut preferences) = SettingsFile::load(Some(path.clone()));
        let broken = b"{\n  \"color\":\n}";
        fs::write(&path, broken).unwrap();
        assert!(file.reload(SettingsFile::read(&path), &mut preferences));
        assert!(file.problem.as_deref().unwrap().contains("第 3 行"));
        assert_eq!(preferences.value()["theme"], "ocean");
        preferences.notify_finish = true;
        assert!(file.save(&mut preferences).is_err());
        assert_eq!(fs::read(&path).unwrap(), broken);

        fs::write(&path, br#"{"color":"system","largeText":true}"#).unwrap();
        assert!(file.reload(SettingsFile::read(&path), &mut preferences));
        assert!(file.problem.is_none());
        assert_eq!(preferences.value()["color"], "system");
        assert_eq!(preferences.value()["theme"], "codex");
        assert_eq!(preferences.font_size(), 18.0);
        assert!(file.has_pending_changes(&preferences));
        file.save(&mut preferences).unwrap();
        assert!(
            parse(SettingsFile::read(&path).unwrap().as_deref()).unwrap()["notifyFinish"] == true
        );
    }

    #[test]
    fn settings_accept_partial_documents_and_report_mistyped_fields() {
        assert!(parse(Some(b"\xef\xbb\xbf{}\n")).is_ok());
        for (input, key) in [
            (r#"{"theme":"missing"}"#, "theme"),
            (r#"{"compact":"true"}"#, "compact"),
            (r#"{"navigation":{"layout":"list"}}"#, "layout"),
            (
                r#"{"navigation":{"pinnedProjects":{"node":[1]}}}"#,
                "pinnedProjects",
            ),
        ] {
            assert!(
                parse(Some(input.as_bytes()))
                    .unwrap_err()
                    .to_string()
                    .contains(key)
            );
        }
    }
}
