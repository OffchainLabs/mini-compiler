/*
 * Copyright 2020, Offchain Labs, Inc. All rights reserved.
 */

use crate::compile::DebugInfo;
use crate::stringtable::StringId;
use crate::uint256::Uint256;
use crate::upload::CodeUploader;
use ethers_core::utils::keccak256;
use serde::de::Visitor;
use serde::{de, Deserialize, Deserializer, Serialize, Serializer};
use serde_repr::{Deserialize_repr, Serialize_repr};
use std::{collections::HashMap, fmt, sync::Arc};

#[derive(PartialEq, Eq, Hash, Debug, Clone, Copy, Serialize, Deserialize)]
pub enum Label {
    Func(StringId),
    Anon(usize),
    External(usize), // slot in imported funcs list
    Evm(usize),      // program counter in EVM contract
}

impl Label {
    pub fn relocate(
        self,
        int_offset: usize,
        ext_offset: usize,
        func_offset: usize,
    ) -> (Self, usize) {
        match self {
            Label::Func(sid) => (Label::Func(sid + func_offset), sid + func_offset),
            Label::Anon(pc) => (Label::Anon(pc + int_offset), func_offset),
            Label::External(slot) => (Label::External(slot + ext_offset), func_offset),
            Label::Evm(_) => (self, func_offset),
        }
    }

    pub fn avm_hash(&self) -> Value {
        match self {
            Label::Func(sid) => Value::avm_hash2(
                &Value::Int(Uint256::from_usize(4)),
                &Value::Int(Uint256::from_usize(*sid)),
            ),
            Label::Anon(n) => Value::avm_hash2(
                &Value::Int(Uint256::from_usize(5)),
                &Value::Int(Uint256::from_usize(*n)),
            ),
            Label::External(n) => Value::avm_hash2(
                &Value::Int(Uint256::from_usize(6)),
                &Value::Int(Uint256::from_usize(*n)),
            ),
            Label::Evm(_) => {
                panic!("tried to avm_hash an EVM label");
            }
        }
    }
}

impl fmt::Display for Label {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Label::Func(sid) => write!(f, "function_{}", sid),
            Label::Anon(n) => write!(f, "label_{}", n),
            Label::External(slot) => write!(f, "external_{}", slot),
            Label::Evm(pc) => write!(f, "EvmPC({})", pc),
        }
    }
}

#[derive(Default)]
pub struct LabelGenerator {
    next: usize,
}

impl LabelGenerator {
    pub fn new() -> Self {
        LabelGenerator { next: 0 }
    }

    pub fn next(self) -> (Label, Self) {
        (
            Label::Anon(self.next),
            LabelGenerator {
                next: self.next + 1,
            },
        )
    }
}

#[derive(Debug, Clone, Serialize, Deserialize, PartialEq, Eq)]
pub struct Instruction<T = Opcode> {
    pub opcode: T,
    pub immediate: Option<Value>,
    #[serde(default)]
    pub debug_info: DebugInfo,
}

impl From<Instruction<AVMOpcode>> for Instruction {
    fn from(from: Instruction<AVMOpcode>) -> Self {
        let Instruction {
            opcode,
            immediate,
            debug_info,
        } = from;
        Self {
            opcode: opcode.into(),
            immediate,
            debug_info,
        }
    }
}

impl<T> Instruction<T> {
    pub fn new(opcode: T, immediate: Option<Value>, debug_info: DebugInfo) -> Self {
        Instruction {
            opcode,
            immediate,
            debug_info,
        }
    }

    pub fn from_opcode(opcode: T, debug_info: DebugInfo) -> Self {
        Instruction::new(opcode, None, debug_info)
    }

    pub fn from_opcode_imm(opcode: T, immediate: Value, debug_info: DebugInfo) -> Self {
        Instruction::new(opcode, Some(immediate), debug_info)
    }

    pub fn replace_labels(self, label_map: &HashMap<Label, CodePt>) -> Result<Self, Label> {
        match self.immediate {
            Some(val) => Ok(Instruction::from_opcode_imm(
                self.opcode,
                val.replace_labels(label_map)?,
                self.debug_info,
            )),
            None => Ok(self),
        }
    }

    pub fn xlate_labels(self, xlate_map: &HashMap<Label, &Label>) -> Self {
        match self.immediate {
            Some(val) => Instruction::from_opcode_imm(
                self.opcode,
                val.xlate_labels(xlate_map),
                self.debug_info,
            ),
            None => self,
        }
    }
}

impl Instruction<AVMOpcode> {
    pub fn _upload(&self, u: &mut CodeUploader) {
        u._push_byte(self.opcode.to_number());
        if let Some(val) = &self.immediate {
            u._push_byte(1u8);
            val._upload(u);
        } else {
            u._push_byte(0u8);
        }
    }
}

impl Instruction {
    pub fn is_pure(&self) -> bool {
        self.opcode.is_pure()
    }
    pub fn get_label(&self) -> Option<&Label> {
        match &self.opcode {
            Opcode::Label(label) => Some(label),
            _ => None,
        }
    }

    pub fn relocate(
        self,
        int_offset: usize,
        ext_offset: usize,
        func_offset: usize,
        globals_offset: usize,
    ) -> (Self, usize) {
        let mut max_func_offset = func_offset;
        let opcode = match self.opcode {
            Opcode::PushExternal(off) => Opcode::PushExternal(off + ext_offset),
            Opcode::Label(label) => {
                let (new_label, new_func_offset) =
                    label.relocate(int_offset, ext_offset, func_offset);
                if max_func_offset < new_func_offset {
                    max_func_offset = new_func_offset;
                }
                Opcode::Label(new_label)
            }
            Opcode::GetGlobalVar(idx) => Opcode::GetGlobalVar(idx + globals_offset),
            Opcode::SetGlobalVar(idx) => Opcode::SetGlobalVar(idx + globals_offset),
            _ => self.opcode,
        };
        let imm = match self.immediate {
            Some(imm) => {
                let (new_imm, new_func_offset) = imm.relocate(int_offset, ext_offset, func_offset);
                if max_func_offset < new_func_offset {
                    max_func_offset = new_func_offset;
                }
                Some(new_imm)
            }
            None => None,
        };
        (
            Instruction::new(opcode, imm, self.debug_info),
            max_func_offset,
        )
    }
}

