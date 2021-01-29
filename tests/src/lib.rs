#[macro_use]
extern crate lazy_static;

use ckb_standalone_debugger::transaction::{
    MockCellDep, MockInfo, MockInput, MockTransaction, ReprMockTransaction,
};
use ckb_testtool::context::Context;
use ckb_tool::ckb_types::{
    bytes::Bytes,
    core::{DepType, TransactionView},
};
use ckb_x64_simulator::RunningSetup;
use rand::{thread_rng, Rng};
use serde_json::to_string_pretty;
use std::env;
use std::fs;
use std::io::Write;
use std::path::{Path, PathBuf};
use std::str::FromStr;

#[cfg(test)]
mod poa_tests;
#[cfg(test)]
mod state_tests;

lazy_static! {
    static ref LOADER: Loader = Loader::default();
    static ref TX_FOLDER: PathBuf = {
        let path = LOADER.path("dumped_tests");
        if Path::new(&path).exists() {
            fs::remove_dir_all(&path).expect("remove old dir");
        }
        fs::create_dir_all(&path).expect("create test dir");
        path
    };
}

const TEST_ENV_VAR: &str = "CAPSULE_TEST_ENV";

pub enum TestEnv {
    Debug,
    Release,
}

impl FromStr for TestEnv {
    type Err = &'static str;

    fn from_str(s: &str) -> Result<Self, Self::Err> {
        match s.to_lowercase().as_str() {
            "debug" => Ok(TestEnv::Debug),
            "release" => Ok(TestEnv::Release),
            _ => Err("no match"),
        }
    }
}

pub struct Loader(PathBuf);

impl Default for Loader {
    fn default() -> Self {
        let test_env = match env::var(TEST_ENV_VAR) {
            Ok(val) => val.parse().expect("test env"),
            Err(_) => TestEnv::Debug,
        };
        Self::with_test_env(test_env)
    }
}

impl Loader {
    fn with_test_env(env: TestEnv) -> Self {
        let load_prefix = match env {
            TestEnv::Debug => "debug",
            TestEnv::Release => "release",
        };
        let dir = env::current_dir().unwrap();
        let mut base_path = PathBuf::new();
        base_path.push(dir);
        base_path.push("..");
        base_path.push("build");
        base_path.push(load_prefix);
        Loader(base_path)
    }

    pub fn path(&self, name: &str) -> PathBuf {
        let mut path = self.0.clone();
        path.push(name);
        path
    }

    pub fn load_binary(&self, name: &str) -> Bytes {
        fs::read(self.path(name)).expect("binary").into()
    }
}

pub fn random_32bytes() -> Bytes {
    let mut rng = thread_rng();
    let mut buf = vec![0u8; 32];
    rng.fill(&mut buf[..]);
    Bytes::from(buf)
}

pub fn create_test_folder(name: &str) -> PathBuf {
    let mut path = TX_FOLDER.clone();
    path.push(&name);
    fs::create_dir_all(&path).expect("create folder");
    path
}

pub fn build_mock_transaction(tx: &TransactionView, context: &Context) -> MockTransaction {
    let mock_inputs = tx
        .inputs()
        .into_iter()
        .map(|input| {
            let (output, data) = context
                .get_cell(&input.previous_output())
                .expect("get cell");
            MockInput {
                input,
                output,
                data,
                header: None,
            }
        })
        .collect();
    let mock_cell_deps = tx
        .cell_deps()
        .into_iter()
        .map(|cell_dep| {
            if cell_dep.dep_type() == DepType::DepGroup.into() {
                panic!("Implement dep group support later!");
            }
            let (output, data) = context.get_cell(&cell_dep.out_point()).expect("get cell");
            MockCellDep {
                cell_dep,
                output,
                data,
                header: None,
            }
        })
        .collect();
    let mock_info = MockInfo {
        inputs: mock_inputs,
        cell_deps: mock_cell_deps,
        header_deps: vec![],
    };
    MockTransaction {
        mock_info,
        tx: tx.data(),
    }
}

