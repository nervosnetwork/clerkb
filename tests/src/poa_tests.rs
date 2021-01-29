use super::*;
use ckb_testtool::{builtin::ALWAYS_SUCCESS, context::Context};
use ckb_tool::ckb_types::{
    bytes::{Bytes, BytesMut},
    core::{ScriptHashType, TransactionBuilder},
    h256,
    packed::*,
    prelude::*,
    H256,
};
use ckb_x64_simulator::RunningSetup;
use std::collections::HashMap;

const MAX_CYCLES: u64 = 10_000_000;

struct PoASetup {
    pub identity_size: u8,
    pub round_interval_uses_seconds: bool,
    pub identities: Vec<Bytes>,
    pub aggregator_change_threshold: u8,
    pub round_intervals: u32,
    pub subblocks_per_round: u32,
}

fn serialize_poa_setup(setup: &PoASetup) -> Bytes {
    let mut buffer = BytesMut::new();
    if setup.round_interval_uses_seconds {
        buffer.extend_from_slice(&[1]);
    } else {
        buffer.extend_from_slice(&[0]);
    }
    if setup.identities.len() > 255 {
        panic!("Too many identities!");
    }
    buffer.extend_from_slice(&[
        setup.identity_size,
        setup.identities.len() as u8,
        setup.aggregator_change_threshold,
    ]);
    buffer.extend_from_slice(&setup.round_intervals.to_le_bytes()[..]);
    buffer.extend_from_slice(&setup.subblocks_per_round.to_le_bytes()[..]);
    for identity in &setup.identities {
        if identity.len() < setup.identity_size as usize {
            panic!("Invalid identity!");
        }
        buffer.extend_from_slice(&identity.slice(0..setup.identity_size as usize));
    }
    buffer.freeze()
}

struct PoAData {
    pub round_initial_subtime: u64,
    pub subblock_subtime: u64,
    pub subblock_index: u32,
    pub aggregator_index: u16,
}

fn serialize_poa_data(data: &PoAData) -> Bytes {
    let mut buffer = BytesMut::new();
    buffer.extend_from_slice(&data.round_initial_subtime.to_le_bytes()[..]);
    buffer.extend_from_slice(&data.subblock_subtime.to_le_bytes()[..]);
    buffer.extend_from_slice(&data.subblock_index.to_le_bytes()[..]);
    buffer.extend_from_slice(&data.aggregator_index.to_le_bytes()[..]);
    buffer.freeze()
}

#[test]
fn test_poa_normal_update() {
    // deploy contract
    let mut context = Context::default();
    let poa_bin: Bytes = Loader::default().load_binary("poa.strip");
    let poa_out_point = context.deploy_cell(poa_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let poa_owner_script1 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_owner_script2 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let simple_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_data_type_id_args = random_32bytes();
    let poa_setup_type_id_args = random_32bytes();
    let poa_data_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_data_type_id_args.pack())
        .build();
    let poa_setup_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_setup_type_id_args.pack())
        .build();
    let poa_lock_data = {
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&poa_setup_type_id_args);
        buffer.extend_from_slice(&poa_data_type_id_args);
        buffer.freeze()
    };
    let poa_lock_script = context
        .build_script(&poa_out_point, poa_lock_data)
        .expect("build script");
    let poa_script_dep = CellDep::new_builder()
        .out_point(poa_out_point.clone())
        .build();
    let always_success_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();

    // prepare cells
    let poa_setup_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_setup_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_setup(&PoASetup {
            identity_size: 32,
            round_interval_uses_seconds: true,
            identities: vec![
                poa_owner_script1.calc_script_hash().as_bytes(),
                poa_owner_script2.calc_script_hash().as_bytes(),
            ],
            aggregator_change_threshold: 2,
            round_intervals: 90,
            subblocks_per_round: 1,
        }),
    );
    let poa_setup_dep = CellDep::new_builder()
        .out_point(poa_setup_out_point.clone())
        .build();

    let owner_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(poa_owner_script2.clone())
            .build(),
        Bytes::new(),
    );
    let owner_input = CellInput::new_builder()
        .previous_output(owner_input_out_point)
        .build();
    let poa_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        Bytes::from_static(b"old"),
    );
    let poa_input = CellInput::new_builder()
        .previous_output(poa_input_out_point)
        .since(0x400000000000044cu64.pack())
        .build();
    let poa_data_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1000,
            subblock_subtime: 1000,
            aggregator_index: 0,
            subblock_index: 0,
        }),
    );
    let poa_data_input = CellInput::new_builder()
        .previous_output(poa_data_input_out_point)
        .build();
    let outputs = vec![
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
    ];

    let outputs_data = vec![
        Bytes::from_static(b"new"),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1100,
            subblock_subtime: 1100,
            aggregator_index: 1,
            subblock_index: 0,
        }),
    ];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(poa_input)
        .input(poa_data_input)
        .input(owner_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(poa_setup_dep)
        .cell_dep(poa_script_dep)
        .cell_dep(always_success_script_dep)
        .build();
    let tx = context.complete_tx(tx);

    // run
    let cycles = context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("pass verification");
    println!("consume cycles: {}", cycles);

    // dump raw test tx files
    let setup = RunningSetup {
        is_lock_script: true,
        is_output: false,
        script_index: 0,
        native_binaries: HashMap::default(),
    };
    write_native_setup(
        "poa_normal_update",
        "poa_sim",
        &tx,
        &context,
        &setup,
        0,
        true,
    );
}

