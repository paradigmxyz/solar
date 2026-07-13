use serde_json::{Map, Value, json};
use std::{
    ffi::OsStr,
    fs,
    io::Read,
    path::{Path, PathBuf},
    process::Command,
};
use ui_test::{
    Errored, build_manager::BuildManager, custom_flags::Flag, per_test_config::TestConfig,
};

pub(super) fn configure_directory_fixture(
    config: &mut ui_test::Config,
    input: &Path,
    tmp_dir: &Path,
) {
    let relative = input.strip_prefix(&config.root_dir).unwrap_or(input);
    let generated = tmp_dir.join("standard-json").join(relative).with_extension("json");

    // ui_test always appends the test path. Consume it as an unused include path and pass the
    // generated Standard JSON input as the positional input instead.
    config.program.input_file_flag = Some("--include-path".into());
    config.comment_defaults.base().compile_flags.push(generated.to_string_lossy().into_owned());
    config.comment_defaults.base().add_custom(
        DirectoryFixture::NAME,
        DirectoryFixture { input: input.to_owned(), generated },
    );
}

#[derive(Clone, Debug)]
struct DirectoryFixture {
    input: PathBuf,
    generated: PathBuf,
}

impl DirectoryFixture {
    const NAME: &'static str = "standard-json-directory-fixture";

    fn generate(&self) -> Result<(), String> {
        let input = read_utf8(&self.input)?;
        let mut json: Value = serde_json::from_str(&strip_json_comments(&input))
            .map_err(|err| format!("failed to parse {}: {err}", self.input.display()))?;
        let root = json
            .as_object_mut()
            .ok_or_else(|| format!("{} must contain a JSON object", self.input.display()))?;
        let sources = root
            .entry("sources")
            .or_insert_with(|| Value::Object(Map::new()))
            .as_object_mut()
            .ok_or_else(|| {
                format!("`sources` in {} must be a JSON object", self.input.display())
            })?;

        let fixture_dir = self.input.parent().unwrap();
        for path in solidity_files(fixture_dir)? {
            let relative = path.strip_prefix(fixture_dir).unwrap();
            let name = source_name(relative)?;
            if !sources.contains_key(&name) {
                let content = read_utf8(&path)?;
                sources.insert(name, json!({ "content": content }));
            }
        }

        let parent = self.generated.parent().unwrap();
        fs::create_dir_all(parent)
            .map_err(|err| format!("failed to create {}: {err}", parent.display()))?;
        let mut output = serde_json::to_vec_pretty(&json)
            .map_err(|err| format!("failed to serialize {}: {err}", self.input.display()))?;
        output.push(b'\n');
        fs::write(&self.generated, output)
            .map_err(|err| format!("failed to write {}: {err}", self.generated.display()))
    }
}

impl Flag for DirectoryFixture {
    fn clone_inner(&self) -> Box<dyn Flag> {
        Box::new(self.clone())
    }

    fn apply(
        &self,
        _cmd: &mut Command,
        _config: &TestConfig,
        _build_manager: &BuildManager,
    ) -> Result<(), Errored> {
        self.generate().map_err(|message| {
            Errored::new(
                vec![ui_test::Error::ConfigError(message)],
                "prepare Standard JSON directory fixture",
            )
        })
    }

    fn must_be_unique(&self) -> bool {
        true
    }
}

fn read_utf8(path: &Path) -> Result<String, String> {
    let mut file =
        fs::File::open(path).map_err(|err| format!("failed to open {}: {err}", path.display()))?;
    let mut contents = String::new();
    file.read_to_string(&mut contents)
        .map_err(|err| format!("failed to read {}: {err}", path.display()))?;
    Ok(contents)
}

fn solidity_files(root: &Path) -> Result<Vec<PathBuf>, String> {
    let mut files = Vec::new();
    collect_solidity_files(root, &mut files)?;
    files.sort();
    Ok(files)
}

fn collect_solidity_files(dir: &Path, files: &mut Vec<PathBuf>) -> Result<(), String> {
    let entries = fs::read_dir(dir)
        .map_err(|err| format!("failed to read directory {}: {err}", dir.display()))?;
    for entry in entries {
        let entry =
            entry.map_err(|err| format!("failed to read an entry in {}: {err}", dir.display()))?;
        let file_type = entry
            .file_type()
            .map_err(|err| format!("failed to inspect {}: {err}", entry.path().display()))?;
        if file_type.is_dir() {
            collect_solidity_files(&entry.path(), files)?;
        } else if file_type.is_file() && entry.path().extension() == Some(OsStr::new("sol")) {
            files.push(entry.path());
        }
    }
    Ok(())
}

fn source_name(path: &Path) -> Result<String, String> {
    let components = path
        .components()
        .map(|component| {
            component.as_os_str().to_str().ok_or_else(|| {
                format!("source path is not valid UTF-8: {}", path.to_string_lossy())
            })
        })
        .collect::<Result<Vec<_>, _>>()?;
    Ok(components.join("/"))
}

fn strip_json_comments(input: &str) -> String {
    let mut out = String::with_capacity(input.len());
    let mut chars = input.chars().peekable();
    let mut in_string = false;
    let mut escaped = false;

    while let Some(ch) = chars.next() {
        if in_string {
            out.push(ch);
            if escaped {
                escaped = false;
            } else if ch == '\\' {
                escaped = true;
            } else if ch == '"' {
                in_string = false;
            }
            continue;
        }

        match ch {
            '"' => {
                in_string = true;
                out.push(ch);
            }
            '/' if chars.peek() == Some(&'/') => {
                chars.next();
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                        break;
                    }
                }
            }
            '/' if chars.peek() == Some(&'*') => {
                chars.next();
                let mut prev = '\0';
                for ch in chars.by_ref() {
                    if ch == '\n' {
                        out.push('\n');
                    }
                    if prev == '*' && ch == '/' {
                        break;
                    }
                    prev = ch;
                }
            }
            _ => out.push(ch),
        }
    }

    out
}
