/**
 * Validates the hand-rolled encoding against the committed IDL. Offline.
 *
 *   pnpm check
 *
 * `encode.ts` and the readers in `common.ts` write and read Borsh by hand rather
 * than depending on the Anchor TS client, so they are a second copy of each
 * program's account layout. This is what stops that copy drifting.
 *
 * It exists because the copy did drift: `pending_admin` was added to both
 * `Config` accounts and the readers were not updated, which silently shifted
 * every field after it — a devnet harness reported a claim window in the year
 * 16466 and the wrong signing key, and nothing failed until a human read it.
 */

import { PublicKey } from "@solana/web3.js";

import { readAirdropConfig, readClaimConfig } from "./common.js";
import { type Idl, IDLS } from "./encode.js";

type Field = { name: string; type: unknown };

const typeOf = (idl: Idl, name: string): Field[] => {
  const found = idl.types?.find((entry) => entry.name === name);
  if (!found) {
    throw new Error(`${name} is not in the IDL`);
  }

  return found.type.fields ?? [];
};

let failures = 0;
const check = (label: string, actual: unknown, expected: unknown) => {
  const ok = JSON.stringify(actual) === JSON.stringify(expected);
  console.log(`${ok ? "ok  " : "FAIL"}  ${label}`);
  if (!ok) {
    console.log(`        expected ${JSON.stringify(expected)}`);
    console.log(`        actual   ${JSON.stringify(actual)}`);
    failures += 1;
  }
};

// The readers walk these in declaration order after the 8-byte discriminator, so
// a reordered, added or resized field shifts every one after it.
check(
  "airdrop Config field order",
  ["admin", "pending_admin", "root", "mint", "start_ts", "end_ts", "paused", "bump"],
  typeOf(IDLS.airdrop, "Config").map((field) => field.name),
);

check(
  "claim Config field order",
  ["admin", "pending_admin", "claim_signer", "mint", "paused", "bump"],
  typeOf(IDLS.claim, "Config").map((field) => field.name),
);

check(
  "airdrop Receipt is empty",
  typeOf(IDLS.airdrop, "Receipt").map((field) => field.name),
  [],
);

check(
  "claim Receipt field order",
  ["claimed"],
  typeOf(IDLS.claim, "Receipt").map((field) => field.name),
);

// `pending_admin` is the only variable-width field in either account: Borsh
// writes `None` as one byte and `Some` as thirty-three. Field order alone cannot
// catch a fixed-width read, so both configs round-trip through the real reader.
const admin = PublicKey.unique();
const mint = PublicKey.unique();
const discriminator = Buffer.alloc(8);

const i64 = (value: bigint): Buffer => {
  const out = Buffer.alloc(8);
  out.writeBigInt64LE(value);

  return out;
};

const option = (key: PublicKey | null): Buffer =>
  key ? Buffer.concat([Buffer.from([1]), key.toBuffer()]) : Buffer.from([0]);

for (const [label, pendingAdmin] of [
  ["none", null],
  ["some", PublicKey.unique()],
] as const) {
  const root = Buffer.alloc(32, 7);
  const airdrop = readAirdropConfig(
    Buffer.concat([
      discriminator,
      admin.toBuffer(),
      option(pendingAdmin),
      root,
      mint.toBuffer(),
      i64(1_700_000_000n),
      i64(1_800_000_000n),
      Buffer.from([1]),
      Buffer.from([254]),
    ]),
  );

  check(
    `airdrop Config round-trips a ${label} pending_admin`,
    {
      ...airdrop,
      admin: airdrop.admin.toBase58(),
      pendingAdmin: airdrop.pendingAdmin?.toBase58() ?? null,
      mint: airdrop.mint.toBase58(),
    },
    {
      admin: admin.toBase58(),
      pendingAdmin: pendingAdmin?.toBase58() ?? null,
      root: root.toString("hex"),
      mint: mint.toBase58(),
      startTs: 1_700_000_000,
      endTs: 1_800_000_000,
      paused: true,
      bump: 254,
    },
  );

  const claimSigner = PublicKey.unique();
  const claim = readClaimConfig(
    Buffer.concat([
      discriminator,
      admin.toBuffer(),
      option(pendingAdmin),
      claimSigner.toBuffer(),
      mint.toBuffer(),
      Buffer.from([0]),
      Buffer.from([253]),
    ]),
  );

  check(
    `claim Config round-trips a ${label} pending_admin`,
    {
      ...claim,
      admin: claim.admin.toBase58(),
      pendingAdmin: claim.pendingAdmin?.toBase58() ?? null,
      claimSigner: claim.claimSigner.toBase58(),
      mint: claim.mint.toBase58(),
    },
    {
      admin: admin.toBase58(),
      pendingAdmin: pendingAdmin?.toBase58() ?? null,
      claimSigner: claimSigner.toBase58(),
      mint: mint.toBase58(),
      paused: false,
      bump: 253,
    },
  );
}

// Every instruction the scripts build must exist on its program.
for (const [program, names] of [
  ["airdrop", ["initialize", "claim", "set_root", "set_window", "withdraw"]],
  ["claim", ["initialize", "claim", "withdraw"]],
] as const) {
  for (const name of names) {
    const found = IDLS[program].instructions.find((entry) => entry.name === name);
    check(`${program}: ${name} is in the IDL`, Boolean(found), true);
  }
}

console.log(failures === 0 ? "\nencoding matches the IDL." : `\n${failures} mismatch(es).`);
process.exit(failures === 0 ? 0 : 1);
