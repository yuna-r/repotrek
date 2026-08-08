use std::{
    collections::HashMap,
    fs,
    path::{Path, PathBuf},
};

use anyhow::{Context, Result, anyhow};
use chrono::Utc;
use directories::ProjectDirs;

use crate::model::{HistoryEntry, HistoryScreen, RepoCard, Repository, RepositoryId};

const MAX_HISTORY_ENTRIES: usize = 40;

#[derive(Debug)]
pub struct HistoryStore {
    path: PathBuf,
    entries: Vec<HistoryEntry>,
}

impl HistoryStore {
    pub fn load() -> Result<Self> {
        let project_dirs = ProjectDirs::from("dev", "yuna-r", "RepoTrek")
            .ok_or_else(|| anyhow!("履歴保存先を決定できません"))?;
        let path = project_dirs.data_dir().join("history.json");
        Self::from_path(path)
    }

    fn from_path(path: PathBuf) -> Result<Self> {
        let entries = if path.exists() {
            let bytes = fs::read(&path)
                .with_context(|| format!("履歴を読み込めません: {}", path.display()))?;
            match serde_json::from_slice::<Vec<HistoryEntry>>(&bytes) {
                Ok(entries) => entries,
                Err(error) => {
                    let corrupt_path = path.with_extension("json.corrupt");
                    let _ = fs::rename(&path, &corrupt_path);
                    eprintln!(
                        "RepoTrek: 壊れた履歴を {} へ退避しました: {error}",
                        corrupt_path.display()
                    );
                    Vec::new()
                }
            }
        } else {
            Vec::new()
        };

        Ok(Self { path, entries })
    }

    #[must_use]
    pub fn entries(&self) -> &[HistoryEntry] {
        &self.entries
    }

    pub fn record_repository(
        &mut self,
        repository: &Repository,
        last_path: Option<String>,
        last_screen: HistoryScreen,
    ) -> Result<()> {
        self.record_card(RepoCard::from(repository), last_path, last_screen)
    }

    pub fn record_card(
        &mut self,
        repository: RepoCard,
        last_path: Option<String>,
        last_screen: HistoryScreen,
    ) -> Result<()> {
        let full_name = repository.id.full_name();
        self.entries.retain(|entry| {
            !entry
                .repository
                .id
                .full_name()
                .eq_ignore_ascii_case(&full_name)
        });
        self.entries.insert(
            0,
            HistoryEntry {
                repository,
                last_path,
                last_screen,
                visited_at: Utc::now(),
            },
        );
        self.entries.truncate(MAX_HISTORY_ENTRIES);
        self.save()
    }

    pub fn update_location(
        &mut self,
        repository_id: &RepositoryId,
        last_path: Option<String>,
        last_screen: HistoryScreen,
    ) -> Result<()> {
        let full_name = repository_id.full_name();
        if let Some(index) = self.entries.iter().position(|entry| {
            entry
                .repository
                .id
                .full_name()
                .eq_ignore_ascii_case(&full_name)
        }) {
            let mut entry = self.entries.remove(index);
            entry.last_path = last_path;
            entry.last_screen = last_screen;
            entry.visited_at = Utc::now();
            self.entries.insert(0, entry);
            self.save()?;
        }
        Ok(())
    }

    #[must_use]
    pub fn top_language(&self) -> Option<String> {
        let mut counts: HashMap<&str, usize> = HashMap::new();
        for entry in &self.entries {
            if let Some(language) = entry.repository.language.as_deref() {
                *counts.entry(language).or_default() += 1;
            }
        }
        counts
            .into_iter()
            .max_by_key(|(_, count)| *count)
            .map(|(language, _)| language.to_owned())
    }

    fn save(&self) -> Result<()> {
        if let Some(parent) = self.path.parent() {
            fs::create_dir_all(parent)
                .with_context(|| format!("履歴保存先を作成できません: {}", parent.display()))?;
        }
        let data = serde_json::to_vec_pretty(&self.entries)?;
        let temporary = temporary_path(&self.path);
        fs::write(&temporary, data)
            .with_context(|| format!("履歴を書き込めません: {}", temporary.display()))?;
        replace_file(&temporary, &self.path)
            .with_context(|| format!("履歴を確定できません: {}", self.path.display()))?;
        Ok(())
    }
}

fn replace_file(source: &Path, destination: &Path) -> std::io::Result<()> {
    #[cfg(target_os = "windows")]
    if destination.exists() {
        fs::remove_file(destination)?;
    }
    fs::rename(source, destination)
}

fn temporary_path(path: &Path) -> PathBuf {
    let file_name = path
        .file_name()
        .and_then(|name| name.to_str())
        .unwrap_or("history.json");
    path.with_file_name(format!(".{file_name}.tmp"))
}

#[cfg(test)]
mod tests {
    use std::{fs, time::SystemTime};

    use chrono::Utc;

    use super::HistoryStore;
    use crate::model::{HistoryScreen, RepoCard, RepositoryId};

    #[test]
    fn records_and_deduplicates_history() {
        let unique = SystemTime::now()
            .duration_since(SystemTime::UNIX_EPOCH)
            .expect("system clock")
            .as_nanos();
        let path = std::env::temp_dir().join(format!("repotrek-history-{unique}.json"));
        let mut store = HistoryStore::from_path(path.clone()).expect("create store");
        let card = RepoCard {
            id: RepositoryId {
                owner: "yuna-r".to_owned(),
                name: "repotrek".to_owned(),
            },
            description: None,
            language: Some("Rust".to_owned()),
            stars: 0,
            updated_at: Some(Utc::now()),
        };

        store
            .record_card(card.clone(), None, HistoryScreen::Code)
            .expect("record history");
        store
            .record_card(card, Some("src/main.rs".to_owned()), HistoryScreen::File)
            .expect("record history again");

        assert_eq!(store.entries().len(), 1);
        assert_eq!(store.entries()[0].last_path.as_deref(), Some("src/main.rs"));
        assert_eq!(store.top_language().as_deref(), Some("Rust"));
        let _ = fs::remove_file(path);
    }
}
