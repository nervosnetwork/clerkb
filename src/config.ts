import Ajv from "ajv";
import { Reader } from "ckb-js-toolkit";
import { readFileSync } from "fs";
import schema from "./config_schema.json";

// TODO: build a proper type later
export type Config = any;

export function readConfig(filename: string): Config {
  return parseConfig(readFileSync("test.json", "utf8"));
}

export function parseConfig(configData: string): Config {
  const config = JSON.parse(configData);
  const ajv = new Ajv();
  const validate = ajv.compile(schema);
  const valid = validate(config);
  if (!valid) {
    throw new Error(ajv.errorsText(validate.errors));
  }
  // Additional check: at least one identity must exist
  if (config.poa_setup.identities.length === 0) {
    throw new Error("No identity is setup!");
  }
  // Additional check: there can at most be 255 aggregators
  if (config.poa_setup.identities.length > 255) {
    throw new Error("Too many aggregators!");
  }
  // Additional check: all identities must be of the same length
  const firstLength = config.poa_setup.identities[0].length;
  for (let i = 1; i < config.poa_setup.identities.length; i++) {
    if (config.poa_setup.identities[i].length !== firstLength) {
      throw new Error("Identity lengths must all be the same!");
    }
  }
  // Additional check: change threshold must not be larger than identity size
  if (
    config.poa_setup.aggregator_change_threshold >
    config.poa_setup.identities.length
  ) {
    throw new Error("Invalid change threshold!");
  }
  return config;
}

export function configToPoASetup(config: Config): ArrayBuffer {
  const poa_setup = config.poa_setup;
  const length =
    44 +
    poa_setup.identities.length * new Reader(poa_setup.identities[0]).length();
  const buffer = new ArrayBuffer(length);
  const view = new DataView(buffer);
  const uint8array = new Uint8Array(buffer);
  uint8array.set(
    new Uint8Array(new Reader(poa_setup.code_hash).toArrayBuffer()),
    0
  );
  let flag = 0;
  if (poa_setup.hash_type === "type") {
    flag |= 1;
  }
  if (poa_setup.interval_uses_seconds) {
    flag |= 2;
  }
  view.setUint8(32, flag);
  view.setUint8(33, poa_setup.identity_size);
  view.setUint8(34, poa_setup.identities.length);
  view.setUint8(35, poa_setup.aggregator_change_threshold);
  view.setUint32(36, poa_setup.subblock_intervals, true);
  view.setUint32(40, poa_setup.subblocks_per_interval, true);
  for (let i = 0; i < poa_setup.identities.length; i++) {
    uint8array.set(
      new Uint8Array(new Reader(poa_setup.identities[i]).toArrayBuffer()),
      44 + i * poa_setup.identity_size
    );
  }
  return buffer;
}