pub fn rewrite_setup(setup: &RunningSetup, binary_suffix: &str) -> RunningSetup {
    let mut setup2 = setup.clone();
    setup2.native_binaries = setup
        .native_binaries
        .iter()
        .map(|(key, binary)| (key.clone(), format!("{}{}", binary, binary_suffix)))
        .collect();
    setup2
}

pub fn write_native_setup(
    test_name: &str,
    binary_name: &str,
    tx: &TransactionView,
    context: &Context,
    setup: &RunningSetup,
    return_code: i8,
    enable_sanitizers: bool,
) {
    let folder = create_test_folder(test_name);
    let mock_tx = build_mock_transaction(&tx, &context);
    let repr_tx: ReprMockTransaction = mock_tx.into();
    let tx_json = to_string_pretty(&repr_tx).expect("serialize to json");
    fs::write(folder.join("tx.json"), tx_json).expect("write tx to local file");
    let setup_json = to_string_pretty(setup).expect("serialize to json");
    fs::write(folder.join("setup.json"), setup_json).expect("write setup to local file");

    let mut cmd_file = fs::File::create(folder.join("cmd")).expect("create cmd file");
    write!(
        &mut cmd_file,
        "CKB_TX_FILE=\"{}\" CKB_RUNNING_SETUP=\"{}\" \"{}\" 2> err\n",
        folder.join("tx.json").to_str().expect("utf8"),
        folder.join("setup.json").to_str().expect("utf8"),
        Loader::default().path(binary_name).to_str().expect("utf8")
    )
    .expect("write");
    write!(&mut cmd_file, "error_code=$?\nif [ $error_code -ne {} ]; then\n    echo \"Return code $error_code is invalid!\"\n    cat err\n    exit 1\nfi\n", return_code as u8).expect("write");

    if enable_sanitizers {
        let ubsan_setup = rewrite_setup(setup, ".ubsan");
        let ubsan_setup_json = to_string_pretty(&ubsan_setup).expect("serialize to json");
        fs::write(folder.join("ubsan_setup.json"), ubsan_setup_json)
            .expect("write setup to local file");

        write!(
            &mut cmd_file,
            "CKB_TX_FILE=\"{}\" CKB_RUNNING_SETUP=\"{}\" \"{}.ubsan\" 2> err\n",
            folder.join("tx.json").to_str().expect("utf8"),
            folder.join("ubsan_setup.json").to_str().expect("utf8"),
            Loader::default().path(binary_name).to_str().expect("utf8")
        )
        .expect("write");
        write!(&mut cmd_file, "error_code=$?\nif [ $error_code -ne {} ]; then\n    echo \"Return code $error_code is invalid!\"\n    cat err\n    exit 1\nfi\n", return_code as u8).expect("write");
        write!(
            &mut cmd_file,
            "if [ -s err ]; then\n    echo \"Errors in stderr!\"\n    cat err\n    exit 1\nfi\n"
        )
        .expect("write");

        let asan_setup = rewrite_setup(setup, ".asan");
        let asan_setup_json = to_string_pretty(&asan_setup).expect("serialize to json");
        fs::write(folder.join("asan_setup.json"), asan_setup_json)
            .expect("write setup to local file");

        write!(
            &mut cmd_file,
            "CKB_TX_FILE=\"{}\" CKB_RUNNING_SETUP=\"{}\" \"{}.asan\" 2> err\n",
            folder.join("tx.json").to_str().expect("utf8"),
            folder.join("asan_setup.json").to_str().expect("utf8"),
            Loader::default().path(binary_name).to_str().expect("utf8")
        )
        .expect("write");
        write!(&mut cmd_file, "error_code=$?\nif [ $error_code -ne {} ]; then\n    echo \"Return code $error_code is invalid!\"\n    cat err\n    exit 1\nfi\n", return_code as u8).expect("write");
        write!(
            &mut cmd_file,
            "if [ -s err ]; then\n    echo \"Errors in stderr!\"\n    cat err\n    exit 1\nfi\n"
        )
        .expect("write");
    }
}