#[test]
fn test_poa_normal_update_same_round() {
    // deploy contract
    let mut context = Context::default();
    let poa_bin: Bytes = Loader::default().load_binary("poa.strip");
    let poa_out_point = context.deploy_cell(poa_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let poa_owner_script1 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_owner_script2 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let simple_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_data_type_id_args = random_32bytes();
    let poa_setup_type_id_args = random_32bytes();
    let poa_data_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_data_type_id_args.pack())
        .build();
    let poa_setup_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_setup_type_id_args.pack())
        .build();
    let poa_lock_data = {
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&poa_setup_type_id_args);
        buffer.extend_from_slice(&poa_data_type_id_args);
        buffer.freeze()
    };
    let poa_lock_script = context
        .build_script(&poa_out_point, poa_lock_data)
        .expect("build script");
    let poa_script_dep = CellDep::new_builder()
        .out_point(poa_out_point.clone())
        .build();
    let always_success_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();

    // prepare cells
    let poa_setup_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_setup_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_setup(&PoASetup {
            identity_size: 32,
            round_interval_uses_seconds: true,
            identities: vec![
                poa_owner_script1.calc_script_hash().as_bytes(),
                poa_owner_script2.calc_script_hash().as_bytes(),
            ],
            aggregator_change_threshold: 2,
            round_intervals: 90,
            subblocks_per_round: 3,
        }),
    );
    let poa_setup_dep = CellDep::new_builder()
        .out_point(poa_setup_out_point.clone())
        .build();

    let owner_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(poa_owner_script2.clone())
            .build(),
        Bytes::new(),
    );
    let owner_input = CellInput::new_builder()
        .previous_output(owner_input_out_point)
        .build();
    let poa_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        Bytes::from_static(b"old"),
    );
    let poa_input = CellInput::new_builder()
        .previous_output(poa_input_out_point)
        .since(0x4000000000000400u64.pack())
        .build();
    let poa_data_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1000,
            subblock_subtime: 1023,
            aggregator_index: 1,
            subblock_index: 1,
        }),
    );
    let poa_data_input = CellInput::new_builder()
        .previous_output(poa_data_input_out_point)
        .build();
    let outputs = vec![
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
    ];

    let outputs_data = vec![
        Bytes::from_static(b"new"),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1000,
            subblock_subtime: 1024,
            aggregator_index: 1,
            subblock_index: 2,
        }),
    ];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(poa_input)
        .input(poa_data_input)
        .input(owner_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(poa_setup_dep)
        .cell_dep(poa_script_dep)
        .cell_dep(always_success_script_dep)
        .build();
    let tx = context.complete_tx(tx);

    // run
    let cycles = context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("pass verification");
    println!("consume cycles: {}", cycles);

    // dump raw test tx files
    let setup = RunningSetup {
        is_lock_script: true,
        is_output: false,
        script_index: 0,
        native_binaries: HashMap::default(),
    };
    write_native_setup(
        "poa_normal_update_same_round",
        "poa_sim",
        &tx,
        &context,
        &setup,
        0,
        true,
    );
}

