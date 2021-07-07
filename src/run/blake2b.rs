// Based on https://github.com/ebfe/rust-blake2/blob/master/src/blake2b.rs (BSD 2-clause simplified license)
// Modifications are Copyright 2021, Offchain Labs, Inc.  All rights reserved.

use crate::mavm::Buffer;
use crate::run::load_from_file;
use crate::uint256::Uint256;
use std::convert::TryInto;
use std::path::Path;

pub const BLOCK_BYTES: usize = 128;

static IV: [u64; 8] = [
    0x6a09e667f3bcc908,
    0xbb67ae8584caa73b,
    0x3c6ef372fe94f82b,
    0xa54ff53a5f1d36f1,
    0x510e527fade682d1,
    0x9b05688c2b3e6c1f,
    0x1f83d9abfb41bd6b,
    0x5be0cd19137e2179,
];

static SIGMA: [[u8; 16]; 10] = [
    [0, 1, 2, 3, 4, 5, 6, 7, 8, 9, 10, 11, 12, 13, 14, 15],
    [14, 10, 4, 8, 9, 15, 13, 6, 1, 12, 0, 2, 11, 7, 5, 3],
    [11, 8, 12, 0, 5, 2, 15, 13, 10, 14, 3, 6, 7, 1, 9, 4],
    [7, 9, 3, 1, 13, 12, 11, 14, 2, 6, 5, 10, 4, 0, 15, 8],
    [9, 0, 5, 7, 2, 4, 10, 15, 14, 1, 11, 12, 6, 8, 3, 13],
    [2, 12, 6, 10, 0, 11, 8, 3, 4, 13, 7, 5, 15, 14, 1, 9],
    [12, 5, 1, 15, 14, 13, 4, 10, 0, 7, 6, 3, 9, 2, 8, 11],
    [13, 11, 7, 14, 12, 1, 3, 9, 5, 0, 15, 4, 8, 6, 2, 10],
    [6, 15, 14, 9, 11, 3, 0, 8, 12, 2, 13, 7, 1, 4, 10, 5],
    [10, 2, 8, 4, 7, 6, 1, 5, 15, 11, 9, 14, 3, 12, 13, 0],
];

pub struct Blake2b {
    h: [u64; 8],
    t: [u64; 2],
    f: [u64; 2],
    buf: [u8; 2 * BLOCK_BYTES],
}

impl Copy for Blake2b {}
impl Clone for Blake2b {
    fn clone(&self) -> Blake2b {
        *self
    }
}

pub fn blake2bf_instruction(buf: Buffer) -> Buffer {
    let mut b = [0u8; 213];
    for i in 0..b.len() {
        b[i] = buf.read_byte(i as u128);
    }
    b[212] = b[212] & 0x1u8;
    match eth_blake2bf_from_bytes(b) {
        Some(result) => {
            let mut ret = Buffer::new_empty();
            for i in 0..64 {
                ret = ret.set_byte(i, result[i as usize]);
            }
            ret
        }
        None => {
            panic!();
        }
    }
}

pub fn eth_blake2bf_from_bytes(b: [u8; 213]) -> Option<[u8; 64]> {
    let mut num_rounds = u32::from_be_bytes(b[0..4].try_into().unwrap());
    if num_rounds > 0xffff {
        num_rounds = 0xffff;
    }
    let mut offset = 4;
    let mut h = [0u64; 8];
    for i in 0..8 {
        h[i] = u64::from_le_bytes(b[offset..offset + 8].try_into().unwrap());
        offset = offset + 8;
    }
    let mut m = [0u64; 16];
    for i in 0..16 {
        m[i] = u64::from_le_bytes(b[offset..offset + 8].try_into().unwrap());
        offset = offset + 8;
    }
    let mut t = [0u64; 2];
    for i in 0..2 {
        t[i] = u64::from_le_bytes(b[offset..offset + 8].try_into().unwrap());
        offset = offset + 8;
    }
    let f = b[offset];
    offset = offset + 1;

    assert_eq!(offset, b.len());

    let chunks = eth_blake2bf(num_rounds, h, m, t, f)?;
    let mut ret = vec![];
    for i in 0..8 {
        ret.extend_from_slice(&chunks[i].to_le_bytes());
    }

    Some(ret[0..64].try_into().unwrap())
}

