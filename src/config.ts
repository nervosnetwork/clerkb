import Ajv from "ajv";
import { readFileSync } from "fs";
import schema from "./config_schema.json";

export function readConfig(filename: string) {
  return parseConfig(readFileSync("test.json", "utf8"));
}

export function parseConfig(configData: string) {
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
