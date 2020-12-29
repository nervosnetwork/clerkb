import { Reader } from "ckb-js-toolkit";
import {
  core,
  since,
  utils,
  Cell,
  CellDep,
  Hash,
  HashType,
  Indexer,
  HexNumber,
  HexString,
  Script,
} from "@ckb-lumos/base";
import { common } from "@ckb-lumos/common-scripts";
import { getConfig, initializeConfig } from "@ckb-lumos/config-manager";
import { TransactionSkeletonType, addressToScript } from "@ckb-lumos/helpers";
import {
  PoAData,
  parsePoAData,
  parsePoASetup,
  serializePoAData,
} from "./config";

type State = "Yes" | "YesIfFull" | "No";

function pushAndFix(
  txSkeleton: TransactionSkeletonType,
  cell: Cell,
  field: "inputs" | "outputs"
) {
  const index = txSkeleton.get(field).count();
  txSkeleton = txSkeleton.update(field, (items) => items.push(cell));
  return txSkeleton.update("fixedEntries", (fixedEntries) => {
    return fixedEntries.push({
      field,
      index,
    });
  });
}

initializeConfig();

export class PoAGenerator {
  ckbAddress: string;
  indexer: Indexer;
  cellDeps: CellDep[];
  roundStartSubtime: bigint | undefined;
  logger: (message: string) => void;

  constructor(
    ckbAddress: string,
    indexer: Indexer,
    cellDeps: CellDep[],
    logger?: (message: string) => void
  ) {
    this.ckbAddress = ckbAddress;
    this.indexer = indexer;
    this.cellDeps = cellDeps;
    this.roundStartSubtime = undefined;
    this.logger = logger || ((_message) => undefined);
  }

  async shouldIssueNewBlock(
    medianTimeHex: HexNumber,
    tipCell: Cell
  ): Promise<State> {
    const medianTime = BigInt(medianTimeHex) / 1000n;
    const { poaData, poaSetup, aggregatorIndex } = await this._queryPoAInfos(
      tipCell
    );
    if (this.roundStartSubtime) {
      const remaining =
        this.roundStartSubtime + BigInt(poaSetup.round_intervals) - medianTime;
      if (remaining > 0) {
        this.logger(`Aggregator in round, remaining time: ${remaining}`);
        return "YesIfFull";
      } else {
        this.roundStartSubtime = undefined;
      }
    }
    let steps =
      (aggregatorIndex +
        poaSetup.identities.length -
        poaData.aggregator_index) %
      poaSetup.identities.length;
    if (steps === 0) {
      steps = poaSetup.identities.length;
    }
    const initialTime = poaData.round_initial_subtime;
    const nextStartTime =
      initialTime + BigInt(poaSetup.round_intervals) * BigInt(steps);
    this.logger(
      `On chain index: ${poaData.aggregator_index}, steps: ${steps}, initial time: ${initialTime}, next start time: ${nextStartTime}`
    );
    if (medianTime >= nextStartTime) {
      this.roundStartSubtime = medianTime;
      return "Yes";
    }
    return "No";
  }

