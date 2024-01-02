use failure::{Error, Fail};
use protobuf::Message as _;
use std::fmt;
use std::fmt::Formatter;

use crate::proto::schema::witnet::SuperBlock;
use crate::{
    chain::Hash,
    proto::{
        schema::witnet::{
            Block, Block_BlockHeader, Block_BlockHeader_BlockMerkleRoots, Block_BlockTransactions,
            LegacyBlock, LegacyBlock_LegacyBlockHeader,
            LegacyBlock_LegacyBlockHeader_LegacyBlockMerkleRoots,
            LegacyBlock_LegacyBlockTransactions, LegacyMessage, LegacyMessage_LegacyCommand,
            LegacyMessage_LegacyCommand_oneof_kind, Message_Command, Message_Command_oneof_kind,
        },
        ProtobufConvert,
    },
    transaction::MemoizedHashable,
    types::Message,
};

#[derive(Clone, Copy, Debug, PartialEq)]
pub enum ProtocolVersion {
    /// The original Witnet protocol.
    V1_6,
    /// The transitional protocol based on 1.x but with staking enabled.
    V1_7,
    /// The final Witnet 2.0 protocol.
    V2_0,
}

impl fmt::Display for ProtocolVersion {
    fn fmt(&self, f: &mut Formatter<'_>) -> fmt::Result {
        let s = match self {
            ProtocolVersion::V1_6 => "v1.6 (legacy)",
            ProtocolVersion::V1_7 => "v1.7 (transitional)",
            ProtocolVersion::V2_0 => "v2.0 (final)",
        };

        f.write_str(s)
    }
}

pub trait Versioned: ProtobufConvert {
    type LegacyType: protobuf::Message;

    /// Turn a protobuf-compatible data structure into a versioned form of itself.
    ///
    /// For truly versionable data structures, this method should be implemented manually. For other
    /// data structures, the trait's own blanket implementation should be fine.
    fn to_versioned_pb(
        &self,
        _version: ProtocolVersion,
    ) -> Result<Box<dyn protobuf::Message>, Error>
    where
        <Self as ProtobufConvert>::ProtoStruct: protobuf::Message,
    {
        Ok(Box::new(self.to_pb()))
    }
    /// Turn a protobuf-compaitble data structures into its serialized protobuf bytes.
    /// This blanket implementation should normally not be overriden.
    fn to_versioned_pb_bytes(&self, version: ProtocolVersion) -> Result<Vec<u8>, Error>
    where
        <Self as ProtobufConvert>::ProtoStruct: protobuf::Message,
    {
        Ok(self.to_versioned_pb(version)?.write_to_bytes().unwrap())
    }

    /// Constructs an instance of this data structure based on a protobuf instance of its legacy
    /// schema.
    fn from_versioned_pb(legacy: Self::LegacyType) -> Result<Self, Error>
    where
        Self: From<Self::LegacyType>,
    {
        Ok(Self::from(legacy))
    }

    /// Tries to deserialize a data structure from its regular protobuf schema, and if it fails, it
    /// retries with its legacy schema.
    fn from_versioned_pb_bytes(bytes: &[u8]) -> Result<Self, Error>
    where
        <Self as ProtobufConvert>::ProtoStruct: protobuf::Message,
        Self: From<Self::LegacyType>,
    {
        let mut current = Self::ProtoStruct::new();
        let direct_attempt = current
            .merge_from_bytes(bytes)
            .map_err(|e| Error::from_boxed_compat(Box::new(e.compat())))
            .and_then(|_| Self::from_pb(current));

        if direct_attempt.is_ok() {
            direct_attempt
        } else {
            let mut legacy = Self::LegacyType::new();
            legacy.merge_from_bytes(bytes)?;

            Ok(Self::from(legacy))
        }
    }
}

impl Versioned for crate::chain::BlockMerkleRoots {
    type LegacyType = LegacyBlock_LegacyBlockHeader_LegacyBlockMerkleRoots;

    fn to_versioned_pb(
        &self,
        version: ProtocolVersion,
    ) -> Result<Box<dyn protobuf::Message>, Error> {
        use ProtocolVersion::*;

        let mut pb = self.to_pb();

        let versioned: Box<dyn protobuf::Message> = match version {
            // Legacy merkle roots need to get rearranged
            V1_6 => Box::new(Self::LegacyType::from(pb)),
            // Transition merkle roots need no transformation
            V1_7 => Box::new(pb),
            // Final merkle roots need to drop the mint hash
            V2_0 => {
                pb.set_mint_hash(Default::default());

                Box::new(pb)
            }
        };

        Ok(versioned)
    }
}