#[test]
fn test_poa_overtime_update() {
    // deploy contract
    let mut context = Context::default();
    let poa_bin: Bytes = Loader::default().load_binary("poa.strip");
    let poa_out_point = context.deploy_cell(poa_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let poa_owner_script1 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_owner_script2 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let simple_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_data_type_id_args = random_32bytes();
    let poa_setup_type_id_args = random_32bytes();
    let poa_data_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_data_type_id_args.pack())
        .build();
    let poa_setup_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_setup_type_id_args.pack())
        .build();
    let poa_lock_data = {
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&poa_setup_type_id_args);
        buffer.extend_from_slice(&poa_data_type_id_args);
        buffer.freeze()
    };
    let poa_lock_script = context
        .build_script(&poa_out_point, poa_lock_data)
        .expect("build script");
    let poa_script_dep = CellDep::new_builder()
        .out_point(poa_out_point.clone())
        .build();
    let always_success_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();

    // prepare cells
    let poa_setup_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_setup_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_setup(&PoASetup {
            identity_size: 32,
            round_interval_uses_seconds: true,
            identities: vec![
                poa_owner_script1.calc_script_hash().as_bytes(),
                poa_owner_script2.calc_script_hash().as_bytes(),
            ],
            aggregator_change_threshold: 2,
            round_intervals: 90,
            subblocks_per_round: 1,
        }),
    );
    let poa_setup_dep = CellDep::new_builder()
        .out_point(poa_setup_out_point.clone())
        .build();

    let owner_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(poa_owner_script1.clone())
            .build(),
        Bytes::new(),
    );
    let owner_input = CellInput::new_builder()
        .previous_output(owner_input_out_point)
        .build();
    let poa_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        Bytes::from_static(b"old"),
    );
    let poa_input = CellInput::new_builder()
        .previous_output(poa_input_out_point)
        .since(0x40000000000004a6u64.pack())
        .build();
    let poa_data_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1000,
            subblock_subtime: 1000,
            aggregator_index: 0,
            subblock_index: 0,
        }),
    );
    let poa_data_input = CellInput::new_builder()
        .previous_output(poa_data_input_out_point)
        .build();
    let outputs = vec![
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
    ];

    let outputs_data = vec![
        Bytes::from_static(b"new"),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1190,
            subblock_subtime: 1190,
            aggregator_index: 0,
            subblock_index: 0,
        }),
    ];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(poa_input)
        .input(poa_data_input)
        .input(owner_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(poa_setup_dep)
        .cell_dep(poa_script_dep)
        .cell_dep(always_success_script_dep)
        .build();
    let tx = context.complete_tx(tx);

    // run
    let cycles = context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("pass verification");
    println!("consume cycles: {}", cycles);

    // dump raw test tx files
    let setup = RunningSetup {
        is_lock_script: true,
        is_output: false,
        script_index: 0,
        native_binaries: HashMap::default(),
    };
    write_native_setup(
        "poa_overtime_update",
        "poa_sim",
        &tx,
        &context,
        &setup,
        0,
        true,
    );
}

