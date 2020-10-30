// # PoA
//
// A lock script used for proof of authority governance on CKB.

// Due to the way CKB works, shared state in dapps is a common problem requiring
// special care. One naive solution, is to introduce a certain kind of
// aggregator, that would pack multiple invididual actions on the sahred state
// into a single CKB transaction. But one issue with aggregator is
// centralization: with one aggregator, the risk of censoring is quite high.
// This script provides a simple attempt at the problem: we will just use
// multiple aggregators! Each aggregator can only issue a new transaction when
// their round is reached. Notice that this is by no means the solution to the
// problem we are facing. Many better attempts are being built, the lock script
// here, simply is built to show one of many possibilities on CKB, and help
// inspire new ideas.

// Terminologies:
// * Subblock: a CKB transaction generated by the aggregator, which can contain
// multiple individual actions. It's like a layer 2 block except all validations
// here happens on layer 1 CKB.
// * Subtime: timestamp, or block number for a subblock.
// * Interval: duration in which only one designated aggregator can issue new
// subblocks, measured in subtime.
// * Round: a single interval duration. One aggregator could issue more than one
// subblock in its round.

// As always, we will need those headers to interact with CKB.
#include "blake2b.h"
#include "blockchain.h"
#include "ckb_dlfcn.h"
#include "ckb_syscalls.h"

#define SCRIPT_BUFFER_SIZE 128
#define POA_BUFFER_SIZE 16384
#define SIGNATURE_WITNESS_BUFFER_SIZE 32768
#define ONE_BATCH_SIZE 32768
#define CODE_SIZE (256 * 1024)
#define PREFILLED_DATA_SIZE (1024 * 1024)
#define IDENTITY_SIZE 1024

#define ERROR_TRANSACTION -1
#define ERROR_ENCODING -2
#define ERROR_DYNAMIC_LOADING -3

#ifdef ENABLE_DEBUG_MODE
#define DEBUG(s) ckb_debug(s)
#else
#define DEBUG(s)
#endif /* ENABLE_DEBUG_MODE */

typedef struct {
  const uint8_t *_source_data;
  size_t _source_length;

  const uint8_t *code_hash;
  uint8_t hash_type;
  int interval_uses_seconds;
  uint8_t identity_size;
  uint8_t aggregator_number;
  uint8_t aggregator_change_threshold;
  uint32_t subblock_intervals;
  uint32_t subblocks_per_interval;
  const uint8_t *identities;
} PoASetup;

int parse_poa_setup(const uint8_t *source_data, size_t source_length,
                    PoASetup *output) {
  if (source_length < 44) {
    DEBUG("PoA data have invalid length!");
    return ERROR_ENCODING;
  }
  output->_source_data = source_data;
  output->_source_length = source_length;

  output->code_hash = source_data;
  output->hash_type = source_data[32] & 1;
  output->interval_uses_seconds = ((source_data[32] >> 1) & 1) == 1;
  output->identity_size = source_data[33];
  output->aggregator_number = source_data[34];
  output->aggregator_change_threshold = source_data[35];
  output->subblock_intervals = *((uint32_t *)(&source_data[36]));
  output->subblocks_per_interval = *((uint32_t *)(&source_data[40]));
  output->identities = &source_data[44];

  if (output->aggregator_change_threshold > output->aggregator_number) {
    DEBUG("Invalid aggregator change threshold!");
    return ERROR_ENCODING;
  }
  if (source_length !=
      44 + (size_t)output->identity_size * (size_t)output->aggregator_number) {
    DEBUG("PoA data have invalid length!");
    return ERROR_ENCODING;
  }
  return CKB_SUCCESS;
}