impl Versioned for crate::chain::BlockHeader {
    type LegacyType = LegacyBlock_LegacyBlockHeader;

    fn to_versioned_pb(
        &self,
        version: ProtocolVersion,
    ) -> Result<Box<dyn protobuf::Message>, Error> {
        use ProtocolVersion::*;

        let pb = self.to_pb();

        let versioned: Box<dyn protobuf::Message> = match version {
            // Legacy block headers need to be rearranged
            V1_6 => Box::new(Self::LegacyType::from(pb)),
            // All other block headers need no transformation
            V1_7 | V2_0 => Box::new(pb),
        };

        Ok(versioned)
    }
}

impl Versioned for crate::chain::SuperBlock {
    type LegacyType = SuperBlock;

    fn to_versioned_pb_bytes(&self, _version: ProtocolVersion) -> Result<Vec<u8>, Error>
    where
        <Self as ProtobufConvert>::ProtoStruct: protobuf::Message,
    {
        Ok(self.hashable_bytes())
    }
}

impl Versioned for crate::chain::Block {
    type LegacyType = LegacyBlock;

    fn to_versioned_pb(
        &self,
        _version: ProtocolVersion,
    ) -> Result<Box<dyn protobuf::Message>, Error>
    where
        <Self as ProtobufConvert>::ProtoStruct: protobuf::Message,
    {
        Ok(Box::new(Self::LegacyType::from(self.to_pb())))
    }
}

impl Versioned for Message {
    type LegacyType = LegacyMessage;

    fn to_versioned_pb(&self, version: ProtocolVersion) -> Result<Box<dyn protobuf::Message>, Error>
    where
        <Self as ProtobufConvert>::ProtoStruct: protobuf::Message,
    {
        use ProtocolVersion::*;

        let pb = self.to_pb();

        let versioned: Box<dyn protobuf::Message> = match version {
            V1_6 => Box::new(Self::LegacyType::from(pb)),
            V1_7 | V2_0 => Box::new(pb),
        };

        Ok(versioned)
    }
}

pub trait AutoVersioned: ProtobufConvert {}

impl AutoVersioned for crate::chain::BlockHeader {}
impl AutoVersioned for crate::chain::SuperBlock {}

pub trait VersionedHashable {
    fn versioned_hash(&self, version: ProtocolVersion) -> Hash;
}

impl<T> VersionedHashable for T
where
    T: AutoVersioned + Versioned,
    <Self as ProtobufConvert>::ProtoStruct: protobuf::Message,
{
    fn versioned_hash(&self, version: ProtocolVersion) -> Hash {
        witnet_crypto::hash::calculate_sha256(&self.to_versioned_pb_bytes(version).unwrap()).into()
    }
}

impl VersionedHashable for crate::chain::Block {
    fn versioned_hash(&self, version: ProtocolVersion) -> Hash {
        self.block_header.versioned_hash(version)
    }
}

impl From<Block_BlockHeader_BlockMerkleRoots>
    for LegacyBlock_LegacyBlockHeader_LegacyBlockMerkleRoots
{
    fn from(header: Block_BlockHeader_BlockMerkleRoots) -> Self {
        let mut legacy = LegacyBlock_LegacyBlockHeader_LegacyBlockMerkleRoots::new();
        legacy.set_mint_hash(header.get_mint_hash().clone());
        legacy.vt_hash_merkle_root = header.vt_hash_merkle_root;
        legacy.dr_hash_merkle_root = header.dr_hash_merkle_root;
        legacy.commit_hash_merkle_root = header.commit_hash_merkle_root;
        legacy.reveal_hash_merkle_root = header.reveal_hash_merkle_root;
        legacy.tally_hash_merkle_root = header.tally_hash_merkle_root;

        legacy
    }
}