#[test]
fn test_poa_setup_update() {
    // deploy contract
    let mut context = Context::default();
    let poa_bin: Bytes = Loader::default().load_binary("poa.strip");
    let poa_out_point = context.deploy_cell(poa_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let poa_owner_script1 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_owner_script2 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let simple_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_data_type_id_args = random_32bytes();
    let poa_setup_type_id_args = random_32bytes();
    let poa_setup_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_setup_type_id_args.pack())
        .build();
    let poa_lock_data = {
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&poa_setup_type_id_args);
        buffer.extend_from_slice(&poa_data_type_id_args);
        buffer.freeze()
    };
    let poa_lock_script = context
        .build_script(&poa_out_point, poa_lock_data)
        .expect("build script");
    let poa_script_dep = CellDep::new_builder()
        .out_point(poa_out_point.clone())
        .build();
    let always_success_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();

    // prepare cells
    let poa_setup_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_setup_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_setup(&PoASetup {
            identity_size: 32,
            round_interval_uses_seconds: true,
            identities: vec![
                poa_owner_script1.calc_script_hash().as_bytes(),
                poa_owner_script2.calc_script_hash().as_bytes(),
            ],
            aggregator_change_threshold: 2,
            round_intervals: 90,
            subblocks_per_round: 1,
        }),
    );
    let poa_setup_input = CellInput::new_builder()
        .previous_output(poa_setup_out_point)
        .build();

    let owner1_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(poa_owner_script1.clone())
            .build(),
        Bytes::new(),
    );
    let owner1_input = CellInput::new_builder()
        .previous_output(owner1_input_out_point)
        .build();
    let owner2_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(poa_owner_script2.clone())
            .build(),
        Bytes::new(),
    );
    let owner2_input = CellInput::new_builder()
        .previous_output(owner2_input_out_point)
        .build();
    let poa_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        Bytes::from_static(b"old"),
    );
    let poa_input = CellInput::new_builder()
        .previous_output(poa_input_out_point)
        .build();
    let outputs = vec![
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_setup_type_id_script.clone()))
                    .build(),
            )
            .build(),
    ];

    let outputs_data = vec![
        Bytes::from_static(b"new"),
        serialize_poa_setup(&PoASetup {
            identity_size: 32,
            round_interval_uses_seconds: true,
            identities: vec![
                poa_owner_script1.calc_script_hash().as_bytes(),
                poa_owner_script2.calc_script_hash().as_bytes(),
            ],
            aggregator_change_threshold: 2,
            round_intervals: 47,
            subblocks_per_round: 2,
        }),
    ];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(poa_setup_input)
        .input(poa_input)
        .input(owner1_input)
        .input(owner2_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(poa_script_dep)
        .cell_dep(always_success_script_dep)
        .build();
    let tx = context.complete_tx(tx);

    // run
    let cycles = context
        .verify_tx(&tx, MAX_CYCLES)
        .expect("pass verification");
    println!("consume cycles: {}", cycles);

    // dump raw test tx files
    let setup = RunningSetup {
        is_lock_script: true,
        is_output: false,
        script_index: 1,
        native_binaries: HashMap::default(),
    };
    write_native_setup(
        "poa_setup_update",
        "poa_sim",
        &tx,
        &context,
        &setup,
        0,
        true,
    );
}

#[test]
fn test_poa_invalid_aggregator_failure() {
    // deploy contract
    let mut context = Context::default();
    let poa_bin: Bytes = Loader::default().load_binary("poa.strip");
    let poa_out_point = context.deploy_cell(poa_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let poa_owner_script1 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_owner_script2 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let simple_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_data_type_id_args = random_32bytes();
    let poa_setup_type_id_args = random_32bytes();
    let poa_data_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_data_type_id_args.pack())
        .build();
    let poa_setup_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_setup_type_id_args.pack())
        .build();
    let poa_lock_data = {
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&poa_setup_type_id_args);
        buffer.extend_from_slice(&poa_data_type_id_args);
        buffer.freeze()
    };
    let poa_lock_script = context
        .build_script(&poa_out_point, poa_lock_data)
        .expect("build script");
    let poa_script_dep = CellDep::new_builder()
        .out_point(poa_out_point.clone())
        .build();
    let always_success_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();

    // prepare cells
    let poa_setup_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_setup_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_setup(&PoASetup {
            identity_size: 32,
            round_interval_uses_seconds: true,
            identities: vec![
                poa_owner_script1.calc_script_hash().as_bytes(),
                poa_owner_script2.calc_script_hash().as_bytes(),
            ],
            aggregator_change_threshold: 2,
            round_intervals: 90,
            subblocks_per_round: 1,
        }),
    );
    let poa_setup_dep = CellDep::new_builder()
        .out_point(poa_setup_out_point.clone())
        .build();

    let owner_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(poa_owner_script1.clone())
            .build(),
        Bytes::new(),
    );
    let owner_input = CellInput::new_builder()
        .previous_output(owner_input_out_point)
        .build();
    let poa_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        Bytes::from_static(b"old"),
    );
    let poa_input = CellInput::new_builder()
        .previous_output(poa_input_out_point)
        .since(0x400000000000044cu64.pack())
        .build();
    let poa_data_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1000,
            subblock_subtime: 1000,
            aggregator_index: 0,
            subblock_index: 0,
        }),
    );
    let poa_data_input = CellInput::new_builder()
        .previous_output(poa_data_input_out_point)
        .build();
    let outputs = vec![
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
    ];

    let outputs_data = vec![
        Bytes::from_static(b"new"),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1100,
            subblock_subtime: 1100,
            aggregator_index: 1,
            subblock_index: 0,
        }),
    ];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(poa_input)
        .input(poa_data_input)
        .input(owner_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(poa_setup_dep)
        .cell_dep(poa_script_dep)
        .cell_dep(always_success_script_dep)
        .build();
    let tx = context.complete_tx(tx);

    // run
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("fail verification");

    // dump raw test tx files
    let setup = RunningSetup {
        is_lock_script: true,
        is_output: false,
        script_index: 0,
        native_binaries: HashMap::default(),
    };
    write_native_setup(
        "invalid_aggregator_failure",
        "poa_sim",
        &tx,
        &context,
        &setup,
        -2,
        true,
    );
}