impl<T> fmt::Display for Instruction<T>
where
    T: fmt::Display,
{
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match &self.immediate {
            Some(v) => write!(f, "[{}]\n        {}", v, self.opcode),
            None => write!(f, "{}", self.opcode),
        }
    }
}

#[derive(Debug, Clone, Copy, PartialEq, Eq, PartialOrd, Ord, Hash, Serialize, Deserialize)]
pub enum CodePt {
    Internal(usize),
    External(usize),         // slot in imported funcs list
    InSegment(usize, usize), // in code segment, at offset
    Null,                    // initial value of the Error Codepoint register
}

impl CodePt {
    pub fn new_internal(pc: usize) -> Self {
        CodePt::Internal(pc)
    }

    pub fn new_external(name: StringId) -> Self {
        CodePt::External(name)
    }

    pub fn new_in_segment(seg_num: usize, offset: usize) -> Self {
        CodePt::InSegment(seg_num, offset)
    }

    pub fn _upload(&self, u: &mut CodeUploader) {
        match self {
            CodePt::Internal(pc) => {
                u._push_byte(1);
                u._push_bytes(&Uint256::from_usize(u._translate_pc(*pc)).rlp_encode());
            }
            _ => {
                panic!();
            }
        }
    }

    pub fn incr(&self) -> Option<Self> {
        match self {
            CodePt::Internal(pc) => Some(CodePt::Internal(pc + 1)),
            CodePt::InSegment(seg, offset) => {
                if *offset == 0 {
                    None
                } else {
                    Some(CodePt::InSegment(*seg, offset - 1))
                }
            }
            CodePt::External(_) => None,
            CodePt::Null => None,
        }
    }

    pub fn relocate(self, int_offset: usize, ext_offset: usize) -> Self {
        match self {
            CodePt::Internal(pc) => CodePt::Internal(pc + int_offset),
            CodePt::External(off) => CodePt::External(off + ext_offset),
            CodePt::InSegment(_, _) => {
                panic!("tried to relocate/link code at runtime");
            }
            CodePt::Null => {
                panic!("tried to relocate/link null codepoint");
            }
        }
    }

    pub fn avm_hash(&self) -> Value {
        match self {
            CodePt::Internal(sz) => Value::avm_hash2(
                &Value::Int(Uint256::from_usize(3)),
                &Value::Int(Uint256::from_usize(*sz)),
            ),
            CodePt::External(_) => {
                Value::Int(Uint256::zero()) // never gets called when it matters
            }
            CodePt::InSegment(_, _) => {
                Value::Int(Uint256::zero()) // never gets called when it matters
            }
            CodePt::Null => Value::Int(Uint256::zero()),
        }
    }
}

impl fmt::Display for CodePt {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            CodePt::Internal(pc) => write!(f, "Internal({})", pc),
            CodePt::External(idx) => write!(f, "External({})", idx),
            CodePt::InSegment(seg, offset) => write!(f, "(segment {}, offset {})", seg, offset),
            CodePt::Null => write!(f, "Null"),
        }
    }
}

