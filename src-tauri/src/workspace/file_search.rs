//! On-demand filename search with bounded, short-lived per-workspace indexes.
use super::FileEntry;
use std::{
    collections::{BinaryHeap, VecDeque},
    path::{Path, PathBuf},
    sync::{Arc, Mutex, OnceLock},
    time::{Duration, Instant},
};

const MAX_FILES: usize = 20_000;
const MAX_PATH_BYTES: usize = 2 * 1024 * 1024;
const CACHE_TTL: Duration = Duration::from_secs(3);
const MAX_ROOTS: usize = 3;

struct IndexedFile {
    path: String,
    folded: String,
}
#[derive(Default)]
struct Index {
    built: Option<Instant>,
    files: Vec<IndexedFile>,
    /// Folders seen while walking; offered for typed queries so `@src/components/`
    /// can point an agent at an area without listing every file in it.
    folders: Vec<IndexedFile>,
}
const MAX_FOLDER_MATCHES: usize = 6;
#[derive(Default)]
struct SearchCache {
    roots: Mutex<VecDeque<(PathBuf, Arc<Mutex<Index>>)>>,
}

pub fn search_files(
    root: &Path,
    query: String,
    limit: Option<usize>,
) -> Result<Vec<FileEntry>, String> {
    static CACHE: OnceLock<SearchCache> = OnceLock::new();
    CACHE
        .get_or_init(SearchCache::default)
        .search(root, &query, limit)
}

impl SearchCache {
    fn search(
        &self,
        root: &Path,
        query: &str,
        limit: Option<usize>,
    ) -> Result<Vec<FileEntry>, String> {
        if query.len() > 512 || query.contains('\0') {
            return Err(
                "File search queries must be at most 512 bytes and contain no null characters."
                    .into(),
            );
        }
        let root = root.canonicalize().map_err(|error| error.to_string())?;
        if !root.is_dir() {
            return Err("Project folder is unavailable.".into());
        }
        let limit = limit.unwrap_or(40).min(100);
        if limit == 0 {
            return Ok(Vec::new());
        }
        let query = query
            .trim()
            .strip_prefix("./")
            .unwrap_or(query.trim())
            .to_lowercase();
        let shared = {
            let mut roots = self
                .roots
                .lock()
                .map_err(|_| "File search is unavailable.")?;
            let entry = roots
                .iter()
                .position(|entry| entry.0 == root)
                .and_then(|position| roots.remove(position))
                .unwrap_or_else(|| (root.clone(), Arc::new(Mutex::new(Index::default()))));
            let shared = entry.1.clone();
            roots.push_front(entry);
            roots.truncate(MAX_ROOTS);
            shared
        };
        // Serialize refreshes only for this root; another project never waits for its scan.
        let mut index = shared.lock().map_err(|_| "File search is unavailable.")?;
        if index.built.is_none_or(|built| built.elapsed() >= CACHE_TTL) {
            let scanned = scan_all(&root, MAX_FILES);
            index.files = scanned.files;
            index.folders = scanned.folders;
            index.built = Some(Instant::now());
        }
        let mut files = ranked(&root, &index.files, &query, limit, false);
        if !query.is_empty() {
            let folders = ranked(&root, &index.folders, &query, MAX_FOLDER_MATCHES, true);
            files.extend(folders);
            // Folders sort with files of the same score, after them on ties.
            files.sort_by(|a, b| a.0.cmp(&b.0).then(a.3.cmp(&b.3)).then(a.1.cmp(&b.1)));
            files.truncate(limit);
        }
        Ok(files
            .into_iter()
            .map(|(_, _, path, is_dir)| FileEntry {
                name: path.rsplit('/').next().unwrap_or(&path).to_owned(),
                path,
                is_dir,
            })
            .collect())
    }
}