impl From<LegacyBlock_LegacyBlockHeader_LegacyBlockMerkleRoots>
    for Block_BlockHeader_BlockMerkleRoots
{
    fn from(
        LegacyBlock_LegacyBlockHeader_LegacyBlockMerkleRoots {
            mint_hash,
            vt_hash_merkle_root,
            dr_hash_merkle_root,
            commit_hash_merkle_root,
            reveal_hash_merkle_root,
            tally_hash_merkle_root,
            ..
        }: LegacyBlock_LegacyBlockHeader_LegacyBlockMerkleRoots,
    ) -> Self {
        let mut header = Block_BlockHeader_BlockMerkleRoots::new();
        header.mint_hash = mint_hash;
        header.vt_hash_merkle_root = vt_hash_merkle_root;
        header.dr_hash_merkle_root = dr_hash_merkle_root;
        header.commit_hash_merkle_root = commit_hash_merkle_root;
        header.reveal_hash_merkle_root = reveal_hash_merkle_root;
        header.tally_hash_merkle_root = tally_hash_merkle_root;
        header.set_stake_hash_merkle_root(Hash::default().to_pb());
        header.set_unstake_hash_merkle_root(Hash::default().to_pb());

        header
    }
}

impl From<Block_BlockHeader> for LegacyBlock_LegacyBlockHeader {
    fn from(
        Block_BlockHeader {
            signals,
            beacon,
            merkle_roots,
            proof,
            bn256_public_key,
            ..
        }: Block_BlockHeader,
    ) -> Self {
        let mut legacy = LegacyBlock_LegacyBlockHeader::new();
        legacy.signals = signals;
        legacy.beacon = beacon;
        legacy.merkle_roots = merkle_roots.map(Into::into);
        legacy.proof = proof;
        legacy.bn256_public_key = bn256_public_key;

        legacy
    }
}

impl From<LegacyBlock_LegacyBlockHeader> for Block_BlockHeader {
    fn from(
        LegacyBlock_LegacyBlockHeader {
            signals,
            beacon,
            merkle_roots,
            proof,
            bn256_public_key,
            ..
        }: LegacyBlock_LegacyBlockHeader,
    ) -> Self {
        let mut header = Block_BlockHeader::new();
        header.signals = signals;
        header.beacon = beacon;
        header.merkle_roots = merkle_roots.map(Into::into);
        header.proof = proof;
        header.bn256_public_key = bn256_public_key;

        header
    }
}

impl From<Block_BlockTransactions> for LegacyBlock_LegacyBlockTransactions {
    fn from(
        Block_BlockTransactions {
            mint,
            value_transfer_txns,
            data_request_txns,
            commit_txns,
            reveal_txns,
            tally_txns,
            ..
        }: Block_BlockTransactions,
    ) -> Self {
        let mut legacy = LegacyBlock_LegacyBlockTransactions::new();
        legacy.mint = mint;
        legacy.value_transfer_txns = value_transfer_txns;
        legacy.data_request_txns = data_request_txns;
        legacy.commit_txns = commit_txns;
        legacy.reveal_txns = reveal_txns;
        legacy.tally_txns = tally_txns;

        legacy
    }
}

impl From<LegacyBlock_LegacyBlockTransactions> for Block_BlockTransactions {
    fn from(
        LegacyBlock_LegacyBlockTransactions {
            mint,
            value_transfer_txns,
            data_request_txns,
            commit_txns,
            reveal_txns,
            tally_txns,
            ..
        }: LegacyBlock_LegacyBlockTransactions,
    ) -> Self {
        let mut txns = Block_BlockTransactions::new();
        txns.mint = mint;
        txns.value_transfer_txns = value_transfer_txns;
        txns.data_request_txns = data_request_txns;
        txns.commit_txns = commit_txns;
        txns.reveal_txns = reveal_txns;
        txns.tally_txns = tally_txns;
        txns.stake_txns = vec![].into();
        txns.unstake_txns = vec![].into();

        txns
    }
}

impl From<Block> for LegacyBlock {
    fn from(
        Block {
            block_header,
            block_sig,
            txns,
            ..
        }: Block,
    ) -> Self {
        let mut legacy = LegacyBlock::new();
        legacy.block_header = block_header.map(Into::into);
        legacy.block_sig = block_sig;
        legacy.txns = txns.map(Into::into);

        legacy
    }
}