#[derive(Debug, Clone, PartialEq, Eq)]
pub struct Buffer {
    root: Arc<BufferNode>,
    size: u128,
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum BufferNode {
    Leaf(Vec<u8>),
    Internal(BufferInternal),
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub struct BufferInternal {
    height: usize,
    capacity: u128,
    left: Arc<BufferNode>,
    right: Arc<BufferNode>,
    hash_val: Uint256,
}

impl Buffer {
    pub fn new_empty() -> Self {
        Buffer {
            root: Arc::new(BufferNode::new_empty()),
            size: 0,
        }
    }

    pub fn from_bytes(contents: Vec<u8>) -> Self {
        let mut ret = Buffer::new_empty();
        for i in 0..contents.len() {
            ret = ret.set_byte(i as u128, contents[i]);
        }
        ret
    }

    pub fn as_bytes(&self, nbytes: usize) -> Vec<u8> {
        // TODO: make this more efficient
        let mut ret = vec![];
        for i in 0..nbytes {
            ret.push(self.read_byte(i as u128));
        }
        ret
    }

    fn avm_hash(&self) -> Uint256 {
        self.root.hash()
    }

    pub fn hex_encode(&self) -> String {
        self.root.hex_encode(self.size as usize)
    }

    pub fn read_byte(&self, offset: u128) -> u8 {
        if offset >= self.size {
            0u8
        } else {
            self.root.read_byte(offset)
        }
    }

    pub fn set_byte(&self, offset: u128, val: u8) -> Self {
        Buffer {
            root: Arc::new(self.root.set_byte(offset, val)),
            size: if offset >= self.size {
                offset + 1
            } else {
                self.size
            },
        }
    }
}

struct BufferVisitor;

impl<'de> Visitor<'de> for BufferVisitor {
    type Value = Buffer;
    fn expecting(&self, formatter: &mut fmt::Formatter) -> fmt::Result {
        formatter.write_str("Expected hex string")
    }
    fn visit_str<E>(self, v: &str) -> Result<Self::Value, E>
    where
        E: de::Error,
    {
        Ok(Buffer::from_bytes(hex::decode(v).map_err(|_| {
            E::custom("Could not buffer as hex string".to_string())
        })?))
    }
}

impl Serialize for Buffer {
    fn serialize<S>(&self, serializer: S) -> Result<<S as Serializer>::Ok, <S as Serializer>::Error>
    where
        S: Serializer,
    {
        serializer.serialize_str(&*format!(
            "{}",
            hex::encode(self.as_bytes(self.size as usize))
        ))
    }
}

impl<'de> Deserialize<'de> for Buffer {
    fn deserialize<D>(deserializer: D) -> Result<Self, <D as Deserializer<'de>>::Error>
    where
        D: Deserializer<'de>,
    {
        deserializer.deserialize_str(BufferVisitor)
    }
}

impl BufferNode {
    pub fn new_empty() -> Self {
        BufferNode::Leaf(vec![0u8; 32])
    }

    pub fn _leaf_from_bytes(v: &[u8]) -> Self {
        assert!(v.len() <= 32);
        let mut buf = vec![0u8; 32];
        for i in 0..v.len() {
            buf[i] = v[i]
        }
        BufferNode::Leaf(buf)
    }

    fn _minimal_internal_from_bytes(v: &[u8]) -> Self {
        assert!(v.len() > 32);
        let (height, size) = _levels_needed(v.len() as u128);
        BufferNode::_internal_from_bytes(height, size, v)
    }

    fn _internal_from_bytes(height: usize, capacity: u128, v: &[u8]) -> Self {
        if height == 1 {
            BufferNode::_leaf_from_bytes(v)
        } else if v.len() == 0 {
            BufferNode::new_empty_internal(height, capacity)
        } else if v.len() as u128 <= capacity / 2 {
            let left = Arc::new(BufferNode::_internal_from_bytes(
                height - 1,
                capacity / 2,
                v,
            ));
            let right = Arc::new(BufferNode::new_empty_internal(height - 1, capacity / 2));
            BufferNode::Internal(BufferInternal {
                height,
                capacity,
                left: left.clone(),
                right: right.clone(),
                hash_val: {
                    let mut b = left.hash().to_bytes_be();
                    b.extend(right.hash().to_bytes_be());
                    Uint256::from_bytes(&keccak256(&b))
                },
            })
        } else {
            let mid = (capacity / 2) as usize;
            let left = Arc::new(BufferNode::_internal_from_bytes(
                height - 1,
                capacity / 2,
                &v[0..mid],
            ));
            let right = Arc::new(BufferNode::_internal_from_bytes(
                height - 1,
                capacity / 2,
                &v[mid..],
            ));
            BufferNode::Internal(BufferInternal {
                height,
                capacity,
                left: left.clone(),
                right: right.clone(),
                hash_val: {
                    let mut b = left.hash().to_bytes_be();
                    b.extend(right.hash().to_bytes_be());
                    Uint256::from_bytes(&keccak256(&b))
                },
            })
        }
    }

    fn new_empty_internal(height: usize, capacity: u128) -> Self {
        let child = Arc::new(if height == 1 {
            BufferNode::new_empty()
        } else {
            BufferNode::new_empty_internal(height - 1, capacity / 2)
        });
        BufferNode::Internal(BufferInternal {
            height,
            capacity,
            left: child.clone(),
            right: child.clone(),
            hash_val: {
                let a = child.hash().to_bytes_be();
                let mut b = a.clone();
                b.extend(a);
                Uint256::from_bytes(&keccak256(&b))
            },
        })
    }

    fn hash(&self) -> Uint256 {
        match self {
            BufferNode::Leaf(b) => Uint256::from_bytes(&keccak256(&b[..])),
            BufferNode::Internal(x) => x.hash_val.clone(),
        }
    }

    fn hex_encode(&self, size: usize) -> String {
        match self {
            BufferNode::Leaf(b) => hex::encode(b)[0..(2 * size)].to_string(),
            BufferNode::Internal(node) => node.hex_encode(size),
        }
    }

    fn read_byte(&self, offset: u128) -> u8 {
        match self {
            BufferNode::Leaf(b) => b[offset as usize],
            BufferNode::Internal(node) => node.read_byte(offset),
        }
    }

    fn set_byte(&self, offset: u128, val: u8) -> Self {
        match self {
            BufferNode::Leaf(b) => {
                if (offset < 32) {
                    let mut bb = b.clone();
                    bb[offset as usize] = val;
                    BufferNode::Leaf(bb)
                } else {
                    BufferNode::Internal(
                        BufferInternal::grow_from_leaf(self.clone()).set_byte(offset, val),
                    )
                }
            }
            BufferNode::Internal(node) => BufferNode::Internal(node.set_byte(offset, val)),
        }
    }
}

impl BufferInternal {
    fn new(height: usize, capacity: u128, left: BufferNode, right: BufferNode) -> Self {
        BufferInternal {
            height,
            capacity,
            left: Arc::new(left.clone()),
            right: Arc::new(right.clone()),
            hash_val: {
                let mut b = left.hash().to_bytes_be();
                b.extend(right.hash().to_bytes_be());
                Uint256::from_bytes(&keccak256(&b))
            },
        }
    }

    fn grow(&self) -> Self {
        BufferInternal::new(
            self.height + 1,
            self.capacity * 2,
            BufferNode::Internal(self.clone()),
            BufferNode::new_empty_internal(self.height, self.capacity),
        )
    }

    fn grow_from_leaf(leaf: BufferNode) -> Self {
        BufferInternal::new(2, 2 * 32, leaf, BufferNode::new_empty())
    }

    fn hex_encode(&self, size: usize) -> String {
        let half_capacity = (self.capacity / 2) as usize;
        if size == 0 {
            "".to_string()
        } else if size <= half_capacity {
            self.left.hex_encode(size)
        } else {
            let mut left_str = self.left.hex_encode(half_capacity);
            let right_str = self.right.hex_encode(size - half_capacity);
            left_str.push_str(&right_str);
            left_str
        }
    }

    fn read_byte(&self, offset: u128) -> u8 {
        if offset < self.capacity / 2 {
            self.left.read_byte(offset)
        } else {
            self.right.read_byte(offset - self.capacity / 2)
        }
    }

    fn set_byte(&self, offset: u128, val: u8) -> BufferInternal {
        if offset < self.capacity / 2 {
            BufferInternal::new(
                self.height,
                self.capacity,
                self.left.set_byte(offset, val),
                (*self.right).clone(),
            )
        } else if offset < self.capacity {
            BufferInternal::new(
                self.height,
                self.capacity,
                (*self.left).clone(),
                self.right.set_byte(offset - self.capacity / 2, val),
            )
        } else {
            self.grow().set_byte(offset, val)
        }
    }
}

fn _levels_needed(x: u128) -> (usize, u128) {
    let mut height = 1;
    let mut size = 32u128;
    while (size < x) {
        height = height + 1;
        size = size * 2;
    }
    (height, size)
}

#[derive(Debug, Clone, PartialEq, Eq, Serialize, Deserialize)]
pub enum Value {
    Int(Uint256),
    Tuple(Arc<Vec<Value>>),
    CodePoint(CodePt),
    Label(Label),
    Buffer(Buffer),
}

impl Value {
    ///Returns a value containing no data, a zero sized tuple.
    pub fn none() -> Self {
        Value::Tuple(Arc::new(vec![]))
    }

    ///Creates a single tuple `Value` from a `Vec<Value>`
    pub fn new_tuple(v: Vec<Value>) -> Self {
        Value::Tuple(Arc::new(v))
    }

    pub fn new_buffer(v: Vec<u8>) -> Self {
        Value::Buffer(Buffer::from_bytes(v))
    }

    pub fn copy_buffer(v: Buffer) -> Self {
        Value::Buffer(v)
    }

    pub fn _upload(&self, u: &mut CodeUploader) {
        match self {
            Value::Int(ui) => {
                u._push_byte(0u8); // type code for uint
                u._push_bytes(&ui.rlp_encode());
            }
            Value::Tuple(tup) => {
                u._push_byte((10 + tup.len()) as u8);
                for subval in &**tup {
                    subval._upload(u);
                }
            }
            Value::CodePoint(cp) => {
                cp._upload(u);
            }
            Value::Buffer(buf) => {
                if buf.size == 0 {
                    u._push_byte(2u8);
                } else {
                    panic!();
                }
            }
            _ => {
                println!("unable to upload value: {}", self);
                panic!();
            } // other types should never be encountered here
        }
    }

    pub fn is_none(&self) -> bool {
        self == &Value::none()
    }

    pub fn type_insn_result(&self) -> usize {
        match self {
            Value::Int(_) => 0,
            Value::CodePoint(_) => 1,
            Value::Tuple(_) => 3,
            Value::Buffer(_) => 4,
            Value::Label(_) => {
                panic!("tried to run type instruction on a label");
            }
        }
    }

    pub fn replace_labels(self, label_map: &HashMap<Label, CodePt>) -> Result<Self, Label> {
        match self {
            Value::Int(_) => Ok(self),
            Value::CodePoint(_) => Ok(self),
            Value::Buffer(_) => Ok(self),
            Value::Label(label) => {
                let maybe_pc = label_map.get(&label);
                match maybe_pc {
                    Some(pc) => Ok(Value::CodePoint(*pc)),
                    None => Err(label),
                }
            }
            Value::Tuple(tup) => {
                let mut new_vec = Vec::new();
                for v in tup.iter() {
                    let val = v.clone();
                    new_vec.push(val.replace_labels(label_map)?);
                }
                Ok(Value::new_tuple(new_vec))
            }
        }
    }

    pub fn replace_last_none(&self, val: &Value) -> Self {
        if self.is_none() {
            return val.clone();
        }
        if let Value::Tuple(tup) = self {
            let tlen = tup.len();
            let mut mut_tup = tup.clone();
            let new_tup = Arc::<Vec<Value>>::make_mut(&mut mut_tup);
            new_tup[tlen - 1] = new_tup[tlen - 1].replace_last_none(val);
            Value::new_tuple(new_tup.to_vec())
        } else {
            panic!();
        }
    }

    pub fn relocate(
        self,
        int_offset: usize,
        ext_offset: usize,
        func_offset: usize,
    ) -> (Self, usize) {
        match self {
            Value::Int(_) => (self, 0),
            Value::Buffer(_) => (self, 0),
            Value::Tuple(v) => {
                let mut rel_v = Vec::new();
                let mut max_func_offset = 0;
                for val in &*v {
                    let (new_val, new_func_offset) =
                        val.clone().relocate(int_offset, ext_offset, func_offset);
                    rel_v.push(new_val);
                    if (max_func_offset < new_func_offset) {
                        max_func_offset = new_func_offset;
                    }
                }
                (Value::new_tuple(rel_v), max_func_offset)
            }
            Value::CodePoint(cpt) => (Value::CodePoint(cpt.relocate(int_offset, ext_offset)), 0),
            Value::Label(label) => {
                let (new_label, new_func_offset) =
                    label.relocate(int_offset, ext_offset, func_offset);
                (Value::Label(new_label), new_func_offset)
            }
        }
    }

    pub fn xlate_labels(self, label_map: &HashMap<Label, &Label>) -> Self {
        match self {
            Value::Int(_) | Value::CodePoint(_) | Value::Buffer(_) => self,
            Value::Tuple(v) => {
                let mut newv = Vec::new();
                for val in &*v {
                    newv.push(val.clone().xlate_labels(label_map));
                }
                Value::new_tuple(newv)
            }
            Value::Label(label) => match label_map.get(&label) {
                Some(label2) => Value::Label(**label2),
                None => self,
            },
        }
    }

    ///Converts `Value` to usize if possible, otherwise returns `None`.
    pub fn to_usize(&self) -> Option<usize> {
        match self {
            Value::Int(i) => i.to_usize(),
            _ => None,
        }
    }

    pub fn avm_hash(&self) -> Value {
        //BUGBUG: should do same hash as AVM
        match self {
            Value::Int(ui) => Value::Int(ui.avm_hash()),
            Value::Buffer(buf) => Value::Int(buf.avm_hash()),
            Value::Tuple(v) => {
                let mut acc = Uint256::zero();
                for val in v.to_vec() {
                    if let Value::Int(ui) = val.avm_hash() {
                        acc = Uint256::avm_hash2(&acc, &ui);
                    } else {
                        panic!("Invalid value type from hash");
                    }
                }
                Value::Int(acc)
            }
            Value::CodePoint(cp) => Value::avm_hash2(&Value::Int(Uint256::one()), &cp.avm_hash()),
            Value::Label(label) => {
                Value::avm_hash2(&Value::Int(Uint256::from_usize(2)), &label.avm_hash())
            }
        }
    }

    pub fn avm_hash2(v1: &Self, v2: &Self) -> Value {
        if let Value::Int(ui) = v1 {
            if let Value::Int(ui2) = v2 {
                let mut buf = ui.to_bytes_be();
                buf.extend(ui2.to_bytes_be());
                Value::Int(Uint256::from_bytes(&keccak256(&buf)))
            } else {
                panic!();
            }
        } else {
            panic!();
        }
    }
}

impl fmt::Display for Value {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Value::Int(i) => i.fmt(f),
            Value::Buffer(buf) => {
                write!(f, "Buffer({})", buf.hex_encode())
            }
            Value::CodePoint(pc) => write!(f, "CodePoint({})", pc),
            Value::Label(label) => write!(f, "Label({})", label),
            Value::Tuple(tup) => {
                if tup.is_empty() {
                    write!(f, "_")
                } else {
                    let mut s = "Tuple(".to_owned();
                    for (i, v) in tup.iter().enumerate() {
                        if i == 0 {
                            s = format!("{}{}", s, v);
                        } else {
                            s = format!("{}, {}", s, v);
                        }
                    }
                    write!(f, "{})", s)
                }
            }
        }
    }
}

