use super::*;
use blake2b_ref::Blake2bBuilder;

const BINARIES: &[(&str, &str)] = &[
    (
        "poa.strip",
        "db4a47bbbc4bc9d6686f8af113ce7e7133be670c7fe048c7423f9c2bd7b539ac",
    ),
    (
        "state.strip",
        "ef0c04978e5e3e77a0c569a63de13992aa7d198134879329aadf60315b535459",
    ),
];

pub fn ckb_hash(data: &[u8]) -> Bytes {
    let mut blake2b = Blake2bBuilder::new(32)
        .personal(b"ckb-default-hash")
        .build();
    blake2b.update(data);
    let mut hash = vec![0u8; 32];
    blake2b.finalize(&mut hash[..]);
    Bytes::from(hash)
}

#[test]
fn test_code_hashes() {
    for (name, expected_hash) in BINARIES {
        let bin = Loader::default().load_binary(name);
        let actual_hash = format!("{:x}", ckb_hash(&bin));

        if expected_hash != &actual_hash {
            panic!(
                "Invalid hash {} for {}, expected: {}",
                actual_hash, name, expected_hash
            );
        }
    }
}
