//! Frontend registration checks shared by every source parser in a compilation. Registration
//! names must be usable in the fixed `@namespace.name` grammar and cannot be ambiguous across
//! native libraries. IR-only plugin names retain the pass loader's naming rules.

use solar_data_structures::map::FxHashSet;
use solar_pass_plugin::{ExtensionKind, host::NativePlugin};
use std::sync::Arc;

pub(crate) fn validate(plugins: &[Arc<NativePlugin>]) -> Result<(), String> {
    let mut owners = FxHashSet::default();
    for plugin in plugins {
        let name = plugin.manifest()?.name;
        if !owners.insert(name.clone()) {
            return Err(format!("duplicate plugin namespace `{name}`"));
        }
        let frontend = plugin.frontend_manifest()?;
        if frontend.extensions.is_empty() && frontend.builtins.is_empty() {
            continue;
        }
        if !identifier(&name) {
            return Err(format!("invalid frontend plugin namespace `{name}`"));
        }
        let mut expressions = FxHashSet::default();
        let mut statements = FxHashSet::default();
        for extension in frontend.extensions {
            let names = match extension.kind {
                ExtensionKind::Expression => &mut expressions,
                ExtensionKind::Statement => &mut statements,
            };
            if !identifier(&extension.name) || !names.insert(extension.name.clone()) {
                return Err(format!(
                    "invalid or duplicate frontend extension `@{name}.{}`",
                    extension.name
                ));
            }
        }
        for builtin in frontend.builtins {
            if !identifier(&builtin.name) || !expressions.insert(builtin.name.clone()) {
                return Err(format!(
                    "invalid or duplicate plugin builtin `@{name}.{}`",
                    builtin.name
                ));
            }
        }
    }
    Ok(())
}

fn identifier(name: &str) -> bool {
    let mut bytes = name.bytes();
    bytes.next().is_some_and(|byte| byte.is_ascii_alphabetic() || byte == b'_')
        && bytes.all(|byte| byte.is_ascii_alphanumeric() || byte == b'_')
}