#[derive(Debug, Clone, Copy, Serialize, Deserialize, Eq, PartialEq, Hash)]
pub enum Opcode {
    GetLocal,
    SetLocal,
    MakeFrame(usize, usize, bool),
    Label(Label),
    PushExternal(usize), // push codeptr of external function -- index in imported_funcs
    TupleGet(usize),     // arg is size of anysize_tuple
    TupleSet(usize),     // arg is size of anysize_tuple
    ArrayGet,
    UncheckedFixedArrayGet(usize), // arg is size of array
    GetGlobalVar(usize),
    SetGlobalVar(usize),
    Return,
    UnaryMinus,
    Len,
    AVMOpcode(AVMOpcode),
}

#[derive(Debug, Clone, Copy, Serialize_repr, Deserialize_repr, Eq, PartialEq, Hash)]
#[repr(u8)]
pub enum AVMOpcode {
    Zero = 0x00,
    Add = 0x01,
    Mul,
    Sub,
    Div,
    Sdiv,
    Mod,
    Smod,
    AddMod,
    MulMod,
    Exp,
    SignExtend,
    LessThan = 0x10,
    GreaterThan,
    SLessThan,
    SGreaterThan,
    Equal,
    IsZero,
    BitwiseAnd,
    BitwiseOr,
    BitwiseXor,
    BitwiseNeg,
    Byte,
    ShiftLeft,
    ShiftRight,
    ShiftArith,
    Hash = 0x20,
    Type,
    EthHash2,
    Keccakf,
    Sha256f,
    Ripemd160f,
    Blake2f,
    Pop = 0x30,
    Spush,
    Rpush,
    Rset,
    Jump,
    Cjump,
    StackEmpty,
    PCpush,
    AuxPush,
    AuxPop,
    AuxStackEmpty,
    Noop,
    ErrPush,
    ErrSet,
    Dup0 = 0x40,
    Dup1,
    Dup2,
    Swap1,
    Swap2,
    Tget = 0x50,
    Tset,
    Tlen,
    Xget,
    Xset,
    Breakpoint = 0x60,
    Log,
    Send = 0x70,
    InboxPeek,
    Inbox,
    Error,
    Halt,
    SetGas,
    PushGas,
    ErrCodePoint,
    PushInsn,
    PushInsnImm,
    OpenInsn,
    Sideload,
    EcRecover = 0x80,
    EcAdd,
    EcMul,
    EcPairing,
    DebugPrint = 0x90,
    NewBuffer = 0xa0,
    GetBuffer8,
    GetBuffer64,
    GetBuffer256,
    SetBuffer8,
    SetBuffer64,
    SetBuffer256,
}