pub fn eth_blake2bf(
    num_rounds: u32,
    h: [u64; 8],
    m: [u64; 16],
    t: [u64; 2],
    f: u8,
) -> Option<[u64; 8]> {
    let f = match f {
        0 => [0u64, 0u64],
        1 => [0xffff_ffff_ffff_ffffu64, 0u64],
        _ => {
            return None;
        }
    };

    let mut buf = [0; 2 * BLOCK_BYTES];
    for i in 0..m.len() {
        let b = m[i].to_le_bytes();
        for j in 0..8 {
            buf[8 * i + j] = b[j];
        }
    }

    let mut blaker = Blake2b { h, t, f, buf: buf };
    blaker.compress(num_rounds as usize, m);
    Some(blaker.h)
}

impl Blake2b {
    fn compress(&mut self, num_rounds: usize, orig_m: [u64; 16]) {
        let mut m = [0u64; 16];
        let mut v = [0u64; 16];
        let block = self.buf.as_ref();

        assert!(block.len() >= BLOCK_BYTES);

        for i in 0..m.len() {
            m[i] = load64(&block[i * 8..]);
            assert_eq!(m[i], orig_m[i]);
        }

        for i in 0..8 {
            v[i] = self.h[i];
        }

        v[8] = IV[0];
        v[9] = IV[1];
        v[10] = IV[2];
        v[11] = IV[3];
        v[12] = self.t[0] ^ IV[4];
        v[13] = self.t[1] ^ IV[5];
        v[14] = self.f[0] ^ IV[6];
        v[15] = self.f[1] ^ IV[7];

        macro_rules! g(
            ($r: expr, $i: expr, $a: expr, $b: expr, $c: expr, $d: expr) => ({
                $a = $a.wrapping_add($b).wrapping_add(m[SIGMA[$r%10][2*$i+0] as usize]);
                $d = ($d ^ $a).rotate_right(32);
                $c = $c.wrapping_add($d);
                $b = ($b ^ $c).rotate_right(24);
                $a = $a.wrapping_add($b).wrapping_add(m[SIGMA[$r%10][2*$i+1] as usize]);
                $d = ($d ^ $a).rotate_right(16);
                $c = $c.wrapping_add($d);
                $b = ($b ^ $c).rotate_right(63);
            });
        );

        macro_rules! round(
            ($r: expr) => ({
                g!($r, 0, v[ 0], v[ 4], v[ 8], v[12]);
                g!($r, 1, v[ 1], v[ 5], v[ 9], v[13]);
                g!($r, 2, v[ 2], v[ 6], v[10], v[14]);
                g!($r, 3, v[ 3], v[ 7], v[11], v[15]);
                g!($r, 4, v[ 0], v[ 5], v[10], v[15]);
                g!($r, 5, v[ 1], v[ 6], v[11], v[12]);
                g!($r, 6, v[ 2], v[ 7], v[ 8], v[13]);
                g!($r, 7, v[ 3], v[ 4], v[ 9], v[14]);
            });
        );

        for i in 0..num_rounds {
            round!(i);
        }

        for i in 0..8 {
            self.h[i] = self.h[i] ^ v[i] ^ v[i + 8];
        }
    }
}

fn load64(b: &[u8]) -> u64 {
    let mut v = 0u64;
    for i in 0..8 {
        v |= (b[i] as u64) << (8 * i);
    }
    v
}

#[test]
fn test_eth_blake() {
    _test_eth_blake_impl()
}

