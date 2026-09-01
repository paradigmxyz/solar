//! Solidity contract metadata and bytecode auxiliary data.

use super::{
    compile::standard_json_source_name,
    data::{CompilerInput, MetadataHash, optimizer_settings},
};
use alloy_primitives::{Bytes, keccak256};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use solar_config::version::SEMVER_VERSION;
use solar_data_structures::{bit_set::GrowableBitSet, index::IndexVec};
use solar_sema::{
    Gcx,
    hir::{ContractId, SourceId},
};
use std::sync::OnceLock;

const INVALID: u8 = 0xfe;

/// Lazily computed metadata for a Standard JSON compilation.
pub(super) struct Metadata<'a, 'input, 'gcx> {
    gcx: Gcx<'gcx>,
    input: &'a CompilerInput<'input>,
    contracts: IndexVec<ContractId, OnceLock<String>>,
    sources: IndexVec<SourceId, OnceLock<Value>>,
    referenced_sources: IndexVec<SourceId, OnceLock<Vec<SourceId>>>,
}

impl<'a, 'input, 'gcx> Metadata<'a, 'input, 'gcx> {
    pub(super) fn new(gcx: Gcx<'gcx>, input: &'a CompilerInput<'input>) -> Self {
        let contracts = IndexVec::from_vec(
            (0..gcx.hir.contract_ids().len()).map(|_| Default::default()).collect(),
        );
        let sources =
            IndexVec::from_vec((0..gcx.hir.source_ids().len()).map(|_| OnceLock::new()).collect());
        let referenced_sources =
            IndexVec::from_vec((0..gcx.hir.source_ids().len()).map(|_| OnceLock::new()).collect());
        Self { gcx, input, contracts, sources, referenced_sources }
    }

    pub(super) fn json(&self, contract_id: ContractId) -> &str {
        self.contracts[contract_id].get_or_init(|| metadata_json(self, contract_id))
    }

    pub(super) fn runtime_suffix(&self, contract_id: ContractId) -> Bytes {
        let settings = self.input.settings.metadata;
        if !settings.append_cbor {
            return Bytes::new();
        }
        let cbor = cbor_metadata(self.json(contract_id), settings.bytecode_hash.value);
        let mut suffix = Vec::with_capacity(cbor.len() + 1);
        suffix.push(INVALID);
        suffix.extend(cbor);
        suffix.into()
    }

    fn source(&self, source_id: SourceId) -> &Value {
        self.sources[source_id].get_or_init(|| source_metadata(self, source_id))
    }

    fn referenced_sources(&self, source_id: SourceId) -> &[SourceId] {
        self.referenced_sources[source_id]
            .get_or_init(|| collect_referenced_sources(self.gcx, source_id))
    }
}

fn metadata_json(metadata: &Metadata<'_, '_, '_>, contract_id: ContractId) -> String {
    let gcx = metadata.gcx;
    let settings = metadata.input.settings.metadata;
    let contract = gcx.hir.contract(contract_id);
    let target_source_name = source_name(gcx, contract.source);
    let mut sources = Map::new();
    for &source_id in metadata.referenced_sources(contract.source) {
        sources.insert(source_name(gcx, source_id), metadata.source(source_id).clone());
    }

    let opts = &gcx.sess.opts;
    let mut metadata_settings = Map::new();
    if !settings.append_cbor {
        metadata_settings.insert("appendCBOR".into(), Value::Bool(false));
    }
    metadata_settings.insert(
        "bytecodeHash".into(),
        json!(match settings.bytecode_hash.value {
            MetadataHash::Ipfs => "ipfs",
            MetadataHash::Bzzr1 => "bzzr1",
            MetadataHash::None => "none",
        }),
    );
    if settings.use_literal_content {
        metadata_settings.insert("useLiteralContent".into(), Value::Bool(true));
    }

    let mut libraries = Map::new();
    for (source, source_libraries) in &metadata.input.settings.libraries.0 {
        for (name, address) in source_libraries {
            let name =
                if source.is_empty() { name.to_string() } else { format!("{source}:{name}") };
            libraries.insert(name, json!(format!("{address:#x}")));
        }
    }
    let mut remappings = opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>();
    remappings.sort_unstable();

    let (optimizer_enabled, optimizer_runs) =
        optimizer_settings(metadata.input.settings.optimizer.as_ref());
    let value = json!({
        "compiler": { "version": SEMVER_VERSION },
        "language": "Solidity",
        "output": {
            "abi": gcx.contract_abi(contract_id),
            "devdoc": gcx.dev_documentation(contract_id),
            "userdoc": gcx.user_documentation(contract_id),
        },
        "settings": {
            "compilationTarget": { target_source_name: contract.name.as_str() },
            "evmVersion": opts.evm_version.to_string(),
            "libraries": libraries,
            "metadata": metadata_settings,
            "optimizer": {
                "enabled": optimizer_enabled,
                "runs": optimizer_runs,
            },
            "remappings": remappings,
        },
        "sources": sources,
        "version": 1,
    });
    serde_json::to_string(&value).expect("contract metadata must serialize")
}

