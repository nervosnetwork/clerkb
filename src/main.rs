mod config;
mod error;

use async_std::{fs, task};
use bytes::BytesMut;
use ckb_jsonrpc_types::Script;
use ckb_types::{
    packed::ScriptBuilder,
    prelude::{Builder, Pack},
};
use clap::{App, AppSettings, Arg, SubCommand};
use config::{Config, IntervalType};
use error::{Error, Result};
use std::process::exit;

fn version_string() -> String {
    let major = env!("CARGO_PKG_VERSION_MAJOR")
        .parse::<u8>()
        .expect("CARGO_PKG_VERSION_MAJOR parse success");
    let minor = env!("CARGO_PKG_VERSION_MINOR")
        .parse::<u8>()
        .expect("CARGO_PKG_VERSION_MINOR parse success");
    let patch = env!("CARGO_PKG_VERSION_PATCH")
        .parse::<u16>()
        .expect("CARGO_PKG_VERSION_PATCH parse success");
    let mut version = format!("{}.{}.{}", major, minor, patch);
    let pre = env!("CARGO_PKG_VERSION_PRE");
    if !pre.is_empty() {
        version.push_str("-");
        version.push_str(pre);
    }
    let commit_id = env!("COMMIT_ID");
    version.push_str(" ");
    version.push_str(commit_id);
    version
}

fn main() {
    match task::block_on(run()) {
        Ok(_) => {}
        Err(err) => {
            eprintln!("error: {}", err);
            exit(-1);
        }
    }
}

async fn run() -> Result<()> {
    env_logger::init();

    let version = version_string();
    let matches = App::new("clerkb")
        .setting(AppSettings::ArgRequiredElseHelp)
        .version(version.as_str())
        .author("Nervos Developer Tools Team")
        .about("An aggregator for Nervos CKB operated in layer 1")
        .arg(
            Arg::with_name("config")
                .short("c")
                .long("config")
                .required(true)
                .takes_value(true)
                .help("Config file path"),
        )
        .subcommand(SubCommand::with_name("run").about("Start aggregator"))
        .subcommand(
            SubCommand::with_name("script")
                .about("Generate lock script associated with current config"),
        )
        .get_matches();

    let config_contents = fs::read(matches.value_of("config").expect("config")).await?;
    let config: Config = toml::de::from_slice(&config_contents)?;
    config.validate()?;

    match matches.subcommand() {
        ("script", _) => {
            println!("Code hash: {:#x}", config.lock.binary.code_hash);
            println!(
                "Hash type: {} ({})",
                config.lock.binary.hash_type,
                config.lock.binary.core_hash_type_string()
            );
            let mut args = BytesMut::new();
            args.extend_from_slice(config.lock.verification_library.code_hash.as_bytes());
            args.extend_from_slice(&[config.lock.verification_library.hash_type]);
            args.extend_from_slice(&[config.lock.identity_size]);
            args.extend_from_slice(&(config.lock.aggregators.len() as u16).to_le_bytes()[..]);
            let block_intervals: u32 = match config.lock.interval_type {
                IntervalType::Blocks => config.lock.block_intervals as u32,
                IntervalType::Seconds => (config.lock.block_intervals as u32) | 0x80000000,
            };
            args.extend_from_slice(&block_intervals.to_le_bytes()[..]);
            args.extend_from_slice(&config.lock.data_info_offset.to_le_bytes()[..]);
            for aggregator in &config.lock.aggregators {
                args.extend_from_slice(aggregator.as_bytes());
            }
            println!("Args: 0x{:#x}", args);
            let script = ScriptBuilder::default()
                .code_hash(config.lock.binary.code_hash.pack())
                .hash_type(config.lock.binary.core_hash_type().into())
                .args(args.freeze().pack())
                .build();
            println!("Molecule serialized script: {:#x}", script);
            let json_script: Script = script.into();
            println!(
                "JSON serialized script: {}",
                serde_json::to_string_pretty(&json_script).unwrap()
            );
        }
        ("run", _) => unimplemented!(),
        (command, _) => {
            return Err(Error::InvalidCommand(command.to_string()).into());
        }
    };

    Ok(())
}
