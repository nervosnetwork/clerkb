use super::*;
use ckb_testtool::{builtin::ALWAYS_SUCCESS, context::Context};
use ckb_tool::ckb_types::{bytes::Bytes, core::TransactionBuilder, packed::*, prelude::*};
use ckb_x64_simulator::RunningSetup;
use std::collections::HashMap;

const MAX_CYCLES: u64 = 10_000_000;

#[test]
fn test_state_normal_unlock() {
    // deploy contract
    let mut context = Context::default();
    let state_bin: Bytes = Loader::default().load_binary("state.strip");
    let state_out_point = context.deploy_cell(state_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let target_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let target_lock_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();
    let state_lock_script = context
        .build_script(
            &state_out_point,
            target_lock_script.calc_script_hash().as_bytes(),
        )
        .expect("build script");
    let state_lock_dep = CellDep::new_builder()
        .out_point(state_out_point.clone())
        .build();
    let output_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");

    // prepare cells
    let target_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(target_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    let target_input = CellInput::new_builder()
        .previous_output(target_input_out_point)
        .build();
    let state_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(state_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    let state_input = CellInput::new_builder()
        .previous_output(state_input_out_point)
        .build();
    let outputs = vec![CellOutput::new_builder()
        .capacity(1000u64.pack())
        .lock(output_lock_script.clone())
        .build()];
    let outputs_data = vec![Bytes::new()];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(state_input)
        .input(target_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(state_lock_dep)
        .cell_dep(target_lock_script_dep)
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
        "state_normal_unlock",
        "state_sim",
        &tx,
        &context,
        &setup,
        0,
        true,
    );
}

#[test]
fn test_state_update_failure() {
    // deploy contract
    let mut context = Context::default();
    let state_bin: Bytes = Loader::default().load_binary("state.strip");
    let state_out_point = context.deploy_cell(state_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let target_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let target_lock_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();
    let state_lock_script = context
        .build_script(&state_out_point, random_32bytes())
        .expect("build script");
    let state_lock_dep = CellDep::new_builder()
        .out_point(state_out_point.clone())
        .build();
    let output_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");

    // prepare cells
    let target_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(target_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    let target_input = CellInput::new_builder()
        .previous_output(target_input_out_point)
        .build();
    let state_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(state_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    let state_input = CellInput::new_builder()
        .previous_output(state_input_out_point)
        .build();
    let outputs = vec![CellOutput::new_builder()
        .capacity(1000u64.pack())
        .lock(output_lock_script.clone())
        .build()];
    let outputs_data = vec![Bytes::new()];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(state_input)
        .input(target_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(state_lock_dep)
        .cell_dep(target_lock_script_dep)
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
        "state_update_failure",
        "state_sim",
        &tx,
        &context,
        &setup,
        1,
        true,
    );
}

#[test]
fn test_state_invalid_args_failure() {
    // deploy contract
    let mut context = Context::default();
    let state_bin: Bytes = Loader::default().load_binary("state.strip");
    let state_out_point = context.deploy_cell(state_bin);
    let always_success_out_point = context.deploy_cell(ALWAYS_SUCCESS.clone());

    // prepare scripts
    let target_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");
    let target_lock_script_dep = CellDep::new_builder()
        .out_point(always_success_out_point.clone())
        .build();
    let state_lock_script = context
        .build_script(
            &state_out_point,
            target_lock_script
                .calc_script_hash()
                .as_bytes()
                .slice(0..16),
        )
        .expect("build script");
    let state_lock_dep = CellDep::new_builder()
        .out_point(state_out_point.clone())
        .build();
    let output_lock_script = context
        .build_script(&always_success_out_point, random_32bytes())
        .expect("build script");

    // prepare cells
    let target_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(target_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    let target_input = CellInput::new_builder()
        .previous_output(target_input_out_point)
        .build();
    let state_input_out_point = context.create_cell(
        CellOutput::new_builder()
            .capacity(500u64.pack())
            .lock(state_lock_script.clone())
            .build(),
        Bytes::new(),
    );
    let state_input = CellInput::new_builder()
        .previous_output(state_input_out_point)
        .build();
    let outputs = vec![CellOutput::new_builder()
        .capacity(1000u64.pack())
        .lock(output_lock_script.clone())
        .build()];
    let outputs_data = vec![Bytes::new()];

    // build transaction
    let tx = TransactionBuilder::default()
        .input(state_input)
        .input(target_input)
        .outputs(outputs)
        .outputs_data(outputs_data.pack())
        .cell_dep(state_lock_dep)
        .cell_dep(target_lock_script_dep)
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
        "state_invalid_args_failure",
        "state_sim",
        &tx,
        &context,
        &setup,
        -1,
        true,
    );
}
