use solar_interface::data_structures::map::{FxHashMap, FxHashSet};
use solar_sema::{Gcx, hir};
use std::{
    path::{Path, PathBuf},
    sync::Arc,
};

#[derive(Clone, Debug, Default)]
pub(crate) struct DocumentIndexSnapshot {
    source_texts: FxHashMap<PathBuf, Arc<String>>,
    indexed_paths: FxHashSet<PathBuf>,
    dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
}

#[derive(Clone, Debug, Default)]
pub(super) struct DocumentIndexState {
    source_texts: FxHashMap<PathBuf, Arc<String>>,
    pub(super) indexed_paths: FxHashSet<PathBuf>,
    pub(super) rebuilt_paths: FxHashSet<PathBuf>,
    dependencies: FxHashMap<PathBuf, FxHashSet<PathBuf>>,
}

impl DocumentIndexState {
    pub(super) fn build(
        gcx: Gcx<'_>,
        indexed_paths: &FxHashSet<PathBuf>,
        previous: &DocumentIndexSnapshot,
        changed_paths: &FxHashSet<PathBuf>,
        rebuild_all: bool,
    ) -> Self {
        let source_texts = source_texts(gcx);
        let dependencies = dependencies(gcx);
        let mut changed_sources = changed_paths
            .iter()
            .filter(|path| source_changed(path, &source_texts, &previous.source_texts))
            .cloned()
            .collect::<FxHashSet<_>>();

        changed_sources.extend(source_texts.iter().filter_map(|(path, contents)| {
            (previous.source_texts.get(path) != Some(contents)).then_some(path.clone())
        }));
        changed_sources.extend(
            indexed_paths.iter().filter(|path| !previous.indexed_paths.contains(*path)).cloned(),
        );

        let rebuilt_paths = if rebuild_all {
            indexed_paths.clone()
        } else {
            reverse_dependencies(&changed_sources, [&previous.dependencies, &dependencies])
                .into_iter()
                .filter(|path| indexed_paths.contains(path))
                .collect()
        };

        Self { source_texts, indexed_paths: indexed_paths.clone(), rebuilt_paths, dependencies }
    }

    pub(super) fn snapshot(&self) -> DocumentIndexSnapshot {
        DocumentIndexSnapshot {
            source_texts: self.source_texts.clone(),
            indexed_paths: self.indexed_paths.clone(),
            dependencies: self.dependencies.clone(),
        }
    }

    pub(super) fn rebuilt_sources(&self, gcx: Gcx<'_>) -> FxHashSet<hir::SourceId> {
        gcx.hir
            .source_ids()
            .filter(|&source_id| {
                gcx.hir
                    .source(source_id)
                    .file
                    .name
                    .as_real()
                    .is_some_and(|path| self.rebuilt_paths.contains(path))
            })
            .collect()
    }

    pub(super) fn extend(&mut self, other: Self) {
        self.source_texts.extend(other.source_texts);
        self.indexed_paths.extend(other.indexed_paths);
        self.rebuilt_paths.extend(other.rebuilt_paths);
        for (path, imports) in other.dependencies {
            self.dependencies.entry(path).or_default().extend(imports);
        }
    }
}

fn source_texts(gcx: Gcx<'_>) -> FxHashMap<PathBuf, Arc<String>> {
    gcx.sess
        .source_map()
        .files()
        .iter()
        .filter_map(|file| {
            let path = file.name.as_real()?.to_path_buf();
            Some((path, file.src.clone()))
        })
        .collect()
}

fn dependencies(gcx: Gcx<'_>) -> FxHashMap<PathBuf, FxHashSet<PathBuf>> {
    let mut dependencies = FxHashMap::default();
    for source in gcx.sources.iter() {
        let Some(path) = source.file.name.as_real() else {
            continue;
        };
        let imports = dependencies.entry(path.to_path_buf()).or_insert_with(FxHashSet::default);
        imports.extend(source.imports.iter().filter_map(|&(_, target_id)| {
            gcx.sources.get(target_id)?.file.name.as_real().map(PathBuf::from)
        }));
    }
    dependencies
}

fn source_changed(
    path: &Path,
    current: &FxHashMap<PathBuf, Arc<String>>,
    previous: &FxHashMap<PathBuf, Arc<String>>,
) -> bool {
    current.get(path) != previous.get(path)
}

fn reverse_dependencies<'a>(
    changed_sources: &FxHashSet<PathBuf>,
    graphs: impl IntoIterator<Item = &'a FxHashMap<PathBuf, FxHashSet<PathBuf>>>,
) -> FxHashSet<PathBuf> {
    let mut reverse = FxHashMap::<&PathBuf, FxHashSet<&PathBuf>>::default();
    for graph in graphs {
        for (source, imports) in graph {
            for import in imports {
                reverse.entry(import).or_default().insert(source);
            }
        }
    }

    let mut affected = changed_sources.clone();
    let mut pending = changed_sources.iter().collect::<Vec<_>>();
    while let Some(path) = pending.pop() {
        if let Some(importers) = reverse.get(path) {
            for &importer in importers {
                if affected.insert(importer.clone()) {
                    pending.push(importer);
                }
            }
        }
    }
    affected
}