impl Opcode {
    pub fn is_pure(&self) -> bool {
        match self {
            Opcode::AVMOpcode(AVMOpcode::Log)
            | Opcode::AVMOpcode(AVMOpcode::Inbox)
            | Opcode::AVMOpcode(AVMOpcode::InboxPeek)
            | Opcode::AVMOpcode(AVMOpcode::Send)
            | Opcode::AVMOpcode(AVMOpcode::Rset)
            | Opcode::AVMOpcode(AVMOpcode::Rpush)
            | Opcode::AVMOpcode(AVMOpcode::PushInsn)
            | Opcode::AVMOpcode(AVMOpcode::PushInsnImm)
            | Opcode::AVMOpcode(AVMOpcode::ErrCodePoint)
            | Opcode::AVMOpcode(AVMOpcode::ErrSet)
            | Opcode::AVMOpcode(AVMOpcode::ErrPush)
            | Opcode::AVMOpcode(AVMOpcode::SetGas)
            | Opcode::AVMOpcode(AVMOpcode::PushGas)
            | Opcode::AVMOpcode(AVMOpcode::Jump)
            | Opcode::AVMOpcode(AVMOpcode::Cjump)
            | Opcode::AVMOpcode(AVMOpcode::AuxPop)
            | Opcode::AVMOpcode(AVMOpcode::AuxPush)
            | Opcode::AVMOpcode(AVMOpcode::Sideload) => false,
            _ => true,
        }
    }
}

