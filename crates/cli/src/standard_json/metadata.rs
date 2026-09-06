//! Solidity contract metadata and bytecode auxiliary data.

use super::{
    compile::standard_json_source_name,
    data::{MetadataHash, Settings, optimizer_settings},
};
use alloy_primitives::{Bytes, keccak256};
use serde_json::{Map, Value, json};
use sha2::{Digest, Sha256};
use solar_config::{RevertStrings, version::SEMVER_VERSION};
use solar_data_structures::{bit_set::GrowableBitSet, index::IndexVec};
use solar_sema::{
    Gcx,
    hir::{ContractId, SourceId},
};
use std::sync::OnceLock;

const INVALID: u8 = 0xfe;
const IPFS_MULTIHASH_LEN: usize = 34;

impl MetadataHash {
    const fn name(self) -> &'static str {
        match self {
            Self::Ipfs => "ipfs",
            Self::Bzzr1 => "bzzr1",
            Self::None => "none",
        }
    }

    const fn cbor_value_len(self) -> Option<usize> {
        match self {
            Self::Ipfs => Some(IPFS_MULTIHASH_LEN),
            Self::Bzzr1 => Some(32),
            Self::None => None,
        }
    }
}

/// Lazily computed metadata for a Standard JSON compilation.
pub(super) struct Metadata<'a, 'input, 'gcx> {
    gcx: Gcx<'gcx>,
    settings: &'a Settings<'input>,
    contracts: IndexVec<ContractId, OnceLock<String>>,
    sources: IndexVec<SourceId, OnceLock<Value>>,
    referenced_sources: IndexVec<SourceId, OnceLock<Vec<SourceId>>>,
}

impl<'a, 'input, 'gcx> Metadata<'a, 'input, 'gcx> {
    pub(super) fn new(gcx: Gcx<'gcx>, settings: &'a Settings<'input>) -> Self {
        let contracts = IndexVec::from_vec(
            (0..gcx.hir.contract_ids().len()).map(|_| Default::default()).collect(),
        );
        let sources =
            IndexVec::from_vec((0..gcx.hir.source_ids().len()).map(|_| OnceLock::new()).collect());
        let referenced_sources =
            IndexVec::from_vec((0..gcx.hir.source_ids().len()).map(|_| OnceLock::new()).collect());
        Self { gcx, settings, contracts, sources, referenced_sources }
    }

    pub(super) fn json(&self, contract_id: ContractId) -> &str {
        self.contracts[contract_id].get_or_init(|| metadata_json(self, contract_id))
    }

