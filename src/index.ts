import {
  HexString,
  Cell,
  PackedSince,
  core,
  utils,
  values,
} from "@ckb-lumos/base";
const { CKBHasher, ckbHash } = utils;
import { LockScriptInfo, FromInfo } from "@ckb-lumos/common-scripts";
import { ScriptConfig, Config } from "@ckb-lumos/config-manager";
import {
  TransactionSkeletonType,
  Options,
  createTransactionFromSkeleton,
} from "@ckb-lumos/helpers";
import { normalizers, Reader } from "ckb-js-toolkit";
import { Set } from "immutable";

function hashBigInt(hasher: any, i: bigint): void {
  const lengthBuffer = new ArrayBuffer(8);
  const view = new DataView(lengthBuffer);
  view.setBigUint64(0, i, true);
  hasher.update(lengthBuffer);
}

function hashWitness(hasher: any, witness: HexString): void {
  hashBigInt(hasher, BigInt(new Reader(witness).length()));
  hasher.update(witness);
}

export function generateLockScriptInfo(
  poaConfig: ScriptConfig
): LockScriptInfo {
  return {
    code_hash: poaConfig.CODE_HASH,
    hash_type: poaConfig.HASH_TYPE,
    lockScriptInfo: {
      CellCollector: "unsupported yet",
      setupInputCell: function (
        txSkeleton: TransactionSkeletonType,
        inputCell: Cell,
        fromInfo?: FromInfo,
        options?: {
          config?: Config;
          defaultWitness?: HexString;
          since?: PackedSince;
        }
      ): Promise<TransactionSkeletonType> {
        return Promise.reject(new Error("unsupported yet"));
      },
      prepareSigningEntries: function (
        txSkeleton: TransactionSkeletonType,
        options: Options
      ): TransactionSkeletonType {
        let processedArgs = Set<string>();
        const tx = createTransactionFromSkeleton(txSkeleton);
        const txHash = ckbHash(
          core.SerializeRawTransaction(normalizers.NormalizeRawTransaction(tx))
        ).serializeJson();
        const inputs = txSkeleton.get("inputs");
        const witnesses = txSkeleton.get("witnesses");
        let signingEntries = txSkeleton.get("signingEntries");
        for (let i = 0; i < inputs.size; i++) {
          const input = inputs.get(i)!;
          if (
            poaConfig.CODE_HASH === input.cell_output.lock.code_hash &&
            poaConfig.HASH_TYPE === input.cell_output.lock.hash_type &&
            !processedArgs.has(input.cell_output.lock.args)
          ) {
            processedArgs = processedArgs.add(input.cell_output.lock.args);
            const lockValue = new values.ScriptValue(input.cell_output.lock, {
              validate: false,
            });
            const hasher = new CKBHasher();
            hasher.update(txHash);
            if (i >= witnesses.size) {
              throw new Error(
                `The first witness in the script group starting at input index ${i} does not exist, maybe some other part has invalidly tampered the transaction?`
              );
            }
            // Different from normal signing scheme, here the whole signature part
            // of the first witness is skipped.
            const firstWitness = witnesses.get(i)!;
            const firstWitnessBuffer = new Reader(firstWitness).toArrayBuffer();
            hashBigInt(hasher, BigInt(firstWitnessBuffer.byteLength));
            hasher.update(firstWitnessBuffer.slice(0, 20));
            const firstWitnessView = new DataView(firstWitnessBuffer);
            const lockLength = firstWitnessView.getUint32(16, true);
            hasher.update(firstWitnessBuffer.slice(20 + lockLength));
            for (let j = i + 1; j < inputs.size && j < witnesses.size; j++) {
              const otherInput = inputs.get(j)!;
              if (
                lockValue.equals(
                  new values.ScriptValue(otherInput.cell_output.lock, {
                    validate: false,
                  })
                )
              ) {
                hashWitness(hasher, witnesses.get(j)!);
              }
            }
            for (let j = inputs.size; j < witnesses.size; j++) {
              hashWitness(hasher, witnesses.get(j)!);
            }
            const signingEntry = {
              type: "witness_args_lock",
              index: i,
              message: hasher.digestHex(),
            };
            signingEntries = signingEntries.push(signingEntry);
          }
        }
        txSkeleton = txSkeleton.set("signingEntries", signingEntries);
        return txSkeleton;
      },
    },
  };
}