int load_and_hash_witness(blake2b_state *ctx, size_t start, size_t index,
                          size_t source) {
  uint8_t temp[ONE_BATCH_SIZE];
  uint64_t len = ONE_BATCH_SIZE;
  int ret = ckb_load_witness(temp, &len, start, index, source);
  if (ret != CKB_SUCCESS) {
    return ret;
  }
  blake2b_update(ctx, (char *)&len, sizeof(uint64_t));
  uint64_t offset = (len > ONE_BATCH_SIZE) ? ONE_BATCH_SIZE : len;
  blake2b_update(ctx, temp, offset);
  while (offset < len) {
    uint64_t current_len = ONE_BATCH_SIZE;
    ret = ckb_load_witness(temp, &current_len, start + offset, index, source);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    uint64_t current_read =
        (current_len > ONE_BATCH_SIZE) ? ONE_BATCH_SIZE : current_len;
    blake2b_update(ctx, temp, current_read);
    offset += current_read;
  }
  return CKB_SUCCESS;
}

uint8_t code_buffer[CODE_SIZE] __attribute__((aligned(RISCV_PGSIZE)));
uint64_t consumed_size = 0;
uint8_t prefilled_data_buffer[PREFILLED_DATA_SIZE];
int (*verify_func)(void *, const uint8_t *, size_t, const uint8_t *, size_t,
                   uint8_t *, size_t *) = NULL;

int initialize_signature_library(const uint8_t *code_hash, uint8_t hash_type) {
  if (verify_func != NULL) {
    DEBUG("Signature library already initialized!");
    return ERROR_DYNAMIC_LOADING;
  }
  void *handle = NULL;
  int ret = ckb_dlopen2(code_hash, hash_type, code_buffer,
                        CODE_SIZE - consumed_size, &handle, &consumed_size);
  if (ret != CKB_SUCCESS) {
    return ret;
  }
  int (*load_prefilled_data_func)(void *, size_t *);
  *(void **)(&load_prefilled_data_func) =
      ckb_dlsym(handle, "load_prefilled_data");
  if (load_prefilled_data_func == NULL) {
    DEBUG("Error loading load prefilled data func!");
    return ERROR_DYNAMIC_LOADING;
  }
  uint64_t len = PREFILLED_DATA_SIZE;
  ret = load_prefilled_data_func(prefilled_data_buffer, &len);
  if (ret != CKB_SUCCESS) {
    DEBUG("Error loading prefilled data!");
    return ret;
  }
  *(void **)(&verify_func) = ckb_dlsym(handle, "validate_signature");
  if (verify_func == NULL) {
    DEBUG("Error loading validate signature func!");
    return ERROR_DYNAMIC_LOADING;
  }
  return CKB_SUCCESS;
}

int validate_signatures(const uint8_t *signatures, size_t signature_size,
                        uint8_t signature_count, const uint8_t *identity_buffer,
                        size_t identity_size, uint8_t identity_count,
                        const uint8_t message[32]) {
  if (verify_func == NULL) {
    DEBUG("Signature library is not initialized!");
    return ERROR_DYNAMIC_LOADING;
  }
  uint64_t mask[4];
  mask[0] = mask[1] = mask[2] = mask[3] = 0;
  for (uint8_t i = 0; i < signature_count; i++) {
    uint8_t output_identity[IDENTITY_SIZE];
    uint64_t len = IDENTITY_SIZE;
    int ret =
        verify_func(prefilled_data_buffer, &signatures[i * signature_size],
                    signature_size, message, 32, output_identity, &len);
    if (ret != CKB_SUCCESS) {
      DEBUG("Error validating signature");
      return ret;
    }
    if (len != identity_size) {
      DEBUG("Identity size does not match!");
      return ERROR_ENCODING;
    }
    uint8_t found_identity = 0;
    for (; found_identity < identity_count; found_identity++) {
      if (memcmp(output_identity,
                 &identity_buffer[found_identity * identity_size],
                 identity_size) == 0) {
        break;
      }
    }
    if (found_identity >= identity_count) {
      DEBUG("Signature does not match any identity!");
      return ERROR_ENCODING;
    }
    if (((mask[found_identity / 64] >> (found_identity % 64)) & 1) != 0) {
      DEBUG("Multiple signature comes from one identity!");
      return ERROR_ENCODING;
    }
    mask[found_identity / 64] |= 1 << (found_identity % 64);
  }
  return CKB_SUCCESS;
}