    pub(super) fn runtime_data(&self, contract_id: ContractId) -> Bytes {
        let settings = self.settings.metadata;
        if !settings.append_cbor {
            return Bytes::new();
        }
        let hash = settings.bytecode_hash.value;
        let mut data = Vec::with_capacity(cbor_metadata_len(hash) + 1);
        data.push(INVALID);
        match hash {
            MetadataHash::Ipfs | MetadataHash::Bzzr1 => {
                push_cbor_metadata(&mut data, self.json(contract_id), hash);
            }
            MetadataHash::None => push_cbor_metadata(&mut data, "", hash),
        }
        data.into()
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
    let settings = metadata.settings.metadata;
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
    metadata_settings.insert("bytecodeHash".into(), json!(settings.bytecode_hash.value.name()));
    if settings.use_literal_content {
        metadata_settings.insert("useLiteralContent".into(), Value::Bool(true));
    }

    let mut libraries = Map::new();
    for (source, source_libraries) in &metadata.settings.libraries.0 {
        for (name, address) in source_libraries {
            let name =
                if source.is_empty() { name.to_string() } else { format!("{source}:{name}") };
            libraries.insert(name, json!(format!("{address:#x}")));
        }
    }
    let mut remappings = opts.import_remappings.iter().map(ToString::to_string).collect::<Vec<_>>();
    remappings.sort_unstable();

    let (optimizer_enabled, optimizer_runs) =
        optimizer_settings(metadata.settings.optimizer.as_ref());
    let mut value = json!({
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
    if opts.revert_strings != RevertStrings::Default {
        value["settings"]["debug"] = json!({ "revertStrings": opts.revert_strings.to_string() });
    }
    serde_json::to_string(&value).expect("contract metadata must serialize")
}

fn source_metadata(metadata: &Metadata<'_, '_, '_>, source_id: SourceId) -> Value {
    let gcx = metadata.gcx;
    let source = gcx.hir.source(source_id);
    let content = source.file.src.as_str();
    let mut value = Map::new();
    value.insert("keccak256".into(), json!(format!("{:#x}", keccak256(content.as_bytes()))));
    if metadata.settings.metadata.use_literal_content {
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

const fn cbor_bytes_len(key_len: usize, value_len: usize) -> usize {
    let key_header_len = if key_len < 24 { 1 } else { 2 };
    let value_header_len = if value_len < 24 { 1 } else { 2 };
    key_header_len + key_len + value_header_len + value_len
}

const fn cbor_metadata_len(hash: MetadataHash) -> usize {
    let hash_entry_len = match hash.cbor_value_len() {
        Some(value_len) => cbor_bytes_len(hash.name().len(), value_len),
        None => 0,
    };
    1 + hash_entry_len + cbor_bytes_len("solar".len(), 3) + 2
}

fn push_cbor_metadata(output: &mut Vec<u8>, metadata: &str, hash: MetadataHash) {
    let start = output.len();
    match hash {
        MetadataHash::Ipfs => {
            output.push(0xa2);
            push_cbor_bytes(output, hash.name(), &ipfs_hash(metadata.as_bytes()));
        }
        MetadataHash::Bzzr1 => {
            output.push(0xa2);
            push_cbor_bytes(output, hash.name(), &bzzr1_hash(metadata.as_bytes()));
        }
        MetadataHash::None => {
            output.push(0xa1);
        }
    }
    let version = semver::Version::parse(SEMVER_VERSION).expect("package version must be semver");
    push_cbor_bytes(
        output,
        "solar",
        &[version.major as u8, version.minor as u8, version.patch as u8],
    );

    let length = u16::try_from(output.len() - start).expect("contract metadata CBOR is too large");
    output.extend(length.to_be_bytes());
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

fn ipfs_hash(input: &[u8]) -> [u8; IPFS_MULTIHASH_LEN] {
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
    hash: [u8; IPFS_MULTIHASH_LEN],
    size: usize,
    block_size: usize,
}

fn ipfs_leaf(input: &[u8]) -> IpfsChunk {
    let protobuf_len = 2
        + if input.is_empty() { 0 } else { 1 + varint_len(input.len()) + input.len() }
        + 1
        + varint_len(input.len());
    let mut protobuf_len_bytes = [0; 10];
    let protobuf_len_len = write_varint(protobuf_len, &mut protobuf_len_bytes);
    let mut input_len_bytes = [0; 10];
    let input_len_len = write_varint(input.len(), &mut input_len_bytes);
    let protobuf_len_bytes = &protobuf_len_bytes[..protobuf_len_len];
    let input_len_bytes = &input_len_bytes[..input_len_len];
    let mut hasher = Sha256::new();
    hasher.update([0x0a]);
    hasher.update(protobuf_len_bytes);
    hasher.update([0x08, 0x02]);
    if !input.is_empty() {
        hasher.update([0x12]);
        hasher.update(input_len_bytes);
        hasher.update(input);
    }
    hasher.update([0x18]);
    hasher.update(input_len_bytes);
    let block_size = 4
        + protobuf_len_bytes.len()
        + input_len_bytes.len() * usize::from(!input.is_empty())
        + input.len()
        + input_len_bytes.len();
    let hash = sha256_multihash_from_hasher(hasher);
    IpfsChunk { hash, size: input.len(), block_size }
}

fn ipfs_parent(children: &mut [IpfsChunk]) -> IpfsChunk {
    let mut links = Vec::new();
    let mut lengths = Vec::new();
    let mut size = 0;
    let mut block_size = 0;
    for child in children {
        size += child.size;
        block_size += child.block_size;
        links.push(0x12);
        let link_len =
            1 + varint_len(child.hash.len()) + child.hash.len() + 3 + varint_len(child.block_size);
        push_varint(&mut links, link_len);
        links.push(0x0a);
        push_varint(&mut links, child.hash.len());
        links.extend(child.hash);
        links.extend([0x12, 0x00, 0x18]);
        push_varint(&mut links, child.block_size);
        lengths.push(0x20);
        push_varint(&mut lengths, child.size);
    }
    let file_len = 3 + varint_len(size) + lengths.len();
    links.push(0x0a);
    push_varint(&mut links, file_len);
    links.extend([0x08, 0x02, 0x18]);
    push_varint(&mut links, size);
    links.extend(lengths);
    block_size += links.len();
    IpfsChunk { hash: sha256_multihash(&links), size, block_size }
}

fn sha256_multihash(input: &[u8]) -> [u8; IPFS_MULTIHASH_LEN] {
    let mut hasher = Sha256::new();
    hasher.update(input);
    sha256_multihash_from_hasher(hasher)
}

fn sha256_multihash_from_hasher(hasher: Sha256) -> [u8; IPFS_MULTIHASH_LEN] {
    let mut output = [0; IPFS_MULTIHASH_LEN];
    output[..2].copy_from_slice(&[0x12, 0x20]);
    output[2..].copy_from_slice(&hasher.finalize());
    output
}

fn push_varint(output: &mut Vec<u8>, mut value: usize) {
    while value > 0x7f {
        output.push(0x80 | (value as u8 & 0x7f));
        value >>= 7;
    }
    output.push(value as u8);
}

fn write_varint(mut value: usize, output: &mut [u8; 10]) -> usize {
    let mut len = 0;
    while value > 0x7f {
        output[len] = 0x80 | (value as u8 & 0x7f);
        len += 1;
        value >>= 7;
    }
    output[len] = value as u8;
    len + 1
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
    let hash = if input.len() == 0x1000 && !force_higher {
        bmt_hash(input)
    } else {
        let mut padded = [0; 0x1000];
        if input.len() < 0x1000 {
            padded[..input.len()].copy_from_slice(input);
        } else {
            let mut represented = 0x1000;
            while represented * (0x1000 / 32) < input.len() {
                represented *= 0x1000 / 32;
            }
            for (output, chunk) in
                padded.as_chunks_mut::<32>().0.iter_mut().zip(input.chunks(represented))
            {
                output.copy_from_slice(&bzzr1_chunk(chunk, represented > 0x1000));
            }
        }
        bmt_hash(&padded)
    };
    let mut value = [0; 40];
    value[..8].copy_from_slice(&(input.len() as u64).to_le_bytes());
    value[8..].copy_from_slice(&hash);
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
        let hash = MetadataHash::None;
        let mut metadata = Vec::with_capacity(cbor_metadata_len(hash));
        push_cbor_metadata(&mut metadata, "{}", hash);
        assert_eq!(&metadata[..7], &[0xa1, 0x65, b's', b'o', b'l', b'a', b'r']);
        assert_eq!(metadata.len(), cbor_metadata_len(hash));
        assert_eq!(
            usize::from(u16::from_be_bytes([
                metadata[metadata.len() - 2],
                metadata[metadata.len() - 1]
            ])),
            metadata.len() - 2
        );
    }
}