fn ranked(
    root: &Path,
    entries: &[IndexedFile],
    query: &str,
    limit: usize,
    directories: bool,
) -> Vec<(u8, String, String, bool)> {
    let mut matches = BinaryHeap::new();
    for (position, file) in entries.iter().enumerate() {
        let name = file.folded.rsplit('/').next().unwrap_or(&file.folded);
        let trimmed = query.strip_suffix('/').unwrap_or(query);
        let score = if query.is_empty() || name == trimmed {
            0
        } else if name.starts_with(trimmed) {
            1
        } else if name.contains(trimmed) {
            2
        } else if file.folded.contains(trimmed) {
            3
        } else {
            continue;
        };
        let rank = (score, file.folded.as_str(), file.path.as_str(), position);
        if matches.len() == limit && matches.peek().is_some_and(|worst| &rank >= worst) {
            continue;
        }
        // Revalidate matches even on a cache hit: files can be deleted or ancestors
        // replaced with symlinks between keystrokes. Never offer an outside path.
        let live = super::resolve_scoped_path(root, &file.path)
            .is_ok_and(|path| if directories { path.is_dir() } else { path.is_file() });
        if !live {
            continue;
        }
        matches.push(rank);
        if matches.len() > limit {
            matches.pop();
        }
    }
    matches
        .into_sorted_vec()
        .into_iter()
        .map(|(score, folded, path, _)| (score, folded.to_owned(), path.to_owned(), directories))
        .collect()
}

pub(crate) fn excluded(name: &std::ffi::OsStr, directory: bool) -> bool {
    matches!(name.to_str(), Some(".git" | ".hg" | ".svn"))
        || directory
            && matches!(
                name.to_str(),
                Some(
                    "node_modules"
                        | "target"
                        | "dist"
                        | "build"
                        | "coverage"
                        | ".next"
                        | ".nuxt"
                        | ".svelte-kit"
                        | ".venv"
                        | "venv"
                        | "__pycache__"
                        | ".turbo"
                        | ".cache"
                        | ".parcel-cache"
                        | ".gradle"
                        | "Pods"
                        | "DerivedData"
                )
            )
}

#[derive(Default)]
struct Scanned {
    files: Vec<IndexedFile>,
    folders: Vec<IndexedFile>,
}

fn scan_all(root: &Path, file_limit: usize) -> Scanned {
    scan_with_budget(
        root,
        file_limit,
        80_000,
        Instant::now() + Duration::from_millis(500),
    )
}

fn scan_with_budget(
    root: &Path,
    file_limit: usize,
    entry_limit: usize,
    deadline: Instant,
) -> Scanned {
    let mut files = Vec::new();
    let mut folders = Vec::new();
    if file_limit == 0 || entry_limit == 0 || Instant::now() >= deadline {
        return Scanned::default();
    }
    let in_repo = root.ancestors().any(|path| path.join(".git").exists());
    let mut builder = ignore::WalkBuilder::new(root);
    builder
        .hidden(false)
        // Keep repository boundaries and linked-worktree common-dir resolution.
        // Plain folders still benefit from local/nested .gitignore files.
        .require_git(in_repo)
        .max_depth(Some(48));
    let Some(mut matcher) = builder.build_matchers().into_iter().next() else {
        return Scanned::default();
    };
    let mut directories: Vec<_> = std::fs::read_dir(root).ok().into_iter().collect();
    let mut bytes = 0;
    // Count raw directory entries, including ignored files. An ignore-filtered
    // iterator can internally enumerate an entire flat directory before yielding.
    // Keep only a bounded-depth stack of directory handles, never sorted entry lists.
    // Filesystem I/O itself cannot be preempted; check the deadline before each entry.
    for _ in 0..entry_limit {
        if Instant::now() >= deadline || files.len() >= file_limit {
            break;
        }
        let Some(directory) = directories.last_mut() else {
            break;
        };
        let Some(result) = directory.next() else {
            directories.pop();
            continue;
        };
        let Ok(entry) = result else { continue };
        let Ok(kind) = entry.file_type() else {
            continue;
        };
        if kind.is_symlink() || excluded(&entry.file_name(), kind.is_dir()) {
            continue;
        }
        if !kind.is_dir() && !kind.is_file() {
            continue;
        }
        let full_path = entry.path();
        let Ok(relative) = full_path.strip_prefix(root) else {
            continue;
        };
        let Some(path) = relative.to_str() else {
            continue;
        };
        if path.len() > 4096 {
            continue;
        }
        let matched = matcher.matched(relative, kind.is_dir());
        if matched.is_ignore() {
            continue;
        }
        #[cfg(windows)]
        let path = path.replace('\\', "/");
        let folded = path.to_lowercase();
        bytes += path.len() + folded.len();
        if bytes > MAX_PATH_BYTES {
            break;
        }
        if kind.is_dir() {
            if matched.should_descend() {
                if let Ok(directory) = std::fs::read_dir(&full_path) {
                    directories.push(directory);
                }
            }
            folders.push(IndexedFile {
                path: path.to_owned(),
                folded,
            });
            continue;
        }
        files.push(IndexedFile {
            path: path.to_owned(),
            folded,
        });
    }
    Scanned { files, folders }
}

