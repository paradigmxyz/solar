//! Function-scoped audits keep the original module as the parser and replay context.
//!
//! Try module certificates for dispatch, declarations and inlining with removed call targets.
//! Other checks match unchanged function identities and validate changed bodies independently.
//! A local proof never certifies an interprocedural or module-wide edit.

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
        if !gcx.sess.opts.unstable.solar_tv_capture
            && self.before.dispatch_entry().is_none()
            && module.dispatch_entry().is_some()
            && module.functions.len() == self.before.functions.len() + 1
        {
            self.snapshot.check(
                gcx,
                "mir",
                pass,
                module.name,
                &after,
                (None, Some("runtime dispatch was materialized")),
            )?;
            return Ok(());
        }
        let prefix = |text: &str| {
            text.lines().take_while(|line| !line.starts_with("fn @")).collect::<Vec<_>>().join("\n")
        };
        if prefix(&self.snapshot.before) != prefix(&after) {
            self.snapshot.check(
                gcx,
                "mir",
                pass,
                module.name,
                &after,
                (None, Some("shared MIR declarations or phase changed; no function-local claim")),
            )?;
            return Ok(());
        }
        if pass == "inline"
            && !gcx.sess.opts.unstable.solar_tv_capture
            && self
                .before
                .functions
                .iter()
                .any(|before| !module.functions.iter().any(|after| before.name == after.name))
            && self.snapshot.check(
                gcx,
                "mir",
                pass,
                module.name,
                &after,
                (None, Some("function was removed or its identity changed")),
            )?
        {
            return Ok(());
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