#[test]
fn test_poa_since_timestamp_failure() {
    // deploy contract
    let mut context = Context::default();
    let poa_bin: Bytes = Loader::default().load_binary("poa.strip");
    let poa_out_point = context.deploy_cell(poa_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let poa_owner_script1 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_owner_script2 = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let simple_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let poa_data_type_id_args = random_32bytes();
    let poa_setup_type_id_args = random_32bytes();
    let poa_data_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_data_type_id_args.pack())
        .build();
    let poa_setup_type_id_script = Script::new_builder()
        .code_hash(h256!("0x545950455f4944").pack())
        .hash_type(ScriptHashType::Type.into())
        .args(poa_setup_type_id_args.pack())
        .build();
    let poa_lock_data = {
        let mut buffer = BytesMut::new();
        buffer.extend_from_slice(&poa_setup_type_id_args);
        buffer.extend_from_slice(&poa_data_type_id_args);
        buffer.freeze()
    };
    let poa_lock_script = context
        .build_script(&poa_out_point, poa_lock_data)
        .expect("build script");
    let poa_script_dep = CellDep::new_builder()
        .out_point(poa_out_point.clone())
        .build();
    let always_success_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();

    // prepare cells
    let poa_setup_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_setup_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_setup(&PoASetup {
            identity_size: 32,
            round_interval_uses_seconds: true,
            identities: vec![
                poa_owner_script1.calc_script_hash().as_bytes(),
                poa_owner_script2.calc_script_hash().as_bytes(),
            ],
            aggregator_change_threshold: 2,
            round_intervals: 90,
            subblocks_per_round: 1,
        }),
    );
    let poa_setup_dep = CellDep::new_builder()
        .out_point(poa_setup_out_point.clone())
        .build();

    let owner_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(poa_owner_script2.clone())
            .build(),
        Bytes::new(),
    );
    let owner_input = CellInput::new_builder()
        .previous_output(owner_input_out_point)
        .build();
    let poa_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        Bytes::from_static(b"old"),
    );
    let poa_input = CellInput::new_builder()
        .previous_output(poa_input_out_point)
        .since(0x4000000000000440u64.pack())
        .build();
    let poa_data_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1000,
            subblock_subtime: 1000,
            aggregator_index: 0,
            subblock_index: 0,
        }),
    );
    let poa_data_input = CellInput::new_builder()
        .previous_output(poa_data_input_out_point)
        .build();
    let outputs = vec![
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(poa_lock_script.clone())
            .build(),
        CellOutput::new_builder()
            .capacity(1000u64.pack())
            .lock(simple_lock_script.clone())
            .type_(
                ScriptOpt::new_builder()
                    .set(Some(poa_data_type_id_script.clone()))
                    .build(),
            )
            .build(),
    ];

    let outputs_data = vec![
        Bytes::from_static(b"new"),
        serialize_poa_data(&PoAData {
            round_initial_subtime: 1100,
            subblock_subtime: 1100,
            aggregator_index: 1,
            subblock_index: 0,
        }),
    ];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(poa_input)
        .input(poa_data_input)
        .input(owner_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(poa_setup_dep)
        .cell_dep(poa_script_dep)
        .cell_dep(always_success_script_dep)
        .build();
    let tx = context.complete_tx(tx);

    // run
    context
        .verify_tx(&tx, MAX_CYCLES)
        .expect_err("fail verification");

    // dump raw test tx files
    let setup = RunningSetup {
        is_lock_script: true,
        is_output: false,
        script_index: 0,
        native_binaries: HashMap::default(),
    };
    write_native_setup(
        "poa_since_timestamp_failure",
        "poa_sim",
        &tx,
        &context,
        &setup,
        -2,
        true,
    );
}