impl From<LegacyBlock> for Block {
    fn from(
        LegacyBlock {
            block_header,
            block_sig,
            txns,
            ..
        }: LegacyBlock,
    ) -> Self {
        let mut block = Block::new();
        block.block_header = block_header.map(Into::into);
        block.block_sig = block_sig;
        block.txns = txns.map(Into::into);

        block
    }
}

impl From<Message_Command_oneof_kind> for LegacyMessage_LegacyCommand_oneof_kind {
    fn from(value: Message_Command_oneof_kind) -> Self {
        match value {
            Message_Command_oneof_kind::Version(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::Version(x)
            }
            Message_Command_oneof_kind::Verack(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::Verack(x)
            }
            Message_Command_oneof_kind::GetPeers(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::GetPeers(x)
            }
            Message_Command_oneof_kind::Peers(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::Peers(x)
            }
            Message_Command_oneof_kind::Block(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::Block(x.into())
            }
            Message_Command_oneof_kind::InventoryAnnouncement(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::InventoryAnnouncement(x)
            }
            Message_Command_oneof_kind::InventoryRequest(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::InventoryRequest(x)
            }
            Message_Command_oneof_kind::LastBeacon(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::LastBeacon(x)
            }
            Message_Command_oneof_kind::Transaction(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::Transaction(x)
            }
            Message_Command_oneof_kind::SuperBlockVote(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::SuperBlockVote(x)
            }
            Message_Command_oneof_kind::SuperBlock(x) => {
                LegacyMessage_LegacyCommand_oneof_kind::SuperBlock(x)
            }
        }
    }
}

impl From<LegacyMessage_LegacyCommand_oneof_kind> for Message_Command_oneof_kind {
    fn from(legacy: LegacyMessage_LegacyCommand_oneof_kind) -> Self {
        match legacy {
            LegacyMessage_LegacyCommand_oneof_kind::Version(x) => {
                Message_Command_oneof_kind::Version(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::Verack(x) => {
                Message_Command_oneof_kind::Verack(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::GetPeers(x) => {
                Message_Command_oneof_kind::GetPeers(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::Peers(x) => {
                Message_Command_oneof_kind::Peers(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::Block(x) => {
                Message_Command_oneof_kind::Block(x.into())
            }
            LegacyMessage_LegacyCommand_oneof_kind::InventoryAnnouncement(x) => {
                Message_Command_oneof_kind::InventoryAnnouncement(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::InventoryRequest(x) => {
                Message_Command_oneof_kind::InventoryRequest(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::LastBeacon(x) => {
                Message_Command_oneof_kind::LastBeacon(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::Transaction(x) => {
                Message_Command_oneof_kind::Transaction(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::SuperBlockVote(x) => {
                Message_Command_oneof_kind::SuperBlockVote(x)
            }
            LegacyMessage_LegacyCommand_oneof_kind::SuperBlock(x) => {
                Message_Command_oneof_kind::SuperBlock(x)
            }
        }
    }
}

impl From<Message_Command> for LegacyMessage_LegacyCommand {
    fn from(Message_Command { kind, .. }: Message_Command) -> Self {
        let mut legacy = LegacyMessage_LegacyCommand::new();
        legacy.kind = kind.map(Into::into);

        legacy
    }
}

impl From<LegacyMessage_LegacyCommand> for Message_Command {
    fn from(LegacyMessage_LegacyCommand { kind, .. }: LegacyMessage_LegacyCommand) -> Self {
        let mut command = Message_Command::new();
        command.kind = kind.map(Into::into);

        command
    }
}

impl From<crate::proto::schema::witnet::Message> for LegacyMessage {
    fn from(
        crate::proto::schema::witnet::Message { magic, kind, .. }: crate::proto::schema::witnet::Message,
    ) -> Self {
        let mut legacy = LegacyMessage::new();
        legacy.magic = magic;
        legacy.kind = kind.map(Into::into);

        legacy
    }
}

impl From<LegacyMessage> for crate::proto::schema::witnet::Message {
    fn from(LegacyMessage { magic, kind, .. }: LegacyMessage) -> Self {
        let mut message = crate::proto::schema::witnet::Message::new();
        message.magic = magic;
        message.kind = kind.map(Into::into);

        message
    }
}

impl From<LegacyMessage> for Message {
    fn from(legacy: LegacyMessage) -> Self {
        let pb = crate::proto::schema::witnet::Message::from(legacy);

        Message::from_pb(pb).unwrap()
    }
}
