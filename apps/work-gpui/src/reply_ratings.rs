use std::{fs, path::PathBuf};

use gpui::{App, Global};
use serde_json::{Value, json};

#[derive(Clone, PartialEq, Eq)]
pub(super) struct Key {
    pub node: String,
    pub project: String,
    pub session: String,
    pub entry: String,
}

#[derive(Clone, Copy, PartialEq, Eq)]
pub(super) enum Rating {
    Helpful,
    Unhelpful,
}

#[derive(Default)]
pub(super) struct Ratings(Vec<(Key, Rating)>);

impl Global for Ratings {}

impl Ratings {
    const LIMIT: usize = 2048;

    fn path() -> Option<PathBuf> {
        work_viewmodel::connection::native_path()
            .map(|path| path.with_file_name("gpui-ratings.json"))
    }

    pub(super) fn initialize(cx: &mut App) {
        if !cx.has_global::<Self>() {
            let entries: Vec<Value> = Self::path()
                .and_then(|path| fs::read(path).ok())
                .and_then(|bytes| serde_json::from_slice(&bytes).ok())
                .unwrap_or_default();
            let mut ratings = Self(
                entries
                    .iter()
                    .filter_map(|entry| {
                        let key = Key {
                            node: entry["node"].as_str()?.into(),
                            project: entry["project"].as_str()?.into(),
                            session: entry["session"].as_str()?.into(),
                            entry: entry["entry"].as_str()?.into(),
                        };
                        let rating = match entry["rating"].as_str()? {
                            "helpful" => Rating::Helpful,
                            "unhelpful" => Rating::Unhelpful,
                            _ => return None,
                        };
                        Some((key, rating))
                    })
                    .collect(),
            );
            ratings.trim();
            cx.set_global(ratings);
        }
    }

    pub(super) fn get(&self, key: &Key) -> Option<Rating> {
        self.0
            .iter()
            .rfind(|(candidate, _)| candidate == key)
            .map(|(_, rating)| *rating)
    }

    pub(super) fn toggle(&mut self, key: Key, rating: Rating) {
        let previous = self.get(&key);
        self.0.retain(|(candidate, _)| candidate != &key);
        if previous != Some(rating) {
            self.0.push((key, rating));
            self.trim();
        }
        if let Err(error) = self.save() {
            eprintln!("Could not save reply ratings: {error}");
        }
    }

    fn trim(&mut self) {
        let excess = self.0.len().saturating_sub(Self::LIMIT);
        self.0.drain(..excess);
    }

    fn save(&self) -> std::io::Result<()> {
        let path = Self::path().ok_or(std::io::ErrorKind::NotFound)?;
        fs::create_dir_all(path.parent().ok_or(std::io::ErrorKind::NotFound)?)?;
        let temporary = path.with_extension(format!("{}.tmp", std::process::id()));
        let entries: Vec<_> = self.0.iter().map(|(key, rating)| json!({
            "node": key.node,
            "project": key.project,
            "session": key.session,
            "entry": key.entry,
            "rating": match rating { Rating::Helpful => "helpful", Rating::Unhelpful => "unhelpful" },
        })).collect();
        fs::write(&temporary, serde_json::to_vec(&entries)?)?;
        if let Err(error) = fs::rename(&temporary, &path) {
            let _ = fs::remove_file(&temporary);
            return Err(error);
        }
        Ok(())
    }
}