#[cfg(test)]
mod tests {
    use super::*;
    use std::{fs, path::PathBuf};

    struct Fixture(PathBuf);
    impl Fixture {
        fn new() -> Self {
            let root =
                std::env::temp_dir().join(format!("pandacode-file-search-{}", uuid::Uuid::new_v4()));
            fs::create_dir(&root).unwrap();
            Self(root.canonicalize().unwrap())
        }
        fn write(&self, path: &str) {
            let path = self.0.join(path);
            fs::create_dir_all(path.parent().unwrap()).unwrap();
            fs::write(path, "test").unwrap();
        }
    }
    impl Drop for Fixture {
        fn drop(&mut self) {
            let _ = fs::remove_dir_all(&self.0);
        }
    }

    #[test]
    fn searches_nested_names_and_paths_without_losing_spaces_or_quotes() {
        let f = Fixture::new();
        f.write("src/components/Welcome Card.tsx");
        f.write("docs/author's notes.md");
        f.write("src/other.ts");
        let files = search_files(&f.0, "WELCOME".into(), None).unwrap();
        assert_eq!(files.len(), 1);
        assert_eq!(files[0].path, "src/components/Welcome Card.tsx");
        assert_eq!(files[0].name, "Welcome Card.tsx");
        assert!(!files[0].is_dir);
        // A path query finds the file inside and, after it, the folder itself.
        let inside = search_files(&f.0, "src/components/".into(), None).unwrap();
        assert_eq!(
            inside.iter().map(|e| (e.path.as_str(), e.is_dir)).collect::<Vec<_>>(),
            [("src/components/Welcome Card.tsx", false), ("src/components", true)]
        );
        assert_eq!(
            search_files(&f.0, "author's notes".into(), None).unwrap()[0].path,
            "docs/author's notes.md"
        );
    }

    #[test]
    fn offers_matching_folders_for_typed_queries_only_after_files_of_equal_rank() {
        let f = Fixture::new();
        f.write("src/components/Button.tsx");
        f.write("src/components/Card.tsx");
        f.write("src/components.test.ts");
        f.write("node_modules/components/index.js");
        let entries = search_files(&f.0, "components".into(), None).unwrap();
        let listed: Vec<_> = entries.iter().map(|e| (e.path.as_str(), e.is_dir)).collect();
        // Exact folder name first, then prefix and path matches; build output never appears.
        assert_eq!(
            listed,
            [
                ("src/components", true),
                ("src/components.test.ts", false),
                ("src/components/Button.tsx", false),
                ("src/components/Card.tsx", false),
            ]
        );
        assert_eq!(entries[0].name, "components");
        // A trailing slash still finds the folder; an empty query lists files only.
        assert!(search_files(&f.0, "components/".into(), None).unwrap().iter().any(|e| e.is_dir));
        assert!(search_files(&f.0, "".into(), None).unwrap().iter().all(|e| !e.is_dir));
        // A removed folder disappears from a cached index before the next rescan.
        fs::remove_dir_all(f.0.join("src/components")).unwrap();
        assert!(search_files(&f.0, "components".into(), None).unwrap().iter().all(|e| !e.is_dir));
    }

    #[cfg(unix)]
    #[test]
    fn searches_literal_double_quotes_in_unix_filenames() {
        let f = Fixture::new();
        f.write("docs/author's \"notes\".md");
        assert_eq!(
            search_files(&f.0, "author's \"notes\"".into(), None).unwrap()[0].path,
            "docs/author's \"notes\".md"
        );
    }

