use crate::ty::Gcx;
use serde::Serialize;
use solar_interface::config::CompilerOutput;
use std::{
    collections::BTreeMap,
    io::{self, Write},
    path::Path,
};

#[derive(Default, Serialize)]
struct CombinedJson {
    #[serde(skip_serializing_if = "BTreeMap::is_empty")]
    contracts: BTreeMap<String, CombinedJsonContract>,
    version: &'static str,
}

#[derive(Default, Serialize)]
struct CombinedJsonContract {
    #[serde(skip_serializing_if = "Option::is_none")]
    abi: Option<Abi>,
    #[serde(skip_serializing_if = "Option::is_none")]
    hashes: Option<Hashes>,
}

type Abi = Vec<alloy_json_abi::AbiItem<'static>>;
type Hashes = BTreeMap<String, String>;

pub(crate) fn emit(gcx: Gcx<'_>) {
    let mut output =
        CombinedJson { contracts: Default::default(), version: solar_interface::VERSION };
    for id in gcx.hir.contract_ids() {
        let name = gcx.contract_fully_qualified_name(id).to_string();
        let contract_output = output.contracts.entry(name).or_default();
        for &emit in &gcx.sess.opts.emit {
            match emit {
                CompilerOutput::Abi => contract_output.abi = Some(gcx.contract_abi(id)),
                CompilerOutput::Hashes => {
                    let mut hashes = Hashes::default();
                    for f in gcx.interface_functions(id) {
                        hashes.insert(
                            gcx.item_signature(f.id.into()).to_string(),
                            alloy_primitives::hex::encode(f.selector),
                        );
                    }
                    contract_output.hashes = Some(hashes);
                }
            }
        }
    }
    let _ = (|| {
        let out_path = gcx.sess.opts.out_dir.as_deref().map(|dir| dir.join("combined.json"));
        let mut writer = out_writer(out_path.as_deref())?;
        to_json(&mut writer, &output, gcx.sess.opts.pretty_json)?;
        writer.flush()?;
        Ok::<_, io::Error>(())
    })()
    .map_err(|e| gcx.dcx().err(format!("failed to write to output: {e}")).emit());
}

fn out_writer(path: Option<&Path>) -> io::Result<impl io::Write> {
    let out: Box<dyn io::Write> = if let Some(path) = path {
        Box::new(std::fs::File::create(path)?)
    } else {
        Box::new(std::io::stdout())
    };
    Ok(io::BufWriter::new(out))
}

fn to_json<W: io::Write, T: Serialize>(
    writer: W,
    value: &T,
    pretty: bool,
) -> serde_json::Result<()> {
    if pretty {
        serde_json::to_writer_pretty(writer, value)
    } else {
        serde_json::to_writer(writer, value)
    }
}
