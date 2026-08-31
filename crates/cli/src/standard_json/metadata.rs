//! Solidity contract metadata and bytecode auxiliary data.

use super::data::{MetadataHash, MetadataSettings};
use alloy_primitives::{Bytes, keccak256};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use solar_config::{OptimizationMode, version::SEMVER_VERSION};
use solar_data_structures::{bit_set::GrowableBitSet, map::FxHashMap};
use solar_sema::{
    Gcx,
    hir::{ContractId, SourceId},
};

/// Metadata emitted for one contract.
pub(super) struct ContractMetadata {
    pub(super) json: String,
    pub(super) cbor: Bytes,
}

pub(super) fn build(
    gcx: Gcx<'_>,
    settings: MetadataSettings,
) -> FxHashMap<ContractId, ContractMetadata> {
    gcx.hir
        .contracts_enumerated()
        .map(|(contract_id, _)| {
            let json = metadata_json(gcx, contract_id, settings);
            let cbor = settings.append_cbor.then(|| cbor_metadata(&json, settings.bytecode_hash));
            (contract_id, ContractMetadata { json, cbor: cbor.unwrap_or_default().into() })
        })
        .collect()
}

fn metadata_json(gcx: Gcx<'_>, contract_id: ContractId, metadata: MetadataSettings) -> String {
    let contract = gcx.hir.contract(contract_id);
    let target_source_name = source_name(gcx, contract.source);
    let mut sources = Map::new();
    for source_id in referenced_sources(gcx, contract.source) {
        let source = gcx.hir.source(source_id);
        let content = source.file.src.as_str();
        let mut value = Map::new();
        value.insert("keccak256".into(), json!(format!("{:#x}", keccak256(content.as_bytes()))));
        if let Some(license) = source_license(content) {
            value.insert("license".into(), json!(license));
        }
        if metadata.use_literal_content {
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
        sources.insert(source_name(gcx, source_id), Value::Object(value));
    }

    let opts = &gcx.sess.opts;
    let mut metadata_settings = Map::new();
    if !metadata.append_cbor {
        metadata_settings.insert("appendCBOR".into(), Value::Bool(false));
    }
    metadata_settings.insert(
        "bytecodeHash".into(),
        json!(match metadata.bytecode_hash {
            MetadataHash::Ipfs => "ipfs",
            MetadataHash::Bzzr1 => "bzzr1",
            MetadataHash::None => "none",
        }),
    );
    if metadata.use_literal_content {
        metadata_settings.insert("useLiteralContent".into(), Value::Bool(true));
    }

    let mut libraries = Map::new();
    for library in &opts.libraries {
        let name = library
            .source
            .as_ref()
            .map_or_else(|| library.name.clone(), |source| format!("{source}:{}", library.name));
        libraries.insert(name, json!(format!("{:#x}", library.address)));
    }
    let mut remappings = opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>();
    remappings.sort_unstable();

    let optimizer_enabled = !matches!(opts.optimization, OptimizationMode::None);
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
                "runs": opts.optimizer_runs.unwrap_or(200),
            },
            "remappings": remappings,
        },
        "sources": sources,
        "version": 1,
    });
    serde_json::to_string(&value).expect("contract metadata must serialize")
}

fn source_name(gcx: Gcx<'_>, source_id: SourceId) -> String {
    gcx.hir.source(source_id).file.name.display().to_string().replace('\\', "/")
}

fn source_license(source: &str) -> Option<&str> {
    const PREFIX: &str = "SPDX-License-Identifier:";
    source.lines().find_map(|line| {
        line.trim_start().strip_prefix("//")?.trim().strip_prefix(PREFIX).map(str::trim)
    })
}

fn referenced_sources(gcx: Gcx<'_>, root: SourceId) -> impl Iterator<Item = SourceId> {
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
    sources.into_iter()
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
    let mut protobuf = vec![0x08, 0x02];
    if !input.is_empty() {
        protobuf.push(0x12);
        push_varint(&mut protobuf, input.len());
        protobuf.extend(input);
    }
    protobuf.push(0x18);
    push_varint(&mut protobuf, input.len());
    let mut block = vec![0x0a];
    push_varint(&mut block, protobuf.len());
    block.extend(protobuf);
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

fn bzzr1_hash(input: &[u8]) -> [u8; 32] {
    if input.is_empty() {
        return [0; 32];
    }
    bzzr1_chunk(input, false)
}

fn bzzr1_chunk(input: &[u8], force_higher: bool) -> [u8; 32] {
    let data = if input.len() < 0x1000 || input.len() == 0x1000 && !force_higher {
        input.to_vec()
    } else {
        let mut represented = 0x1000;
        while represented * (0x1000 / 32) < input.len() {
            represented *= 0x1000 / 32;
        }
        input
            .chunks(represented)
            .flat_map(|chunk| bzzr1_chunk(chunk, represented > 0x1000))
            .collect()
    };
    let mut padded = data;
    padded.resize(0x1000, 0);
    let mut value = (input.len() as u64).to_le_bytes().to_vec();
    value.extend(bmt_hash(&padded));
    keccak256(value).into()
}

fn bmt_hash(input: &[u8]) -> [u8; 32] {
    if input.len() <= 64 {
        return keccak256(input).into();
    }
    let middle = input.len() / 2;
    let mut value = bmt_hash(&input[..middle]).to_vec();
    value.extend(bmt_hash(&input[middle..]));
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
