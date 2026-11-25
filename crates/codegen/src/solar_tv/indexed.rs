//! Audit indexed transfers after byte emission. The assembler records where it emitted the
//! terminator and each original block label. The validator reads the logical target order
//! separately from the EVM IR and checks the physical transfers in the resulting byte image.

use crate::backend::assembler::Label;
use solar_data_structures::map::FxHashMap;
use solar_evmir::{
    ir::{Block, Module, TerminatorKind},
    op,
};
use solar_interface::source_map::{FileLoader, RealFileLoader};
use solar_sema::Gcx;
use std::{env, fs, io, path::Path, process::Command};

#[derive(Debug)]
pub(crate) struct Capture {
    pub(crate) source: String,
    pub(crate) module: String,
    pub(crate) artifact: String,
    pub(crate) original_labels: FxHashMap<Label, u32>,
    pub(crate) sites: Vec<Site>,
}

#[derive(Debug)]
pub(crate) struct Site {
    pub(crate) block: u32,
    pub(crate) start: usize,
    pub(crate) mask_start: Option<usize>,
    pub(crate) jumps: u8,
}

pub(crate) fn enabled(gcx: Gcx<'_>, module: &Module) -> bool {
    gcx.sess.opts.unstable.solar_tv.is_some()
        && module.blocks.iter().any(|block| {
            matches!(
                block.terminator.as_ref().map(|term| &term.kind),
                Some(TerminatorKind::IndexedJump(_))
            )
        })
}

pub(crate) fn mask_producer(block: &Block) -> Option<usize> {
    let [rest @ .., push, and] = block.instructions.as_slice() else { return None };
    let value = push.pushed_value()?;
    let Some(TerminatorKind::IndexedJump(targets)) =
        block.terminator.as_ref().map(|term| &term.kind)
    else {
        return None;
    };
    (and.as_evm_opcode() == Some(op::AND) && value < alloy_primitives::U256::from(targets.len()))
        .then_some(rest.len())
}

impl Capture {
    pub(crate) fn audit(
        self,
        gcx: Gcx<'_>,
        bytecode: &[u8],
        code_end: usize,
        labels: &[(u32, usize)],
        offsets: &[usize],
    ) {
        let Some(directory) = gcx.sess.opts.unstable.solar_tv.as_deref() else { return };
        let context = format!("solar-tv: indexed tables in `{}` ({})", self.module, self.artifact);
        if let Err(error) =
            self.write_and_check(gcx, directory, bytecode, code_end, labels, offsets)
        {
            gcx.dcx().err(format!("{context}: {error}")).emit();
        }
    }

    fn write_and_check(
        self,
        gcx: Gcx<'_>,
        directory: &Path,
        bytecode: &[u8],
        code_end: usize,
        labels: &[(u32, usize)],
        offsets: &[usize],
    ) -> io::Result<()> {
        let root = std::path::absolute(directory)?.join("indexed-tables");
        fs::create_dir_all(&root)?;
        let path = tempfile::Builder::new().prefix("artifact-").tempdir_in(root)?.keep();
        let manifest = path.join("layout.json");
        let sites: Vec<_> = self
            .sites
            .iter()
            .map(|site| {
                serde_json::json!({
                    "block": site.block, "pc": offsets[site.start], "jumps": site.jumps,
                    "mask_pc": site.mask_start.map(|start| offsets[start]),
                })
            })
            .collect();
        let record = serde_json::json!({
            "version": 1, "module": self.module, "artifact": self.artifact,
            "evm_version": gcx.sess.opts.evm_version.to_string(),
            "code_end": code_end, "labels": labels, "sites": sites,
        });
        fs::write(path.join("source.evmir"), self.source)?;
        fs::write(path.join("bytecode.hex"), alloy_primitives::hex::encode(bytecode))?;
        serde_json::to_writer_pretty(fs::File::create(&manifest)?, &record)?;
        if gcx.sess.opts.unstable.solar_tv_capture {
            return Ok(());
        }

        let report = path.join("result.json");
        let solar =
            env::var_os("SOLAR").map(Ok).unwrap_or_else(|| env::current_exe().map(Into::into))?;
        let output = Command::new(env::var_os("SOLAR_TV").unwrap_or_else(|| "solar-tv".into()))
            .arg("indexed-tables")
            .arg(&manifest)
            .arg("--report")
            .arg(&report)
            .env("SOLAR", solar)
            .output()?;
        let response = RealFileLoader.load_file(&report).ok().and_then(|text| {
            serde_json::from_str::<serde_json::Value>(&text)
                .ok()
                .filter(|value| value["version"] == 1)
        });
        if let Some(response) = response {
            match (output.status.code(), response["status"].as_str()) {
                (Some(0), Some("verified")) => return Ok(()),
                (Some(2), Some("unsupported" | "timeout")) => {
                    gcx.dcx()
                        .warn(format!(
                            "solar-tv: indexed table check for `{}` is inconclusive",
                            self.module,
                        ))
                        .note(
                            response["diagnostic"]
                                .as_str()
                                .unwrap_or("see audit report")
                                .to_owned(),
                        )
                        .note(format!("bytecode and layout saved in `{}`", path.display()))
                        .emit();
                    return Ok(());
                }
                _ => {}
            }
        }
        Err(io::Error::other(format!(
            "indexed table check failed ({}): {}\nbytecode and layout saved in `{}`",
            output.status,
            String::from_utf8_lossy(&output.stderr).trim(),
            path.display(),
        )))
    }
}