fn _test_eth_blake_impl() {
    let test_vectors = [
        (
            "",
            None,
        ),
        (
            "00000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            None,
        ),
        (
            "000000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            None,
        ),
        (
            "0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000002",
            None,
        ),
        (
            "0000000048c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            Some("08c9bcf367e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d282e6ad7f520e511f6c3e2b8c68059b9442be0454267ce079217e1319cde05b"),
        ),
        (
            "0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            Some("ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"),
        ),
        (
            "0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000",
            Some("75ab69d3190a562c51aef8d88f1c2775876944407270c42c9844252c26d2875298743e7f6d5ea2f2d3e8d226039cd31b4e426ac4f2d3d666a610c2116fde4735"),
        ),
        (
            "0000000148c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            Some("b63a380cb2897d521994a85234ee2c181b5f844d2c624c002677e9703449d2fba551b3a8333bcdf5f2f7e08993d53923de3d64fcc68c034e717b9293fed7a421"),
        ),
        //(  // skip this test because it uses more than 100k rounds, and we cap num_rounds at 100k
        //    "ffffffff48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
        //    Some("fc59093aafa9ab43daae0e914c57635c5402d8e3d2130eb9b3cc181de7f0ecf9b22bf99a7815ce16419e200e01846e6b5df8cc7703041bbceb571de6631d2615"),
        //),
    ];

    for (x, y) in &test_vectors {
        println!("Trying a test vector");
        let xbytes = hex::decode(x).unwrap();
        if xbytes.len() == 213 {
            match eth_blake2bf_from_bytes(xbytes.try_into().unwrap()) {
                None => assert_eq!(y, &None),
                Some(ybytes) => assert_eq!(ybytes, hex::decode(y.unwrap()).unwrap()[0..64]),
            }
        } else {
            assert_eq!(y, &None);
        }
    }
}

#[test]
fn _test_eth_blake_precompile() {
    _test_eth_blake_precompile_impl();
}

fn _test_eth_blake_precompile_impl() {
    let test_vectors = [
        (
            "",
            None,
        ),
        (
            "00000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            None,
        ),
        (
            "000000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            None,
        ),
        (
            "0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000002",
            None,
        ),
        (
            "0000000048c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            Some("08c9bcf367e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d282e6ad7f520e511f6c3e2b8c68059b9442be0454267ce079217e1319cde05b"),
        ),
        (
            "0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            Some("ba80a53f981c4d0d6a2797b69f12f6e94c212f14685ac4b74b12bb6fdbffa2d17d87c5392aab792dc252d5de4533cc9518d38aa8dbf1925ab92386edd4009923"),
        ),
        (
            "0000000c48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000000",
            Some("75ab69d3190a562c51aef8d88f1c2775876944407270c42c9844252c26d2875298743e7f6d5ea2f2d3e8d226039cd31b4e426ac4f2d3d666a610c2116fde4735"),
        ),
        (
            "0000000148c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            Some("b63a380cb2897d521994a85234ee2c181b5f844d2c624c002677e9703449d2fba551b3a8333bcdf5f2f7e08993d53923de3d64fcc68c034e717b9293fed7a421"),
        ),
        (  // should fail because this exceeds the max number of rounds
            "ffffffff48c9bdf267e6096a3ba7ca8485ae67bb2bf894fe72f36e3cf1361d5f3af54fa5d182e6ad7f520e511f6c3e2b8c68059b6bbd41fbabd9831f79217e1319cde05b61626300000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000000300000000000000000000000000000001",
            None,
        ),
    ];

    let mut machine = load_from_file(Path::new("arb_os/arbos.mexe"));
    machine.start_at_zero();

    let my_addr = Uint256::from_u64(1025);

    for (x, y) in &test_vectors {
        let xbytes = hex::decode(x).unwrap();

        let tx_id = machine.runtime_env.insert_tx_message(
            my_addr.clone(),
            Uint256::from_u64(1000000000),
            None,
            Uint256::from_u64(9), // blake2f precompile
            Uint256::from_u64(0),
            &xbytes,
            false,
        );
        let _ = machine.run(None);

        let receipts = machine.runtime_env.get_all_receipt_logs();
        let last_rcpt_num = receipts.len() - 1;
        assert_eq!(receipts[last_rcpt_num].get_request_id(), tx_id);
        let yret = if receipts[last_rcpt_num].succeeded() {
            Some(receipts[last_rcpt_num].get_return_data())
        } else {
            None
        };

        assert_eq!(y.map(|s| hex::decode(s).unwrap()), yret);
    }
}