    #[test]
    fn honors_nested_ignore_rules_in_git_and_plain_folders_with_dotfiles_and_untracked_files() {
        for repo in [false, true] {
            let f = Fixture::new();
            if repo {
                assert!(std::process::Command::new("git")
                    .args(["init", "-q"])
                    .arg(&f.0)
                    .status()
                    .unwrap()
                    .success());
            }
            fs::write(f.0.join(".gitignore"), "ignored/\n*.log\n!keep.log\n").unwrap();
            f.write("ignored/secret.ts");
            f.write("error.log");
            f.write("keep.log");
            f.write("src/new file.ts");
            fs::write(f.0.join("src/.gitignore"), "private.ts\n").unwrap();
            f.write("src/private.ts");
            f.write(".github/workflows/check.yml");
            f.write(".env.example");
            for dir in [
                "node_modules",
                "target",
                "dist",
                "build",
                ".next",
                ".venv",
                "__pycache__",
            ] {
                f.write(&format!("{dir}/hidden.txt"));
            }
            if repo {
                f.write(".git/hidden.txt");
            } else {
                assert!(!f.0.join(".git").exists());
            }
            let files = search_files(&f.0, "".into(), None).unwrap();
            let paths: Vec<_> = files.iter().map(|file| file.path.as_str()).collect();
            assert!(paths.contains(&".env.example"));
            assert!(paths.contains(&".github/workflows/check.yml"));
            assert!(paths.contains(&"src/new file.ts"));
            assert!(paths.contains(&"keep.log"));
            assert_eq!(files.len(), 6, "{paths:?}");
            assert!(files.iter().all(|file| !file.is_dir));
        }
    }

    #[test]
    fn ranks_filename_matches_and_bounds_results_and_query_input() {
        let f = Fixture::new();
        f.write("z/main.rs");
        f.write("a/main.rs.bak");
        f.write("main.rs/other.txt");
        for i in 0..130 {
            f.write(&format!("many/file-{i}.rs"));
        }
        assert_eq!(
            search_files(&f.0, "main.rs".into(), Some(1)).unwrap()[0].path,
            "z/main.rs"
        );
        assert_eq!(search_files(&f.0, "".into(), None).unwrap().len(), 40);
        assert_eq!(
            search_files(&f.0, "".into(), Some(usize::MAX))
                .unwrap()
                .len(),
            100
        );
        assert!(search_files(&f.0, "".into(), Some(0)).unwrap().is_empty());
        assert!(search_files(&f.0, "x".repeat(513), None).is_err());
        assert!(search_files(&f.0, "bad\0query".into(), None).is_err());
        let index = scan_all(&f.0, 8).files;
        assert_eq!(index.len(), 8);
        assert!(
            index
                .iter()
                .map(|file| file.path.len() + file.folded.len())
                .sum::<usize>()
                <= MAX_PATH_BYTES
        );
    }

    #[test]
    fn reuses_the_index_between_keystrokes_then_refreshes_and_evicts_old_roots() {
        let cache = SearchCache::default();
        let f = Fixture::new();
        f.write("before.rs");
        assert_eq!(cache.search(&f.0, "before", None).unwrap().len(), 1);
        f.write("after.rs");
        assert!(
            cache.search(&f.0, "after", None).unwrap().is_empty(),
            "keystrokes must reuse the filename index"
        );
        fs::remove_file(f.0.join("before.rs")).unwrap();
        assert!(
            cache.search(&f.0, "before", None).unwrap().is_empty(),
            "deleted paths must not be offered from the cache"
        );
        cache.roots.lock().unwrap()[0].1.lock().unwrap().built = Some(Instant::now() - CACHE_TTL);
        assert_eq!(cache.search(&f.0, "after", None).unwrap().len(), 1);
        for _ in 0..MAX_ROOTS + 1 {
            let other = Fixture::new();
            other.write("unique.rs");
            assert_eq!(cache.search(&other.0, "unique", None).unwrap().len(), 1);
        }
        assert_eq!(cache.roots.lock().unwrap().len(), MAX_ROOTS);
    }

    #[test]
    fn large_filename_index_keeps_searches_bounded_and_reuses_the_scan() {
        let f = Fixture::new();
        for i in 0..3_000 {
            f.write(&format!("src/package-{}/component-{i}.tsx", i % 30));
        }
        for i in 0..100 {
            f.write(&format!("node_modules/ignored/dependency-{i}.js"));
        }
        let cache = SearchCache::default();
        let start = Instant::now();
        assert_eq!(cache.search(&f.0, "component", None).unwrap().len(), 40);
        let cold = start.elapsed();
        let built = cache.roots.lock().unwrap()[0].1.lock().unwrap().built;
        assert_eq!(
            cache.roots.lock().unwrap()[0].1.lock().unwrap().files.len(),
            3_000
        );
        let start = Instant::now();
        for query in [
            "c",
            "co",
            "com",
            "comp",
            "compo",
            "compon",
            "compone",
            "componen",
            "component",
            "component-2",
        ] {
            assert_eq!(cache.search(&f.0, query, None).unwrap().len(), 40);
        }
        eprintln!(
            "3,000-file search: cold {cold:?}, 10 cached queries {:?}",
            start.elapsed()
        );
        assert_eq!(
            cache.roots.lock().unwrap()[0].1.lock().unwrap().built,
            built
        );
    }