fn source_metadata(metadata: &Metadata<'_, '_, '_>, source_id: SourceId) -> Value {
    let gcx = metadata.gcx;
    let source = gcx.hir.source(source_id);
    let content = source.file.src.as_str();
    let mut value = Map::new();
    value.insert("keccak256".into(), json!(format!("{:#x}", keccak256(content.as_bytes()))));
    if metadata.input.settings.metadata.use_literal_content {
        value.insert("content".into(), json!(content));
    } else {
        let swarm = bzzr1_hash(content.as_bytes());
        let ipfs = ipfs_hash(content.as_bytes());
        value.insert(
            "urls".into(),
            json!([
                format!("bzz-raw://{}", alloy_primitives::hex::encode(swarm)),
                format!("dweb:/ipfs/{}", bs58::encode(ipfs).into_string()),
            ]),
        );
    }
    Value::Object(value)
}

fn source_name(gcx: Gcx<'_>, source_id: SourceId) -> String {
    standard_json_source_name(&gcx.hir.source(source_id).file.name)
}

fn collect_referenced_sources(gcx: Gcx<'_>, root: SourceId) -> Vec<SourceId> {
    fn visit(gcx: Gcx<'_>, source_id: SourceId, sources: &mut GrowableBitSet<SourceId>) {
        if !sources.insert(source_id) {
            return;
        }
        for &(_, imported) in gcx.hir.source(source_id).imports {
            visit(gcx, imported, sources);
        }
    }

    let mut sources = GrowableBitSet::new_empty();
    visit(gcx, root, &mut sources);
    let mut sources = sources.iter().collect::<Vec<_>>();
    sources.sort_unstable_by_key(|&source_id| source_name(gcx, source_id));
    sources
}

fn cbor_metadata(metadata: &str, hash: MetadataHash) -> Vec<u8> {
    let mut entries = Vec::with_capacity(2);
    match hash {
        MetadataHash::Ipfs => {
            push_cbor_bytes(&mut entries, "ipfs", &ipfs_hash(metadata.as_bytes()))
        }
        MetadataHash::Bzzr1 => {
            push_cbor_bytes(&mut entries, "bzzr1", &bzzr1_hash(metadata.as_bytes()))
        }
        MetadataHash::None => {}
    }
    let version = semver::Version::parse(SEMVER_VERSION).expect("package version must be semver");
    push_cbor_bytes(
        &mut entries,
        "solc",
        &[version.major as u8, version.minor as u8, version.patch as u8],
    );

    let entry_count = if matches!(hash, MetadataHash::None) { 1 } else { 2 };
    let mut output = Vec::with_capacity(entries.len() + 3);
    output.push(0xa0 + entry_count);
    output.extend(entries);
    let length = u16::try_from(output.len()).expect("contract metadata CBOR is too large");
    output.extend(length.to_be_bytes());
    output
}

fn push_cbor_bytes(output: &mut Vec<u8>, key: &str, value: &[u8]) {
    push_cbor_value(output, 0x60, key.as_bytes());
    push_cbor_value(output, 0x40, value);
}

fn push_cbor_value(output: &mut Vec<u8>, kind: u8, value: &[u8]) {
    if value.len() < 24 {
        output.push(kind + value.len() as u8);
    } else {
        output.extend([kind + 24, u8::try_from(value.len()).expect("CBOR value is too large")]);
    }
    output.extend(value);
}

fn ipfs_hash(input: &[u8]) -> Vec<u8> {
    const CHUNK_SIZE: usize = 256 * 1024;
    let mut chunks = input.chunks(CHUNK_SIZE).map(ipfs_leaf).collect::<Vec<_>>();
    if chunks.is_empty() {
        chunks.push(ipfs_leaf(&[]));
    }
    while chunks.len() > 1 {
        chunks = chunks.chunks_mut(174).map(ipfs_parent).collect();
    }
    chunks.pop().expect("IPFS tree must have a root").hash
}

struct IpfsChunk {
    hash: Vec<u8>,
    size: usize,
    block_size: usize,
}

fn ipfs_leaf(input: &[u8]) -> IpfsChunk {
    let protobuf_len = 2
        + if input.is_empty() { 0 } else { 1 + varint_len(input.len()) + input.len() }
        + 1
        + varint_len(input.len());
    let mut block = Vec::with_capacity(1 + varint_len(protobuf_len) + protobuf_len);
    block.push(0x0a);
    push_varint(&mut block, protobuf_len);
    block.extend([0x08, 0x02]);
    if !input.is_empty() {
        block.push(0x12);
        push_varint(&mut block, input.len());
        block.extend(input);
    }
    block.push(0x18);
    push_varint(&mut block, input.len());
    IpfsChunk { hash: sha256_multihash(&block), size: input.len(), block_size: block.len() }
}

