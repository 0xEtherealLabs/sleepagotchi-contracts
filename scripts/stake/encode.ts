/**
 * Instruction and account encoding straight from the committed IDL.
 *
 * The IDL carries every discriminator, and the program uses only fixed-size
 * primitives, so this is a few lines of Borsh rather than a dependency on the
 * Anchor TS client.
 */

import { PublicKey, TransactionInstruction } from "@solana/web3.js";

import idl from "../../idl/sleepagotchi_stake.json" with { type: "json" };

interface IdlAccount {
  name: string;
  writable?: boolean;
  signer?: boolean;
  /** Pinned in the program — the token, ATA and system programs. */
  address?: string;
}

type Idl = {
  address: string;
  instructions: { name: string; discriminator: number[]; accounts: IdlAccount[] }[];
  accounts: { name: string; discriminator: number[] }[];
  types?: { name: string; type: { fields: { name: string; type: unknown }[] } }[];
};

export const IDL = idl as unknown as Idl;

export const PROGRAM = new PublicKey(IDL.address);

function instructionOf(name: string) {
  const found = IDL.instructions.find((entry) => entry.name === name);
  if (!found) {
    throw new Error(`${name} is not in the IDL`);
  }

  return found;
}

export function discriminator(name: string): Buffer {
  return Buffer.from(instructionOf(name).discriminator);
}

/**
 * Builds an instruction from the IDL rather than a hand-written key array.
 *
 * The IDL already carries account order and the writable/signer flags, and pins
 * the addresses of the token, ATA and system programs — so a key array here
 * would be a second copy of all of it, free to drift from the program that a
 * `cargo test` never touches. Accounts are supplied by name; a missing or
 * misspelled one fails before anything is signed rather than as an opaque
 * `ConstraintSeeds` on chain.
 */
export function instruction(
  name: string,
  accounts: Record<string, PublicKey>,
  args: Buffer = Buffer.alloc(0),
): TransactionInstruction {
  const declared = instructionOf(name);

  const keys = declared.accounts.map((account) => {
    const pubkey = accounts[account.name] ?? (account.address && new PublicKey(account.address));
    if (!pubkey) {
      throw new Error(`${name}: no account supplied for "${account.name}"`);
    }

    return {
      pubkey,
      isSigner: account.signer ?? false,
      isWritable: account.writable ?? false,
    };
  });

  const unknown = Object.keys(accounts).filter(
    (supplied) => !declared.accounts.some((account) => account.name === supplied),
  );
  if (unknown.length > 0) {
    throw new Error(`${name}: not an account of this instruction: ${unknown.join(", ")}`);
  }

  return new TransactionInstruction({
    programId: PROGRAM,
    keys,
    data: Buffer.concat([Buffer.from(declared.discriminator), args]),
  });
}

export function u16(value: number): Buffer {
  const buf = Buffer.alloc(2);
  buf.writeUInt16LE(value);

  return buf;
}

export function u32(value: number): Buffer {
  const buf = Buffer.alloc(4);
  buf.writeUInt32LE(value);

  return buf;
}

export function u64(value: bigint): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigUInt64LE(value);

  return buf;
}

export function i64(value: bigint): Buffer {
  const buf = Buffer.alloc(8);
  buf.writeBigInt64LE(value);

  return buf;
}

export interface SeasonParams {
  startTs: bigint;
  endTs: bigint;
  rewardPool: bigint;
  maxTotalStaked: bigint;
  maxPerWallet: bigint;
  maxAprBps: number;
  maxMultiplierBps: number;
  sweepDelaySeconds: number;
}

/** Field order must match `SeasonParams` in state.rs. */
export function seasonParams(params: SeasonParams): Buffer {
  return Buffer.concat([
    i64(params.startTs),
    i64(params.endTs),
    u64(params.rewardPool),
    u64(params.maxTotalStaked),
    u64(params.maxPerWallet),
    u16(params.maxAprBps),
    u32(params.maxMultiplierBps),
    u32(params.sweepDelaySeconds),
  ]);
}

/** Skips the 8-byte discriminator, then reads fields in declaration order. */
export class Reader {
  private offset = 8;

  constructor(private readonly data: Buffer) {}

  pubkey(): PublicKey {
    return new PublicKey(this.take(32));
  }

  /**
   * Borsh writes an `Option` as a one-byte tag, then the value only when set —
   * so `None` is one byte, not thirty-three. Reading it as a fixed width would
   * shift every field after it.
   */
  optionPubkey(): PublicKey | null {
    return this.bool() ? this.pubkey() : null;
  }

  u128(): bigint {
    const bytes = this.take(16);

    return bytes.readBigUInt64LE(0) | (bytes.readBigUInt64LE(8) << 64n);
  }

  u64(): bigint {
    return this.take(8).readBigUInt64LE();
  }

  i64(): bigint {
    return this.take(8).readBigInt64LE();
  }

  u32(): number {
    return this.take(4).readUInt32LE();
  }

  u16(): number {
    return this.take(2).readUInt16LE();
  }

  bool(): boolean {
    return this.take(1)[0] === 1;
  }

  u8(): number {
    return this.take(1)[0];
  }

  private take(n: number): Buffer {
    const slice = this.data.subarray(this.offset, this.offset + n);
    if (slice.length !== n) {
      throw new Error(`account data ended early at offset ${this.offset}`);
    }
    this.offset += n;

    return slice;
  }
}
