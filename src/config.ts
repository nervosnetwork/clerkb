import { HexString } from "@ckb-lumos/base";
import Ajv from "ajv";
import { Reader } from "ckb-js-toolkit";
import { readFileSync } from "fs";
import schema from "./config_schema.json";

export interface PoASetup {
  identity_size: number;
  interval_uses_seconds: boolean;
  identities: Array<HexString>;
  aggregator_change_threshold: number;
  subblock_intervals: number;
  subblocks_per_interval: number;
}

export interface PoAData {
  round_initial_subtime: bigint;
  subblock_subtime: bigint;
  subblock_index: number;
  aggregator_index: number;
}

export interface Config {
  poa_setup: PoASetup;
}

export function readConfig(filename: string): Config {
  return parseConfig(readFileSync("test.json", "utf8"));
}

export function parseConfig(configData: string): Config {
  const config = JSON.parse(configData);
  return validateConfig(config);
}

export function validateConfig(config: Config): Config {
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

export function parsePoASetup(buffer: ArrayBuffer): PoASetup {
  if (buffer.byteLength < 12) {
    throw new Error("Invalid length!");
  }
  const bufferArray = new Uint8Array(buffer);
  const view = new DataView(buffer);
  const identitySize = view.getUint8(1);
  const aggregatorNumber = view.getUint8(2);
  if (buffer.byteLength !== 12 + identitySize * aggregatorNumber) {
    throw new Error("Invalid length!");
  }
  const identities = [];
  for (let i = 0; i < aggregatorNumber; i++) {
    const identityBuffer = new ArrayBuffer(identitySize);
    const identityArray = new Uint8Array(identityBuffer);
    const offset = 12 + i * identitySize;
    identityArray.set(bufferArray.slice(offset, offset + identitySize));
    identities.push(new Reader(identityBuffer).serializeJson());
  }
  const setup: PoASetup = {
    interval_uses_seconds: (view.getUint8(0) & 1) === 1,
    aggregator_change_threshold: view.getUint8(3),
    subblock_intervals: view.getUint32(4, true),
    subblocks_per_interval: view.getUint32(8, true),
    identity_size: identitySize,
    identities: identities,
  };
  return validateConfig({ poa_setup: setup }).poa_setup;
}

export function serializePoASetup(poaSetup: PoASetup): ArrayBuffer {
  const length =
    12 +
    poaSetup.identities.length * new Reader(poaSetup.identities[0]).length();
  const buffer = new ArrayBuffer(length);
  const view = new DataView(buffer);
  const uint8array = new Uint8Array(buffer);
  view.setUint8(0, poaSetup.interval_uses_seconds ? 1 : 0);
  view.setUint8(1, poaSetup.identity_size);
  view.setUint8(2, poaSetup.identities.length);
  view.setUint8(3, poaSetup.aggregator_change_threshold);
  view.setUint32(4, poaSetup.subblock_intervals, true);
  view.setUint32(8, poaSetup.subblocks_per_interval, true);
  for (let i = 0; i < poaSetup.identities.length; i++) {
    uint8array.set(
      new Uint8Array(new Reader(poaSetup.identities[i]).toArrayBuffer()),
      12 + i * poaSetup.identity_size
    );
  }
  return buffer;
}

export function parsePoAData(buffer: ArrayBuffer): PoAData {
  if (buffer.byteLength !== 22) {
    throw new Error("Invalid length!");
  }
  const view = new DataView(buffer);
  return {
    round_initial_subtime: view.getBigUint64(0, true),
    subblock_subtime: view.getBigUint64(8, true),
    subblock_index: view.getUint32(16, true),
    aggregator_index: view.getUint16(20, true),
  };
}

export function serializePoAData(poaData: PoAData): ArrayBuffer {
  const buffer = new ArrayBuffer(22);
  const view = new DataView(buffer);
  view.setBigUint64(0, poaData.round_initial_subtime, true);
  view.setBigUint64(8, poaData.subblock_subtime, true);
  view.setUint32(16, poaData.subblock_index, true);
  view.setUint16(20, poaData.aggregator_index, true);
  return buffer;
}