fn ipfs_parent(children: &mut [IpfsChunk]) -> IpfsChunk {
    let mut links = Vec::new();
    let mut lengths = Vec::new();
    let mut size = 0;
    let mut block_size = 0;
    for child in children {
        size += child.size;
        block_size += child.block_size;
        let mut link = vec![0x0a];
        push_varint(&mut link, child.hash.len());
        link.append(&mut child.hash);
        link.extend([0x12, 0x00, 0x18]);
        push_varint(&mut link, child.block_size);
        links.push(0x12);
        push_varint(&mut links, link.len());
        links.extend(link);
        lengths.push(0x20);
        push_varint(&mut lengths, child.size);
    }
    let mut file = vec![0x08, 0x02, 0x18];
    push_varint(&mut file, size);
    file.extend(lengths);
    let mut data = vec![0x0a];
    push_varint(&mut data, file.len());
    data.extend(file);
    links.extend(data);
    block_size += links.len();
    IpfsChunk { hash: sha256_multihash(&links), size, block_size }
}

fn sha256_multihash(input: &[u8]) -> Vec<u8> {
    let mut output = vec![0x12, 0x20];
    output.extend(Sha256::digest(input));
    output
}

fn push_varint(output: &mut Vec<u8>, mut value: usize) {
    while value > 0x7f {
        output.push(0x80 | (value as u8 & 0x7f));
        value >>= 7;
    }
    output.push(value as u8);
}

fn varint_len(value: usize) -> usize {
    ((usize::BITS - value.leading_zeros()).max(1) as usize).div_ceil(7)
}

fn bzzr1_hash(input: &[u8]) -> [u8; 32] {
    if input.is_empty() {
        return [0; 32];
    }
    bzzr1_chunk(input, false)
}

fn bzzr1_chunk(input: &[u8], force_higher: bool) -> [u8; 32] {
    let higher;
    let data = if input.len() < 0x1000 || input.len() == 0x1000 && !force_higher {
        input
    } else {
        let mut represented = 0x1000;
        while represented * (0x1000 / 32) < input.len() {
            represented *= 0x1000 / 32;
        }
        higher = input
            .chunks(represented)
            .flat_map(|chunk| bzzr1_chunk(chunk, represented > 0x1000))
            .collect::<Vec<_>>();
        &higher
    };
    let mut padded = Vec::with_capacity(0x1000);
    padded.extend_from_slice(data);
    padded.resize(0x1000, 0);
    let mut value = [0; 40];
    value[..8].copy_from_slice(&(input.len() as u64).to_le_bytes());
    value[8..].copy_from_slice(&bmt_hash(&padded));
    keccak256(value).into()
}

fn bmt_hash(input: &[u8]) -> [u8; 32] {
    if input.len() <= 64 {
        return keccak256(input).into();
    }
    let middle = input.len() / 2;
    let mut value = [0; 64];
    value[..32].copy_from_slice(&bmt_hash(&input[..middle]));
    value[32..].copy_from_slice(&bmt_hash(&input[middle..]));
    keccak256(value).into()
}

#[cfg(test)]
mod tests {
    use super::*;

    #[test]
    fn ipfs_hash_matches_solc() {
        assert_eq!(
            bs58::encode(ipfs_hash(b"")).into_string(),
            "QmbFMke1KXqnYyBBWxB74N4c5SBnJMVAiMNRcGu6x1AwQH"
        );
        assert_eq!(
            bs58::encode(ipfs_hash(b"Solidity\n")).into_string(),
            "QmSsm9M7PQRBnyiz1smizk8hZw3URfk8fSeHzeTo3oZidS"
        );
        assert_eq!(
            bs58::encode(ipfs_hash(&[0; 10250])).into_string(),
            "QmVJJBB3gKKBWYC9QTywpH8ZL1bDeTDJ17B63Af5kino9i"
        );
    }

    #[test]
    fn bzzr1_hash_matches_solc() {
        assert_eq!(bzzr1_hash(b""), [0; 32]);
        assert_eq!(
            alloy_primitives::hex::encode(bzzr1_hash(b"hello world")),
            "92672a471f4419b255d7cb0cf313474a6f5856fb347c5ece85fb706d644b630f"
        );
        assert_eq!(
            alloy_primitives::hex::encode(bzzr1_hash(&[0; 4097])),
            "c082943c4cb8a97c67947f290f5421cf4c61d021eb303c8df77de6fe208df516"
        );
    }

    #[test]
    fn cbor_metadata_matches_solc_shape() {
        let metadata = cbor_metadata("{}", MetadataHash::None);
        assert_eq!(&metadata[..6], &[0xa1, 0x64, b's', b'o', b'l', b'c']);
        assert_eq!(
            usize::from(u16::from_be_bytes([
                metadata[metadata.len() - 2],
                metadata[metadata.len() - 1]
            ])),
            metadata.len() - 2
        );
    }
}