int validate_signature(const uint8_t *signature, size_t signature_size,
                       const uint8_t *identity, size_t identity_size,
                       const uint8_t message[32]) {
  if (verify_func == NULL) {
    DEBUG("Signature library is not initialized!");
    return ERROR_DYNAMIC_LOADING;
  }
  uint8_t output_identity[IDENTITY_SIZE];
  uint64_t len = IDENTITY_SIZE;
  int ret = verify_func(prefilled_data_buffer, signature, signature_size,
                        message, 32, output_identity, &len);
  if (ret != CKB_SUCCESS) {
    DEBUG("Error validating signature");
    return ret;
  }
  if (len != identity_size) {
    DEBUG("Identity size does not match!");
    return ERROR_ENCODING;
  }
  if (memcmp(output_identity, identity, identity_size) != 0) {
    DEBUG("Identities do not match!");
    return ERROR_ENCODING;
  }
  return CKB_SUCCESS;
}

int look_for_poa_cell(const uint8_t *type_hash, size_t source, size_t *index) {
  size_t current = 0;
  size_t found_index = SIZE_MAX;
  int running = 1;
  while ((running == 1) && (current < SIZE_MAX)) {
    uint64_t len = 32;
    uint8_t hash[32];

    int ret = ckb_load_cell_by_field(hash, &len, 0, current, source,
                                     CKB_CELL_FIELD_TYPE_HASH);
    switch (ret) {
      case CKB_ITEM_MISSING:
        break;
      case CKB_SUCCESS:
        if (memcmp(type_hash, hash, 32) == 0) {
          // Found a match;
          if (found_index != SIZE_MAX) {
            // More than one PoA cell exists
            DEBUG("Duplicate PoA cell!");
            return ERROR_ENCODING;
          }
          found_index = current;
        }
        break;
      default:
        running = 0;
        break;
    }
    current++;
  }
  if (found_index == SIZE_MAX) {
    return CKB_INDEX_OUT_OF_BOUND;
  }
  *index = found_index;
  return CKB_SUCCESS;
}