impl Opcode {
    pub fn from_name(name: &str) -> Self {
        match name {
            "rget" => Opcode::AVMOpcode(AVMOpcode::Rpush),
            "rset" => Opcode::AVMOpcode(AVMOpcode::Rset),
            "spush" => Opcode::AVMOpcode(AVMOpcode::Spush),
            "tset" => Opcode::AVMOpcode(AVMOpcode::Tset),
            "tget" => Opcode::AVMOpcode(AVMOpcode::Tget),
            "tlen" => Opcode::AVMOpcode(AVMOpcode::Tlen),
            "pop" => Opcode::AVMOpcode(AVMOpcode::Pop),
            "stackempty" => Opcode::AVMOpcode(AVMOpcode::StackEmpty),
            "auxpush" => Opcode::AVMOpcode(AVMOpcode::AuxPush),
            "auxpop" => Opcode::AVMOpcode(AVMOpcode::AuxPop),
            "auxstackempty" => Opcode::AVMOpcode(AVMOpcode::AuxStackEmpty),
            "xget" => Opcode::AVMOpcode(AVMOpcode::Xget),
            "xset" => Opcode::AVMOpcode(AVMOpcode::Xset),
            "dup0" => Opcode::AVMOpcode(AVMOpcode::Dup0),
            "dup1" => Opcode::AVMOpcode(AVMOpcode::Dup1),
            "dup2" => Opcode::AVMOpcode(AVMOpcode::Dup2),
            "swap1" => Opcode::AVMOpcode(AVMOpcode::Swap1),
            "swap2" => Opcode::AVMOpcode(AVMOpcode::Swap2),
            "unaryminus" => Opcode::UnaryMinus,
            "bitwiseneg" => Opcode::AVMOpcode(AVMOpcode::BitwiseNeg),
            "hash" => Opcode::AVMOpcode(AVMOpcode::Hash),
            "ethhash2" => Opcode::AVMOpcode(AVMOpcode::EthHash2),
            "keccakf" => Opcode::AVMOpcode(AVMOpcode::Keccakf),
            "sha256f" => Opcode::AVMOpcode(AVMOpcode::Sha256f),
            "ripemd160f" => Opcode::AVMOpcode(AVMOpcode::Ripemd160f),
            "blake2f" => Opcode::AVMOpcode(AVMOpcode::Blake2f),
            "length" => Opcode::AVMOpcode(AVMOpcode::Tlen),
            "add" => Opcode::AVMOpcode(AVMOpcode::Add),
            "sub" => Opcode::AVMOpcode(AVMOpcode::Sub),
            "mul" => Opcode::AVMOpcode(AVMOpcode::Mul),
            "div" => Opcode::AVMOpcode(AVMOpcode::Div),
            "mod" => Opcode::AVMOpcode(AVMOpcode::Mod),
            "sdiv" => Opcode::AVMOpcode(AVMOpcode::Sdiv),
            "smod" => Opcode::AVMOpcode(AVMOpcode::Smod),
            "exp" => Opcode::AVMOpcode(AVMOpcode::Exp),
            "lt" => Opcode::AVMOpcode(AVMOpcode::LessThan),
            "gt" => Opcode::AVMOpcode(AVMOpcode::GreaterThan),
            "slt" => Opcode::AVMOpcode(AVMOpcode::SLessThan),
            "sgt" => Opcode::AVMOpcode(AVMOpcode::SGreaterThan),
            "eq" => Opcode::AVMOpcode(AVMOpcode::Equal),
            "iszero" => Opcode::AVMOpcode(AVMOpcode::IsZero),
            "byte" => Opcode::AVMOpcode(AVMOpcode::Byte),
            "signextend" => Opcode::AVMOpcode(AVMOpcode::SignExtend),
            "shl" => Opcode::AVMOpcode(AVMOpcode::ShiftLeft),
            "shr" => Opcode::AVMOpcode(AVMOpcode::ShiftRight),
            "sar" => Opcode::AVMOpcode(AVMOpcode::ShiftArith),
            "bitwiseand" => Opcode::AVMOpcode(AVMOpcode::BitwiseAnd),
            "bitwiseor" => Opcode::AVMOpcode(AVMOpcode::BitwiseOr),
            "bitwisexor" => Opcode::AVMOpcode(AVMOpcode::BitwiseXor),
            "inbox" => Opcode::AVMOpcode(AVMOpcode::Inbox),
            "inboxpeek" => Opcode::AVMOpcode(AVMOpcode::InboxPeek),
            "jump" => Opcode::AVMOpcode(AVMOpcode::Jump),
            "log" => Opcode::AVMOpcode(AVMOpcode::Log),
            "send" => Opcode::AVMOpcode(AVMOpcode::Send),
            "errcodept" => Opcode::AVMOpcode(AVMOpcode::ErrCodePoint),
            "pushinsn" => Opcode::AVMOpcode(AVMOpcode::PushInsn),
            "pushinsnimm" => Opcode::AVMOpcode(AVMOpcode::PushInsnImm),
            "openinsn" => Opcode::AVMOpcode(AVMOpcode::OpenInsn),
            "debugprint" => Opcode::AVMOpcode(AVMOpcode::DebugPrint),
            "setgas" => Opcode::AVMOpcode(AVMOpcode::SetGas),
            "pushgas" => Opcode::AVMOpcode(AVMOpcode::PushGas),
            "errset" => Opcode::AVMOpcode(AVMOpcode::ErrSet),
            "sideload" => Opcode::AVMOpcode(AVMOpcode::Sideload),
            "ecrecover" => Opcode::AVMOpcode(AVMOpcode::EcRecover),
            "ecadd" => Opcode::AVMOpcode(AVMOpcode::EcAdd),
            "ecmul" => Opcode::AVMOpcode(AVMOpcode::EcMul),
            "ecpairing" => Opcode::AVMOpcode(AVMOpcode::EcPairing),
            "addmod" => Opcode::AVMOpcode(AVMOpcode::AddMod),
            "mulmod" => Opcode::AVMOpcode(AVMOpcode::MulMod),
            _ => {
                panic!("opcode not supported in asm segment: {}", name);
            }
        }
    }

    pub fn to_name(&self) -> &str {
        match self {
            Opcode::AVMOpcode(avm) => avm.to_name(),
            Opcode::UnaryMinus => "unaryminus",
            _ => "Unknown",
        }
    }
}

impl From<AVMOpcode> for Opcode {
    fn from(from: AVMOpcode) -> Self {
        Opcode::AVMOpcode(from)
    }
}