    #[test]
    fn scan_budget_counts_ignored_entries_in_flat_directories() {
        let f = Fixture::new();
        f.write(".gitignore");
        for i in 0..100 {
            f.write(&format!("file-{i}.txt"));
        }
        let names: Vec<_> = fs::read_dir(&f.0)
            .unwrap()
            .map(|entry| entry.unwrap().file_name().to_string_lossy().into_owned())
            .collect();
        let wanted = names
            .iter()
            .rev()
            .find(|name| *name != ".gitignore")
            .unwrap();
        fs::write(f.0.join(".gitignore"), format!("*\n!{wanted}\n")).unwrap();
        assert!(
            scan_with_budget(&f.0, 100, 2, Instant::now() + Duration::from_secs(1)).files.is_empty(),
            "ignored entries must consume the raw directory entry budget too"
        );
    }

    #[cfg(unix)]
    #[test]
    fn excludes_outside_symlinks_and_rechecks_cached_paths_after_directory_replacement() {
        let f = Fixture::new();
        let outside = Fixture::new();
        outside.write("secret.txt");
        std::os::unix::fs::symlink(&outside.0, f.0.join("escape")).unwrap();
        std::os::unix::fs::symlink(outside.0.join("secret.txt"), f.0.join("file-link")).unwrap();
        f.write("src/secret.txt");
        assert_eq!(search_files(&f.0, "".into(), None).unwrap().len(), 1);
        fs::remove_dir_all(f.0.join("src")).unwrap();
        std::os::unix::fs::symlink(&outside.0, f.0.join("src")).unwrap();
        assert!(search_files(&f.0, "secret".into(), None)
            .unwrap()
            .is_empty());
    }

    #[test]
    fn uses_task_worktree_and_rejects_cross_project_task_roots() {
        use crate::storage::Store;
        let f = Fixture::new();
        let git = |args: &[&str]| {
            let result = std::process::Command::new("git")
                .arg("-C")
                .arg(&f.0)
                .args(args)
                .output()
                .unwrap();
            assert!(
                result.status.success(),
                "{}",
                String::from_utf8_lossy(&result.stderr)
            );
        };
        git(&["init", "-q"]);
        git(&[
            "-c",
            "user.name=Test",
            "-c",
            "user.email=test@example.invalid",
            "-c",
            "commit.gpgSign=false",
            "-c",
            "core.hooksPath=/dev/null",
            "commit",
            "--allow-empty",
            "-qm",
            "Initial",
        ]);
        let checkout = Fixture::new();
        git(&[
            "worktree",
            "add",
            "-qb",
            "file-search",
            super::super::git_path(&checkout.0).to_str().unwrap(),
        ]);
        f.write("main-only.rs");
        checkout.write("task-only.rs");
        fs::write(f.0.join(".git/info/exclude"), "excluded-only.rs\n").unwrap();
        checkout.write("excluded-only.rs");
        let store = Store::open(Path::new(":memory:")).unwrap();
        let project = store.add_project(&f.0).unwrap();
        let source = store
            .create_session(&project.id, "codex", "default", "ask")
            .unwrap();
        let task = store.create_worktree_session_on(&source, &checkout.0, None).unwrap();
        let root = store.workspace_root(&project.id, Some(&task.id)).unwrap();
        assert_eq!(
            search_files(&root, "only".into(), None).unwrap().len(),
            1,
            "worktree searches must honor the shared Git exclude file"
        );
        assert_eq!(
            search_files(&root, "only".into(), None).unwrap()[0].path,
            "task-only.rs"
        );
        assert_eq!(
            search_files(
                &store.workspace_root(&project.id, None).unwrap(),
                "only".into(),
                None
            )
            .unwrap()[0]
                .path,
            "main-only.rs"
        );
        let other = Fixture::new();
        let other_project = store.add_project(&other.0).unwrap();
        assert!(store
            .workspace_root(&other_project.id, Some(&task.id))
            .is_err());
        fs::remove_file(root.join(".git")).unwrap();
        assert!(store.workspace_root(&project.id, Some(&task.id)).is_err());
    }
}
