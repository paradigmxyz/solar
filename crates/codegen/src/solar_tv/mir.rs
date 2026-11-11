//! Function-scoped audits keep the original module as the parser and replay context.
//!
//! Match unchanged function identities, then check every changed body independently. Shared
//! declarations, added/deleted functions and ambiguous names are explicit unsupported checks;
//! a local proof never claims that an interprocedural or module-wide edit was verified.

use super::{Region, Snapshot};
use solar_interface::Result;
use solar_mir::Module;
use solar_sema::Gcx;

pub(crate) struct MirSnapshot<'a> {
    snapshot: Snapshot<'a>,
    before: Module,
}

impl<'a> MirSnapshot<'a> {
    pub(crate) fn capture(gcx: Gcx<'a>, cached: bool, module: &Module) -> Option<Self> {
        let snapshot = Snapshot::capture(gcx, cached, module.to_text())?;
        Some(Self { snapshot, before: module.clone() })
    }

    pub(crate) fn check(self, gcx: Gcx<'_>, pass: &str, module: &Module) -> Result {
        let after = module.to_text().to_string();
        if self.snapshot.before == after {
            return Ok(());
        }
        let prefix = |text: &str| {
            text.lines().take_while(|line| !line.starts_with("fn @")).collect::<Vec<_>>().join("\n")
        };
        if prefix(&self.snapshot.before) != prefix(&after) {
            return self.snapshot.check(
                gcx,
                "mir",
                pass,
                module.name,
                &after,
                (None, Some("shared MIR declarations or phase changed; no function-local claim")),
            );
        }
        for (id, before) in self.before.functions.iter_enumerated() {
            let region = Region { kind: "function", name: before.name.to_string(), height: None };
            let same = module.functions.get(id).filter(|f| f.name == before.name);
            let rejection = if same.is_none() {
                Some("function was removed or its identity changed")
            } else if self.before.functions.iter().filter(|f| f.name == before.name).count() != 1
                || module.functions.iter().filter(|f| f.name == before.name).count() != 1
            {
                Some("ambiguous function name")
            } else {
                None
            };
            if rejection.is_some()
                || self.before.function_text(id).to_string() != module.function_text(id).to_string()
            {
                self.snapshot.check(
                    gcx,
                    "mir",
                    pass,
                    module.name,
                    &after,
                    (Some(&region), rejection),
                )?;
            }
        }
        if module.functions.len() > self.before.functions.len() {
            self.snapshot.check(
                gcx,
                "mir",
                pass,
                module.name,
                &after,
                (None, Some("functions were added; interprocedural edits are not modeled")),
            )?;
        }
        Ok(())
    }
}