impl AVMOpcode {
    fn to_name(&self) -> &str {
        match self {
            AVMOpcode::Rpush => "rpush",
            AVMOpcode::Rset => "rset",
            AVMOpcode::Spush => "spush",
            AVMOpcode::Tset => "tset",
            AVMOpcode::Tget => "tget",
            AVMOpcode::Pop => "pop",
            AVMOpcode::StackEmpty => "stackempty",
            AVMOpcode::AuxPush => "auxpush",
            AVMOpcode::AuxPop => "auxpop",
            AVMOpcode::AuxStackEmpty => "auxstackempty",
            AVMOpcode::Xget => "xget",
            AVMOpcode::Xset => "xset",
            AVMOpcode::Dup0 => "dup0",
            AVMOpcode::Dup1 => "dup1",
            AVMOpcode::Dup2 => "dup2",
            AVMOpcode::Swap1 => "swap1",
            AVMOpcode::Swap2 => "swap2",
            AVMOpcode::BitwiseNeg => "bitwiseneg",
            AVMOpcode::Hash => "hash",
            AVMOpcode::EthHash2 => "ethhash2",
            AVMOpcode::Type => "type",
            AVMOpcode::Keccakf => "keccakf",
            AVMOpcode::Sha256f => "sha256f",
            AVMOpcode::Ripemd160f => "ripemd160f",
            AVMOpcode::Blake2f => "blake2f",
            AVMOpcode::Tlen => "length",
            AVMOpcode::Add => "add",
            AVMOpcode::Sub => "sub",
            AVMOpcode::Mul => "mul",
            AVMOpcode::Div => "div",
            AVMOpcode::Mod => "mod",
            AVMOpcode::Sdiv => "sdiv",
            AVMOpcode::Smod => "smod",
            AVMOpcode::AddMod => "addmod",
            AVMOpcode::MulMod => "mulmod",
            AVMOpcode::Exp => "exp",
            AVMOpcode::LessThan => "lt",
            AVMOpcode::GreaterThan => "gt",
            AVMOpcode::SLessThan => "slt",
            AVMOpcode::SGreaterThan => "sgt",
            AVMOpcode::Equal => "eq",
            AVMOpcode::IsZero => "iszero",
            AVMOpcode::Byte => "byte",
            AVMOpcode::SignExtend => "signextend",
            AVMOpcode::ShiftLeft => "shl",
            AVMOpcode::ShiftRight => "shr",
            AVMOpcode::ShiftArith => "sar",
            AVMOpcode::BitwiseAnd => "bitwiseand",
            AVMOpcode::BitwiseOr => "bitwiseor",
            AVMOpcode::BitwiseXor => "bitwisexor",
            AVMOpcode::Noop => "noop",
            AVMOpcode::ErrPush => "errpush",
            AVMOpcode::Inbox => "inbox",
            AVMOpcode::Error => "error",
            AVMOpcode::Zero => "zero",
            AVMOpcode::Halt => "halt",
            AVMOpcode::InboxPeek => "inboxpeek",
            AVMOpcode::Jump => "jump",
            AVMOpcode::Cjump => "cjump",
            AVMOpcode::PCpush => "pcpush",
            AVMOpcode::Breakpoint => "breakpoint",
            AVMOpcode::Log => "log",
            AVMOpcode::Send => "send",
            AVMOpcode::ErrCodePoint => "errcodept",
            AVMOpcode::PushInsn => "pushinsn",
            AVMOpcode::PushInsnImm => "pushinsnimm",
            AVMOpcode::OpenInsn => "openinsn",
            AVMOpcode::DebugPrint => "debugprint",
            AVMOpcode::SetGas => "setgas",
            AVMOpcode::PushGas => "pushgas",
            AVMOpcode::ErrSet => "errset",
            AVMOpcode::Sideload => "sideload",
            AVMOpcode::EcRecover => "ecrecover",
            AVMOpcode::EcAdd => "ecadd",
            AVMOpcode::EcMul => "ecmul",
            AVMOpcode::EcPairing => "ecpairing",
            AVMOpcode::NewBuffer => "newbuffer",
            AVMOpcode::GetBuffer8 => "getbuffer8",
            AVMOpcode::GetBuffer64 => "getbuffer64",
            AVMOpcode::GetBuffer256 => "getbuffer256",
            AVMOpcode::SetBuffer8 => "setbuffer8",
            AVMOpcode::SetBuffer64 => "setbuffer64",
            AVMOpcode::SetBuffer256 => "setbuffer256",
        }
    }

    pub fn from_number(num: usize) -> Option<Self> {
        match num {
            0x00 => Some(AVMOpcode::Zero),
            0x01 => Some(AVMOpcode::Add),
            0x02 => Some(AVMOpcode::Mul),
            0x03 => Some(AVMOpcode::Sub),
            0x04 => Some(AVMOpcode::Div),
            0x05 => Some(AVMOpcode::Sdiv),
            0x06 => Some(AVMOpcode::Mod),
            0x07 => Some(AVMOpcode::Smod),
            0x08 => Some(AVMOpcode::AddMod),
            0x09 => Some(AVMOpcode::MulMod),
            0x0a => Some(AVMOpcode::Exp),
            0x0b => Some(AVMOpcode::SignExtend),
            0x10 => Some(AVMOpcode::LessThan),
            0x11 => Some(AVMOpcode::GreaterThan),
            0x12 => Some(AVMOpcode::SLessThan),
            0x13 => Some(AVMOpcode::SGreaterThan),
            0x14 => Some(AVMOpcode::Equal),
            0x15 => Some(AVMOpcode::IsZero),
            0x16 => Some(AVMOpcode::BitwiseAnd),
            0x17 => Some(AVMOpcode::BitwiseOr),
            0x18 => Some(AVMOpcode::BitwiseXor),
            0x19 => Some(AVMOpcode::BitwiseNeg),
            0x1a => Some(AVMOpcode::Byte),
            0x1b => Some(AVMOpcode::ShiftLeft),
            0x1c => Some(AVMOpcode::ShiftRight),
            0x1d => Some(AVMOpcode::ShiftArith),
            0x20 => Some(AVMOpcode::Hash),
            0x21 => Some(AVMOpcode::Type),
            0x22 => Some(AVMOpcode::EthHash2),
            0x23 => Some(AVMOpcode::Keccakf),
            0x24 => Some(AVMOpcode::Sha256f),
            0x25 => Some(AVMOpcode::Ripemd160f),
            0x26 => Some(AVMOpcode::Blake2f),
            0x30 => Some(AVMOpcode::Pop),
            0x31 => Some(AVMOpcode::Spush),
            0x32 => Some(AVMOpcode::Rpush),
            0x33 => Some(AVMOpcode::Rset),
            0x34 => Some(AVMOpcode::Jump),
            0x35 => Some(AVMOpcode::Cjump),
            0x36 => Some(AVMOpcode::StackEmpty),
            0x37 => Some(AVMOpcode::PCpush),
            0x38 => Some(AVMOpcode::AuxPush),
            0x39 => Some(AVMOpcode::AuxPop),
            0x3a => Some(AVMOpcode::AuxStackEmpty),
            0x3b => Some(AVMOpcode::Noop),
            0x3c => Some(AVMOpcode::ErrPush),
            0x3d => Some(AVMOpcode::ErrSet),
            0x40 => Some(AVMOpcode::Dup0),
            0x41 => Some(AVMOpcode::Dup1),
            0x42 => Some(AVMOpcode::Dup2),
            0x43 => Some(AVMOpcode::Swap1),
            0x44 => Some(AVMOpcode::Swap2),
            0x50 => Some(AVMOpcode::Tget),
            0x51 => Some(AVMOpcode::Tset),
            0x52 => Some(AVMOpcode::Tlen),
            0x53 => Some(AVMOpcode::Xget),
            0x54 => Some(AVMOpcode::Xset),
            0x60 => Some(AVMOpcode::Breakpoint),
            0x61 => Some(AVMOpcode::Log),
            0x70 => Some(AVMOpcode::Send),
            0x71 => Some(AVMOpcode::InboxPeek),
            0x72 => Some(AVMOpcode::Inbox),
            0x73 => Some(AVMOpcode::Error),
            0x74 => Some(AVMOpcode::Halt),
            0x75 => Some(AVMOpcode::SetGas),
            0x76 => Some(AVMOpcode::PushGas),
            0x77 => Some(AVMOpcode::ErrCodePoint),
            0x78 => Some(AVMOpcode::PushInsn),
            0x79 => Some(AVMOpcode::PushInsnImm),
            0x7a => Some(AVMOpcode::OpenInsn),
            0x7b => Some(AVMOpcode::Sideload),
            0x80 => Some(AVMOpcode::EcRecover),
            0x81 => Some(AVMOpcode::EcAdd),
            0x82 => Some(AVMOpcode::EcMul),
            0x83 => Some(AVMOpcode::EcPairing),
            0x90 => Some(AVMOpcode::DebugPrint),
            0xa0 => Some(AVMOpcode::NewBuffer),
            0xa1 => Some(AVMOpcode::GetBuffer8),
            0xa2 => Some(AVMOpcode::GetBuffer64),
            0xa3 => Some(AVMOpcode::GetBuffer256),
            0xa4 => Some(AVMOpcode::SetBuffer8),
            0xa5 => Some(AVMOpcode::SetBuffer64),
            0xa6 => Some(AVMOpcode::SetBuffer256),
            _ => None,
        }
    }

