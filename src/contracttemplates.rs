/*
* Copyright 2020, Offchain Labs, Inc. All rights reserved.
*/

use crate::mavm::Value;
use crate::run::_bytestack_from_bytes;
use bytes::{BufMut, BytesMut};
use std::{fs::File, io::Write, path::Path};

pub fn generate_contract_template_file_or_die(path: &Path) {
    let display = path.display();
    let mut file = match File::create(&path) {
        Err(why) => panic!("couldn't create {}: {}", display, why),
        Ok(file) => file,
    };

    if let Err(why) = file.write_all(&mini_code_for_templates().freeze()[..]) {
        panic!("couldn't write to {}: {}", display, why)
    }
}

fn mini_code_for_avm_value(buf: &mut BytesMut, val: Value) {
    match val {
        Value::Int(ui) => {
            buf.extend(ui.to_string().as_bytes());
        }
        Value::Tuple(tup) => {
            buf.put_u8(b'(');
            for t in &*tup {
                mini_code_for_avm_value(buf, t.clone());
                buf.put_u8(b',');
            }
            buf.put_u8(b')');
        }
        _ => {
            panic!("Can't generate code for this AVM value: {}", val);
        }
    }
}

fn mini_code_for_bytes(buf: &mut BytesMut, b: &[u8]) {
    mini_code_for_avm_value(buf, _bytestack_from_bytes(b));
}

fn mini_code_getter_for_bytes(buf: &mut BytesMut, name: &str, b: &[u8]) {
    buf.put(&b"public func "[..]);
    buf.put(name.as_bytes());
    buf.put(&b"() -> MarshalledBytes {\n  return unsafecast<MarshalledBytes>("[..]);
    mini_code_for_bytes(buf, b);
    buf.put(&b");\n}\n\n"[..]);
}

fn mini_code_for_templates() -> BytesMut {
    let mut buf = BytesMut::with_capacity(1024);
    buf.put(&b"// DO NOT EDIT -- this is machine-generated code.\n\n"[..]);
    buf.put(&b"use std::bytearray::MarshalledBytes;\nuse std::storageMap::StorageMap;\n\n"[..]);
    buf.put(&b"use std::storageMap::storageMap_new;\n\n"[..]);

    mini_code_for_arbinfo(&mut buf);
    buf
}

fn mini_code_for_arbinfo(buf: &mut BytesMut) {
    mini_code_getter_for_bytes(
        buf,
        "getArbInfoCode",
        &get_deployed_bytecode(Path::new(
            "contracts/artifacts/arbos/builtin/ArbInfo.sol/ArbInfo.json",
        )),
    );

    buf.put(
        &b"public func getArbInfoStorage() -> StorageMap {\n return storageMap_new();\n}\n\n"[..],
    );

    buf.put(&b"public func getArbInfoAddress() -> address {\n  return address(0x65);\n}\n\n"[..])
}

fn get_deployed_bytecode(path: &Path) -> Vec<u8> {
    let contents =
        std::fs::read_to_string(path.clone()).expect("Something went wrong reading the file");
    let json: serde_json::Value =
        serde_json::from_str(&contents).expect("JSON was not well-formatted");
    let code_str = json["deployedBytecode"].to_string();
    hex::decode(&code_str[3..(code_str.len() - 1)]).unwrap()
}