int main() {
  // TODO: cell termination.
  // One CKB transaction can only have one cell using current lock.
  uint64_t len = 0;
  int ret = ckb_load_cell(NULL, &len, 0, 1, CKB_SOURCE_GROUP_INPUT);
  if (ret != CKB_INDEX_OUT_OF_BOUND) {
    DEBUG("Transaction has more than one input cell using current lock!");
    return ERROR_TRANSACTION;
  }
  len = 0;
  ret = ckb_load_cell(NULL, &len, 0, 1, CKB_SOURCE_GROUP_OUTPUT);
  if (ret != CKB_INDEX_OUT_OF_BOUND) {
    DEBUG("Transaction has more than one output cell using current lock!");
    return ERROR_TRANSACTION;
  }

  // Extract signature(s) from the first witness
  uint8_t witness[SIGNATURE_WITNESS_BUFFER_SIZE];
  len = SIGNATURE_WITNESS_BUFFER_SIZE;
  ret = ckb_load_witness(witness, &len, 0, 0, CKB_SOURCE_GROUP_INPUT);
  if (ret != CKB_SUCCESS) {
    return ret;
  }
  size_t readed_len = len;
  if (readed_len > SIGNATURE_WITNESS_BUFFER_SIZE) {
    readed_len = SIGNATURE_WITNESS_BUFFER_SIZE;
  }
  // Assuming the witness is in WitnessArgs structure, we are doing some
  // shortcuts here to support bigger witness.
  if (readed_len < 20) {
    DEBUG("Invalid witness length!");
    return ERROR_ENCODING;
  }
  uint32_t lock_length = *((uint32_t *)(&witness[16]));
  if (readed_len < 20 + lock_length) {
    DEBUG("Witness lock part is far tooooo long!");
    return ERROR_ENCODING;
  }
  // The lock field in WitnessArgs for current PoA script, contains a variable
  // length signature.
  const uint8_t *signature = &witness[20];
  size_t signature_size = lock_length;
  size_t remaining_offset = 20 + lock_length;

  // Prepare signing message for signature validation.
  // Different from our current scripts, this PoA script will actually skip
  // the signature part when hashing for signing message, instead of filling
  // the signature with all zeros.
  uint8_t message[32];
  {
    blake2b_state message_ctx;
    blake2b_init(&message_ctx, 32);
    // Hash current transaction first.
    unsigned char tx_hash[32];
    len = 32;
    ret = ckb_load_tx_hash(tx_hash, &len, 0);
    if (ret != CKB_SUCCESS) {
      DEBUG("Error loading transaction hash");
      return ret;
    }
    if (len != 32) {
      DEBUG("Transaction hash is not 32 bytes!");
      return ERROR_TRANSACTION;
    }
    blake2b_update(&message_ctx, tx_hash, 32);
    blake2b_update(&message_ctx, witness, 22);
    // If we have loaded some witness parts that are after the signature, we
    // will try to use them.
    if (remaining_offset < readed_len) {
      blake2b_update(&message_ctx, &witness[remaining_offset],
                     readed_len - remaining_offset);
      remaining_offset = readed_len;
    }
    if (remaining_offset < len) {
      ret = load_and_hash_witness(&message_ctx, remaining_offset, 0,
                                  CKB_SOURCE_GROUP_INPUT);
      if (ret != CKB_SUCCESS) {
        return ret;
      }
    }
    // Digest same group witnesses
    size_t i = 1;
    while (1) {
      int ret =
          load_and_hash_witness(&message_ctx, 0, i, CKB_SOURCE_GROUP_INPUT);
      if (ret == CKB_INDEX_OUT_OF_BOUND) {
        break;
      }
      if (ret != CKB_SUCCESS) {
        return ret;
      }
      i += 1;
    }
    // Digest witnesses that not covered by inputs
    i = ckb_calculate_inputs_len();
    while (1) {
      int ret = load_and_hash_witness(&message_ctx, 0, i, CKB_SOURCE_INPUT);
      if (ret == CKB_INDEX_OUT_OF_BOUND) {
        break;
      }
      if (ret != CKB_SUCCESS) {
        return ret;
      }
      i += 1;
    }
    blake2b_final(&message_ctx, message, 32);
  }

  // Load current script so as to extract PoA cell information
  unsigned char script[SCRIPT_BUFFER_SIZE];
  len = SCRIPT_BUFFER_SIZE;
  ret = ckb_checked_load_script(script, &len, 0);
  if (ret != CKB_SUCCESS) {
    return ret;
  }

  mol_seg_t script_seg;
  script_seg.ptr = (uint8_t *)script;
  script_seg.size = len;
  if (MolReader_Script_verify(&script_seg, false) != MOL_OK) {
    DEBUG("molecule verification failure!");
    return ERROR_ENCODING;
  }
  mol_seg_t args_seg = MolReader_Script_get_args(&script_seg);
  mol_seg_t args_bytes_seg = MolReader_Bytes_raw_bytes(&args_seg);

  if (args_bytes_seg.size != 64) {
    DEBUG("Script args must be 64 bytes long!");
    return ERROR_ENCODING;
  }

  size_t dep_poa_setup_cell_index = SIZE_MAX;
  ret = look_for_poa_cell(args_bytes_seg.ptr, CKB_SOURCE_CELL_DEP,
                          &dep_poa_setup_cell_index);
  if (ret == CKB_SUCCESS) {
    // Normal new blocks
    uint8_t dep_poa_setup_buffer[POA_BUFFER_SIZE];
    uint64_t len = POA_BUFFER_SIZE;
    ret = ckb_load_cell_data(dep_poa_setup_buffer, &len, 0,
                             dep_poa_setup_cell_index, CKB_SOURCE_CELL_DEP);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    if (len > POA_BUFFER_SIZE) {
      DEBUG("Dep PoA cell is too large!");
      return ERROR_ENCODING;
    }
    PoASetup poa_setup;
    ret = parse_poa_setup(dep_poa_setup_buffer, len, &poa_setup);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    ret =
        initialize_signature_library(poa_setup.code_hash, poa_setup.hash_type);
    if (ret != CKB_SUCCESS) {
      return ret;
    }

    size_t input_poa_data_cell_index = SIZE_MAX;
    ret = look_for_poa_cell(&args_bytes_seg.ptr[32], CKB_SOURCE_INPUT,
                            &input_poa_data_cell_index);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    uint8_t input_poa_data_buffer[22];
    len = 22;
    ret = ckb_load_cell_data(input_poa_data_buffer, &len, 0,
                             input_poa_data_cell_index, CKB_SOURCE_INPUT);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    if (len != 22) {
      DEBUG("Invalid input poa data cell!");
      return ERROR_ENCODING;
    }
    const uint8_t *last_subblock_info = input_poa_data_buffer;

    size_t output_poa_data_cell_index = SIZE_MAX;
    ret = look_for_poa_cell(&args_bytes_seg.ptr[32], CKB_SOURCE_OUTPUT,
                            &output_poa_data_cell_index);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    uint8_t output_poa_data_buffer[22];
    len = 22;
    ret = ckb_load_cell_data(output_poa_data_buffer, &len, 0,
                             output_poa_data_cell_index, CKB_SOURCE_OUTPUT);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    if (len != 22) {
      DEBUG("Invalid output poa data cell!");
      return ERROR_ENCODING;
    }
    const uint8_t *current_subblock_info = output_poa_data_buffer;

    // Check that current aggregator is indeed due to issuing new block.
    uint64_t last_round_initial_subtime = *((uint64_t *)last_subblock_info);
    uint64_t last_subblock_subtime = *((uint64_t *)(&last_subblock_info[8]));
    uint32_t last_block_index = *((uint32_t *)(&last_subblock_info[16]));
    uint16_t last_aggregator_index = *((uint16_t *)(&last_subblock_info[20]));

    uint64_t current_round_initial_subtime =
        *((uint64_t *)current_subblock_info);
    uint64_t current_subblock_subtime =
        *((uint64_t *)(&current_subblock_info[8]));
    uint32_t current_subblock_index =
        *((uint32_t *)(&current_subblock_info[16]));
    uint16_t current_aggregator_index =
        *((uint16_t *)(&current_subblock_info[20]));
    if (current_aggregator_index >= poa_setup.aggregator_number) {
      DEBUG("Invalid aggregator index!");
      return ERROR_ENCODING;
    }

    // Since is used to ensure aggregators wait till the correct time.
    uint64_t since = 0;
    len = 8;
    ret =
        ckb_load_input_by_field(((uint8_t *)&since), &len, 0, 0,
                                CKB_SOURCE_GROUP_INPUT, CKB_INPUT_FIELD_SINCE);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    if (len != 8) {
      DEBUG("Invalid loading since!");
      return ERROR_ENCODING;
    }
    if (poa_setup.interval_uses_seconds) {
      if (since >> 56 != 0x40) {
        DEBUG("PoA requires absolute timestamp since!");
        return ERROR_ENCODING;
      }
    } else {
      if (since >> 56 != 0) {
        DEBUG("PoA requires absolute block number since!");
        return ERROR_ENCODING;
      }
    }
    since &= 0x00FFFFFFFFFFFFFF;
    if (current_subblock_subtime != since) {
      DEBUG("Invalid current time!");
      return ERROR_ENCODING;
    }

    // There are 2 supporting modes:
    // 1. An aggregator can issue as much new blocks as it wants as long as
    // subblock_intervals requirement is met.
    // 2. When the subblock_intervals duration has passed, the next aggregator
    // should now be able to issue more blocks.
    if (since < last_round_initial_subtime + poa_setup.subblock_intervals) {
      // Current aggregator is issuing blocks
      if (current_round_initial_subtime != last_round_initial_subtime) {
        DEBUG("Invalid current round first timestamp!");
        return ERROR_ENCODING;
      }
      // Timestamp must be non-decreasing
      if (current_subblock_subtime < last_subblock_subtime) {
        DEBUG("Invalid current timestamp!");
        return ERROR_ENCODING;
      }
      if (current_aggregator_index != last_aggregator_index) {
        DEBUG("Invalid aggregator!");
        return ERROR_ENCODING;
      }
      if ((current_subblock_index != last_block_index + 1) ||
          (current_subblock_index >= poa_setup.subblocks_per_interval)) {
        DEBUG("Invalid block index");
        return ERROR_ENCODING;
      }
    } else {
      if (current_round_initial_subtime != current_subblock_subtime) {
        DEBUG("Invalid current round first timestamp!");
        return ERROR_ENCODING;
      }
      if (current_subblock_index != 0) {
        DEBUG("Invalid block index");
        return ERROR_ENCODING;
      }
      // Next aggregator in place
      uint64_t duration = (((uint64_t)current_aggregator_index +
                            (uint64_t)poa_setup.aggregator_number -
                            (uint64_t)last_aggregator_index) %
                           (uint64_t)poa_setup.aggregator_number) *
                          ((uint64_t)poa_setup.subblock_intervals);
      if (since < duration + last_round_initial_subtime) {
        DEBUG("Invalid time!");
        return ERROR_ENCODING;
      }
    }

    return validate_signature(
        signature, signature_size,
        &poa_setup.identities[(size_t)current_aggregator_index *
                              (size_t)poa_setup.identity_size],
        poa_setup.identity_size, message);
  } else if (ret == CKB_INDEX_OUT_OF_BOUND) {
    // PoA consensus mode
    size_t input_poa_setup_cell_index = SIZE_MAX;
    ret = look_for_poa_cell(args_bytes_seg.ptr, CKB_SOURCE_INPUT,
                            &input_poa_setup_cell_index);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    uint8_t input_poa_setup_buffer[POA_BUFFER_SIZE];
    uint64_t input_poa_setup_len = POA_BUFFER_SIZE;
    ret = ckb_load_cell_data(input_poa_setup_buffer, &input_poa_setup_len, 0,
                             input_poa_setup_cell_index, CKB_SOURCE_INPUT);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    if (input_poa_setup_len > POA_BUFFER_SIZE) {
      DEBUG("Input PoA cell is too large!");
      return ERROR_ENCODING;
    }
    PoASetup poa_setup;
    ret = parse_poa_setup(input_poa_setup_buffer, input_poa_setup_len,
                          &poa_setup);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    ret =
        initialize_signature_library(poa_setup.code_hash, poa_setup.hash_type);
    if (ret != CKB_SUCCESS) {
      return ret;
    }

    size_t output_poa_setup_cell_index = SIZE_MAX;
    ret = look_for_poa_cell(args_bytes_seg.ptr, CKB_SOURCE_OUTPUT,
                            &output_poa_setup_cell_index);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    uint8_t output_poa_setup_buffer[POA_BUFFER_SIZE];
    uint64_t output_poa_setup_len = POA_BUFFER_SIZE;
    ret = ckb_load_cell_data(output_poa_setup_buffer, &output_poa_setup_len, 0,
                             output_poa_setup_cell_index, CKB_SOURCE_OUTPUT);
    if (ret != CKB_SUCCESS) {
      return ret;
    }
    if (output_poa_setup_len > POA_BUFFER_SIZE) {
      DEBUG("Output PoA cell is too large!");
      return ERROR_ENCODING;
    }
    PoASetup new_poa_setup;
    ret = parse_poa_setup(output_poa_setup_buffer, output_poa_setup_len,
                          &new_poa_setup);
    if (ret != CKB_SUCCESS) {
      return ret;
    }

    size_t single_signature_size =
        signature_size / (size_t)poa_setup.aggregator_change_threshold;
    if ((size_t)poa_setup.aggregator_change_threshold * single_signature_size !=
        signature_size) {
      DEBUG("Invalid signature length!");
      return ERROR_ENCODING;
    }
    return validate_signatures(signature, single_signature_size,
                               poa_setup.aggregator_change_threshold,
                               poa_setup.identities, poa_setup.identity_size,
                               poa_setup.aggregator_number, message);
  }
  // Error
  return ERROR_ENCODING;
}