  async fixTransactionSkeleton(
    medianTimeHex: HexNumber,
    txSkeleton: TransactionSkeletonType
  ): Promise<TransactionSkeletonType> {
    const {
      poaData,
      poaDataCell,
      poaSetup,
      poaSetupCell,
      aggregatorIndex,
      script,
      scriptHash,
    } = await this._queryPoAInfos(txSkeleton.get("inputs").get(0)!);
    for (const cellDep of this.cellDeps) {
      txSkeleton = txSkeleton.update("cellDeps", (cellDeps) =>
        cellDeps.push(cellDep)
      );
    }
    txSkeleton = txSkeleton.update("cellDeps", (cellDeps) =>
      cellDeps.push({
        out_point: poaSetupCell.out_point!,
        dep_type: "code",
      })
    );
    txSkeleton = pushAndFix(txSkeleton, poaDataCell, "inputs");
    // Dummy witness to hold the place for input cell.
    txSkeleton = txSkeleton.update("witnesses", (witnesses) =>
      witnesses.push("0x")
    );
    const medianTime = BigInt(medianTimeHex) / 1000n;
    let newPoAData: PoAData;
    if (
      medianTime <
        poaData.round_initial_subtime + BigInt(poaSetup.round_intervals) &&
      poaData.subblock_index + 1 < poaSetup.subblocks_per_round
    ) {
      // New block in current round
      newPoAData = {
        round_initial_subtime: poaData.round_initial_subtime,
        subblock_subtime: poaData.subblock_subtime + 1n,
        subblock_index: poaData.subblock_index + 1,
        aggregator_index: poaData.aggregator_index,
      };
    } else {
      // New block in new round
      newPoAData = {
        round_initial_subtime: medianTime,
        subblock_subtime: medianTime,
        subblock_index: 0,
        aggregator_index: aggregatorIndex,
      };
    }
    // Update PoA cell since time
    // TODO: block interval handling
    txSkeleton = txSkeleton.update("inputSinces", (inputSinces) => {
      return inputSinces.set(
        0,
        since.generateSince({
          relative: false,
          type: "blockTimestamp",
          value: newPoAData.subblock_subtime,
        })
      );
    });
    const newPackedPoAData = new Reader(
      serializePoAData(newPoAData)
    ).serializeJson();
    const newPoADataCell = {
      cell_output: poaDataCell.cell_output,
      data: newPackedPoAData,
    };
    txSkeleton = pushAndFix(txSkeleton, newPoADataCell, "outputs");
    // Add one owner cell if not exists already
    const ownerCells = txSkeleton.get("inputs").filter((cell) => {
      const currentScriptHash = utils.computeScriptHash(cell.cell_output.lock);
      return currentScriptHash === scriptHash;
    });
    if (ownerCells.count() === 0) {
      const ownerCell = await this._queryOwnerCell(script);
      txSkeleton = await common.setupInputCell(txSkeleton, ownerCell);
    }
    return txSkeleton;
  }

  async _queryPoAInfos(tipCell: Cell) {
    const poaDataCellTypeHash = new Reader(
      new Reader(tipCell.cell_output.lock.args).toArrayBuffer().slice(32)
    );
    if (poaDataCellTypeHash.length() !== 32) {
      throw new Error("Invalid PoA cell lock args!");
    }
    const poaDataCell = await this._queryPoaStateCell(
      poaDataCellTypeHash.serializeJson()
    );
    const poaData = parsePoAData(new Reader(poaDataCell.data).toArrayBuffer());
    const poaSetupCellTypeHash = new Reader(
      new Reader(tipCell.cell_output.lock.args).toArrayBuffer().slice(0, 32)
    );
    const poaSetupCell = await this._queryPoaStateCell(
      poaSetupCellTypeHash.serializeJson()
    );
    const poaSetup = parsePoASetup(
      new Reader(poaSetupCell.data).toArrayBuffer()
    );
    if (!poaSetup.round_interval_uses_seconds) {
      throw new Error("TODO: implement block interval PoA");
    }
    let script = addressToScript(this.ckbAddress);
    let scriptHash = utils.computeScriptHash(script);
    let truncatedScriptHash = new Reader(
      new Reader(scriptHash).toArrayBuffer().slice(0, poaSetup.identity_size)
    ).serializeJson();
    const aggregatorIndex = poaSetup.identities.findIndex(
      (identity) => identity === truncatedScriptHash
    );
    if (aggregatorIndex < 0) {
      throw new Error("Specified identity cannot be located!");
    }
    return {
      poaData,
      poaDataCell,
      poaSetup,
      poaSetupCell,
      aggregatorIndex,
      scriptHash,
      script,
    };
  }

  async _queryPoaStateCell(args: Hash) {
    const query = {
      type: {
        code_hash:
          "0x00000000000000000000000000000000000000000000000000545950455f4944",
        hash_type: "type" as HashType,
        args: args,
      },
    };
    const collector = this.indexer.collector(query);
    const results = [];
    for await (const cell of collector.collect()) {
      results.push(cell);
    }
    if (results.length !== 1) {
      throw new Error(`Invalid number of poa state cells: ${results.length}`);
    }
    return results[0];
  }

  async _queryOwnerCell(script: Script) {
    const query = {
      lock: script,
    };
    const collector = this.indexer.collector(query);
    for await (const cell of collector.collect()) {
      return cell;
    }
    throw new Error("Cannot find any owner cell!");
  }
}
