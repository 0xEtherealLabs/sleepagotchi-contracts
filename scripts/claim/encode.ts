/**
 * Instruction and account encoding straight from the committed IDL.
 *
 * The IDL carries every discriminator, and both programs use only fixed-size
 * primitives plus one `Vec<[u8; 32]>`, so this is a few lines of Borsh rather than
 * a dependency on the Anchor TS client.
 */

import { PublicKey } from "@solana/web3.js";

import airdropIdl from "../../idl/sleepagotchi_airdrop.json" with { type: "json" };
import claimIdl from "../../idl/sleepagotchi_claim.json" with { type: "json" };

type Idl = {
  instructions: { name: string; discriminator: number[] }[];
  accounts: { name: string; discriminator: number[] }[];
  types?: { name: string; type: { fields?: { name: string; type: unknown }[] } }[];
};

export type { Idl };

export const IDLS = {
  airdrop: airdropIdl as unknown as Idl,
  claim: claimIdl as unknown as Idl,
} as const;

export function discriminator(idl: Idl, instruction: string): Buffer {
  const found = idl.instructions.find(({ name }) => name === instruction);
  if (!found) {
    throw new Error(`${instruction} is not in the IDL`);
  }

  return Buffer.from(found.discriminator);
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

export function vecOf32(items: Buffer[]): Buffer {
  const len = Buffer.alloc(4);
  len.writeUInt32LE(items.length);

  return Buffer.concat([len, ...items]);
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

  bytes32(): Buffer {
    return this.take(32);
  }

  u64(): bigint {
    return this.take(8).readBigUInt64LE();
  }

  i64(): bigint {
    return this.take(8).readBigInt64LE();
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