    pub fn to_number(&self) -> u8 {
        match self {
            AVMOpcode::Zero => 0,
            AVMOpcode::Add => 0x01,
            AVMOpcode::Mul => 0x02,
            AVMOpcode::Sub => 0x03,
            AVMOpcode::Div => 0x04,
            AVMOpcode::Sdiv => 0x05,
            AVMOpcode::Mod => 0x06,
            AVMOpcode::Smod => 0x07,
            AVMOpcode::AddMod => 0x08,
            AVMOpcode::MulMod => 0x09,
            AVMOpcode::Exp => 0x0a,
            AVMOpcode::SignExtend => 0x0b,
            AVMOpcode::LessThan => 0x10,
            AVMOpcode::GreaterThan => 0x11,
            AVMOpcode::SLessThan => 0x12,
            AVMOpcode::SGreaterThan => 0x13,
            AVMOpcode::Equal => 0x14,
            AVMOpcode::IsZero => 0x15,
            AVMOpcode::BitwiseAnd => 0x16,
            AVMOpcode::BitwiseOr => 0x17,
            AVMOpcode::BitwiseXor => 0x18,
            AVMOpcode::BitwiseNeg => 0x19,
            AVMOpcode::Byte => 0x1a,
            AVMOpcode::ShiftLeft => 0x1b,
            AVMOpcode::ShiftRight => 0x1c,
            AVMOpcode::ShiftArith => 0x1d,
            AVMOpcode::Hash => 0x20,
            AVMOpcode::Type => 0x21,
            AVMOpcode::EthHash2 => 0x22,
            AVMOpcode::Keccakf => 0x23,
            AVMOpcode::Sha256f => 0x24,
            AVMOpcode::Ripemd160f => 0x25,
            AVMOpcode::Blake2f => 0x26,
            AVMOpcode::Pop => 0x30,
            AVMOpcode::Spush => 0x31,
            AVMOpcode::Rpush => 0x32,
            AVMOpcode::Rset => 0x33,
            AVMOpcode::Jump => 0x34,
            AVMOpcode::Cjump => 0x35,
            AVMOpcode::StackEmpty => 0x36,
            AVMOpcode::PCpush => 0x37,
            AVMOpcode::AuxPush => 0x38,
            AVMOpcode::AuxPop => 0x39,
            AVMOpcode::AuxStackEmpty => 0x3a,
            AVMOpcode::Noop => 0x3b,
            AVMOpcode::ErrPush => 0x3c,
            AVMOpcode::ErrSet => 0x3d,
            AVMOpcode::Dup0 => 0x40,
            AVMOpcode::Dup1 => 0x41,
            AVMOpcode::Dup2 => 0x42,
            AVMOpcode::Swap1 => 0x43,
            AVMOpcode::Swap2 => 0x44,
            AVMOpcode::Tget => 0x50,
            AVMOpcode::Tset => 0x51,
            AVMOpcode::Tlen => 0x52,
            AVMOpcode::Xget => 0x53,
            AVMOpcode::Xset => 0x54,
            AVMOpcode::Breakpoint => 0x60,
            AVMOpcode::Log => 0x61,
            AVMOpcode::Send => 0x70,
            AVMOpcode::InboxPeek => 0x71,
            AVMOpcode::Inbox => 0x72,
            AVMOpcode::Error => 0x73,
            AVMOpcode::Halt => 0x74,
            AVMOpcode::SetGas => 0x75,
            AVMOpcode::PushGas => 0x76,
            AVMOpcode::ErrCodePoint => 0x77,
            AVMOpcode::PushInsn => 0x78,
            AVMOpcode::PushInsnImm => 0x79,
            AVMOpcode::OpenInsn => 0x7a,
            AVMOpcode::Sideload => 0x7b,
            AVMOpcode::EcRecover => 0x80,
            AVMOpcode::EcAdd => 0x81,
            AVMOpcode::EcMul => 0x82,
            AVMOpcode::EcPairing => 0x83,
            AVMOpcode::DebugPrint => 0x90,
            AVMOpcode::NewBuffer => 0xa0,
            AVMOpcode::GetBuffer8 => 0xa1,
            AVMOpcode::GetBuffer64 => 0xa2,
            AVMOpcode::GetBuffer256 => 0xa3,
            AVMOpcode::SetBuffer8 => 0xa4,
            AVMOpcode::SetBuffer64 => 0xa5,
            AVMOpcode::SetBuffer256 => 0xa6,
        }
    }
}

#[test]
fn test_consistent_opcode_numbers() {
    for i in 0..256 {
        if let Some(op) = AVMOpcode::from_number(i) {
            assert_eq!(i as u8, op.to_number());
        }
    }
}

impl fmt::Display for Opcode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        match self {
            Opcode::MakeFrame(s1, s2, ret) => write!(f, "MakeFrame({}, {}, {})", s1, s2, ret),
            Opcode::Label(label) => label.fmt(f),
            _ => write!(f, "{}", self.to_name()),
        }
    }
}

impl fmt::Display for AVMOpcode {
    fn fmt(&self, f: &mut fmt::Formatter) -> fmt::Result {
        write!(f, "{}", self.to_name())
    }
}
